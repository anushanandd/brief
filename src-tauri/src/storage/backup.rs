//! Backups, restore, and recovery. Every durable change backs the database up first through
//! SQLite's Backup API; a database that cannot be read opens read-only beside its retained copy.
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use serde_json::Value;

use super::{
    empty_state, legacy::LEGACY_FINANCE_FILES, validate_state, BackupInfo, FinanceState, Storage,
    EDIT_BACKUP_INTERVAL,
};
use crate::database::Database;

/// A file that is removed when dropped unless kept, so a failed step leaves nothing behind.
struct Scratch {
    path: PathBuf,
    kept: bool,
}

impl Scratch {
    fn new(path: PathBuf) -> Self {
        Self { path, kept: false }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Storage {
    pub(super) fn backup_path(&self) -> PathBuf {
        self.path.with_file_name("finance-state.backup.sqlite3")
    }

    pub fn recover(&mut self, restore: bool) -> Result<Value, String> {
        let recovery = self.recovery.as_ref().ok_or("No recovery is needed")?;
        if restore && !recovery.can_restore {
            return Err("No valid retained snapshot is available".into());
        }
        let next = validate_state(if restore {
            self.data.clone()
        } else {
            empty_state()?
        })?;
        // Retain the original bytes before replacing anything, including a newer-version file.
        for path in [&self.path, &self.backup_path()] {
            if path.exists() {
                let retained =
                    path.with_extension(format!("retained-{}.sqlite3", uuid::Uuid::new_v4()));
                fs::copy(path, retained)
                    .map_err(|_| "Could not retain the original file; no recovery was applied")?;
            }
        }
        let temporary = Scratch::new(
            self.path
                .with_extension(format!("{}.tmp", uuid::Uuid::new_v4())),
        );
        let mut database = Database::open(temporary.path())?;
        database.replace(&next)?;
        drop(database);
        fs::rename(temporary.path(), &self.path).map_err(|error| error.to_string())?;
        self.database = Database::open(&self.path)?;
        self.data = next;
        self.recovery = None;
        self.pending = None;
        // Primary is now valid. A failed backup is safe to retry on the next durable write.
        let _ = self.database.backup(&self.backup_path());
        self.snapshot()
    }

    pub fn erase(&mut self) -> Result<Value, String> {
        self.ensure_writable()?;
        let directory = self
            .path
            .parent()
            .ok_or("Local data directory unavailable")?;
        let mut removable = vec![self.backup_path()];
        removable.extend(LEGACY_FINANCE_FILES.map(|name| directory.join(name)));
        if let Ok(entries) = fs::read_dir(directory) {
            removable.extend(entries.flatten().filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                (name.starts_with("finance-state.retained-") && name.ends_with(".sqlite3"))
                    .then(|| entry.path())
            }));
        }
        for path in removable {
            if path.exists() {
                fs::remove_file(path).map_err(|_| "Could not remove retained financial data")?;
            }
        }

        let mut next = empty_state()?;
        next.revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or("Snapshot revision overflow")?;
        next.snapshot.revision = Some(next.revision);
        next.snapshot.updated_at = chrono::Utc::now().to_rfc3339();
        self.database.erase(&next)?;
        self.data = next;
        self.pending = None;
        self.recovery = None;
        self.snapshot()
    }

    /// Retains the pre-edit database before a small edit. Snapshot commits, restores, and
    /// erasure always back up first; consecutive small edits at one revision reuse a recent copy.
    pub(super) fn backup_before_edit(&mut self) -> Result<(), String> {
        let revision = self.data.revision;
        if self
            .edit_backup
            .is_some_and(|(at, saved)| saved == revision && at.elapsed() < EDIT_BACKUP_INTERVAL)
        {
            return Ok(());
        }
        self.database.backup(&self.backup_path())?;
        self.edit_backup = Some((Instant::now(), revision));
        Ok(())
    }

    pub fn export_backup(&self, destination: &Path) -> Result<(), String> {
        // A new file only: a mistaken selection cannot overwrite a database or any user file.
        drop(crate::files::create_private(destination)?);
        let written = Scratch::new(destination.to_owned());
        self.database.backup(destination)?;
        Database::validate_backup(destination)?;
        written.keep();
        Ok(())
    }

    pub fn inspect_backup(&self, source: &Path) -> Result<BackupInfo, String> {
        let (_temporary, database, state) = self.validated_backup_copy(source)?;
        drop(database);
        Ok(BackupInfo {
            updated_at: state.snapshot.updated_at,
            revision: state.revision,
            schema_version: state.schema_version,
            accounts: state.snapshot.accounts.len(),
            transactions: state.snapshot.transactions.len(),
            holdings: state.snapshot.holdings.len(),
        })
    }

    pub fn import_backup(&mut self, source: &Path) -> Result<(), String> {
        let (temporary, mut database, mut next) = self.validated_backup_copy(source)?;
        next.revision = self
            .data
            .revision
            .max(next.revision)
            .checked_add(1)
            .ok_or("Revision overflow")?;
        next.snapshot.revision = Some(next.revision);
        database.replace(&next)?;
        Database::validate_backup(temporary.path())?;
        let retained = self
            .path
            .with_extension(format!("retained-{}.sqlite3", uuid::Uuid::new_v4()));
        if self.recovery.is_some() && self.path.exists() {
            fs::copy(&self.path, &retained)
                .map_err(|_| "Could not retain the unreadable finance database")?;
        } else {
            self.database.backup(&retained)?;
        }
        self.database.backup(&self.backup_path())?;
        if self.recovery.is_some() {
            drop(database);
            fs::rename(temporary.path(), &self.path).map_err(|_| "Could not install backup")?;
            self.database = Database::open(&self.path)?;
        } else {
            self.database.restore_from(&database)?;
        }
        self.data = next;
        self.pending = None;
        self.recovery = None;
        Ok(())
    }

    /// A validated copy of the backup at `source`, opened from a scratch file removed with it.
    fn validated_backup_copy(
        &self,
        source: &Path,
    ) -> Result<(Scratch, Database, FinanceState), String> {
        let metadata = fs::metadata(source).map_err(|_| "Backup cannot be read")?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err("Backup is empty or is not a file".into());
        }
        if metadata.len() > 512 * 1024 * 1024 {
            return Err("Backup exceeds 512 MB".into());
        }
        let temporary = Scratch::new(
            self.path
                .with_extension(format!("{}.import", uuid::Uuid::new_v4())),
        );
        Database::copy_backup(source, temporary.path())?;
        Database::validate_backup(temporary.path())?;
        let database = Database::open(temporary.path())?;
        let state = validate_state(database.load()?.ok_or("This is not a Brief backup")?)?;
        Ok((temporary, database, state))
    }
}
