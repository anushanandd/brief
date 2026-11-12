//! Older copies of the finance database, in `backups/` beside it, so one bad day leaves a way
//! back: a newer build that migrated the database, or a mistake the rolling backup has already
//! copied. The first durable write of each local day keeps a dated copy of the database as the
//! day began (the newest `DAILY_KEPT` stay), and a database older than this build's schema is
//! copied at its own version before it is migrated (the newest `PRE_MIGRATION_KEPT` versions
//! stay), so the build that wrote it can still open it.
//!
//! Each copy is written under a scratch name through SQLite's Backup API, validated, and only
//! then renamed into place, so a half-written copy never carries a real name and no file a
//! connection holds is renamed. Names carry the date or the version, so retention reads neither
//! the clock nor file times. Recovery reads every copy here, the rolling backup, and the copies
//! a release kept (`releases/`), each through a scratch copy so reading never migrates one.
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::NaiveDate;

use super::backup::Scratch;
use crate::database::{Database, DATABASE_SCHEMA_VERSION};

/// The folder of dated and pre-migration copies, in the app's data folder.
pub(crate) const DIRECTORY: &str = "backups";
/// The folder `scripts/release.sh` keeps database copies in before installing a build.
pub(crate) const RELEASES: &str = "releases";
const DAILY_KEPT: usize = 7;
const PRE_MIGRATION_KEPT: usize = 3;
const PREFIX: &str = "finance-state.";
const SUFFIX: &str = ".sqlite3";

/// The backups folder, made private to this user.
fn directory(data: &Path) -> Result<PathBuf, String> {
    let directory = data.join(DIRECTORY);
    fs::create_dir_all(&directory).map_err(|_| "Could not create the backups folder")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect the backups folder")?;
    }
    Ok(directory)
}

fn daily_name(day: NaiveDate) -> String {
    format!("{PREFIX}{}{SUFFIX}", day.format("%Y-%m-%d"))
}

/// The day a dated copy's name holds.
fn daily(name: &str) -> Option<NaiveDate> {
    let day = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    (day.len() == 10)
        .then(|| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .flatten()
}

fn pre_migration_name(version: u32) -> String {
    format!("{PREFIX}pre-v{version}{SUFFIX}")
}

/// The schema version a pre-migration copy's name holds.
fn pre_migration(name: &str) -> Option<u32> {
    name.strip_prefix(PREFIX)?
        .strip_prefix("pre-v")?
        .strip_suffix(SUFFIX)?
        .parse()
        .ok()
}

/// The files in `directory`, by name.
fn entries(directory: &Path) -> Vec<(String, PathBuf)> {
    fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        entry.path(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Writes a copy with `write` under a scratch name, validates it, then gives it `destination`'s
/// name, replacing a closed earlier copy of that name.
fn install(
    destination: &Path,
    write: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let scratch = Scratch::new(destination.with_extension(format!("{}.tmp", uuid::Uuid::new_v4())));
    write(scratch.path())?;
    Database::validate_backup(scratch.path())?;
    fs::rename(scratch.path(), destination)
        .map_err(|_| "Could not keep a copy of the finance database")?;
    scratch.keep();
    crate::database::secure_file(destination)
}

/// Removes all but the `kept` copies whose `key` is greatest.
fn prune<K: Ord>(
    directory: &Path,
    key: impl Fn(&str) -> Option<K>,
    kept: usize,
) -> Result<(), String> {
    let mut copies = entries(directory)
        .into_iter()
        .filter_map(|(name, path)| key(&name).map(|key| (key, path)))
        .collect::<Vec<_>>();
    copies.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in copies.into_iter().skip(kept) {
        fs::remove_file(path)
            .map_err(|_| "Could not remove an old copy of the finance database")?;
    }
    Ok(())
}

/// Keeps a copy of `database` dated `today` unless one exists, then only the newest
/// `DAILY_KEPT`. Called before each durable write, so the copy holds the day's first state.
pub(super) fn keep_daily(database: &Database, data: &Path, today: NaiveDate) -> Result<(), String> {
    let directory = directory(data)?;
    let path = directory.join(daily_name(today));
    if path.exists() {
        return Ok(());
    }
    install(&path, |scratch| database.backup(scratch))?;
    prune(&directory, daily, DAILY_KEPT)
}

/// Why an older database wasn't copied before its migration.
#[derive(Debug)]
pub(super) enum NotKept {
    /// The file can't be read as a database.
    Unreadable,
    /// It reads, but the copy couldn't be written, so it isn't migrated.
    CopyFailed(String),
}

/// Before the database at `path` is migrated, keeps a copy at its own schema version. Returns
/// the version kept, or `None` when the database is already current (or holds no Brief state).
/// A copy that can't be kept stops the migration; trimming older copies never does.
pub(super) fn keep_before_migration(path: &Path, data: &Path) -> Result<Option<u32>, NotKept> {
    let Some(version) = Database::schema_version(path)
        .map_err(|_| NotKept::Unreadable)?
        .filter(|version| *version < DATABASE_SCHEMA_VERSION)
    else {
        return Ok(None);
    };
    let directory = directory(data).map_err(NotKept::CopyFailed)?;
    install(&directory.join(pre_migration_name(version)), |scratch| {
        Database::copy_backup(path, scratch)
    })
    .map_err(NotKept::CopyFailed)?;
    let _ = prune(&directory, pre_migration, PRE_MIGRATION_KEPT);
    Ok(Some(version))
}

/// A copy recovery may restore from, and how to name it to the user.
pub(super) struct Candidate {
    pub path: PathBuf,
    pub label: String,
    modified: Option<SystemTime>,
}

/// Every copy recovery may restore from: the rolling backup, the dated and pre-migration
/// copies, and those a release kept, newest written first. Which of them this build can read
/// is for the reader to find out.
pub(super) fn candidates(data: &Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    let mut add = |path: PathBuf, label: String| {
        if path.is_file() {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok();
            found.push(Candidate {
                path,
                label,
                modified,
            });
        }
    };
    add(
        data.join("finance-state.backup.sqlite3"),
        "the copy saved before the latest change".into(),
    );
    for (name, path) in entries(&data.join(DIRECTORY)) {
        if let Some(day) = daily(&name) {
            add(path, format!("the daily copy from {day}"));
        } else if let Some(version) = pre_migration(&name) {
            add(
                path,
                format!("the copy kept before an upgrade from database version {version}"),
            );
        }
    }
    for (name, folder) in entries(&data.join(RELEASES)) {
        if name.starts_with("pre-release-") {
            for file in ["finance-state.sqlite3", "finance-state.backup.sqlite3"] {
                add(
                    folder.join(file),
                    format!("the copy a release kept in {name}"),
                );
            }
        }
    }
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    found
}

/// Removes every copy in `backups/` and `releases/`, and each folder once empty. Only files
/// named as finance database copies (and their scratch files) go; anything else stays.
pub(super) fn erase(data: &Path) -> Result<(), String> {
    let remove = |folder: &Path| -> Result<(), String> {
        for (name, path) in entries(folder) {
            if name.starts_with(PREFIX) && path.is_file() {
                fs::remove_file(path).map_err(|_| "Could not remove retained financial data")?;
            }
        }
        // A folder still holding something Brief didn't write stays.
        let _ = fs::remove_dir(folder);
        Ok(())
    };
    remove(&data.join(DIRECTORY))?;
    let releases = data.join(RELEASES);
    for (name, folder) in entries(&releases) {
        if name.starts_with("pre-release-") && folder.is_dir() {
            remove(&folder)?;
        }
    }
    let _ = fs::remove_dir(releases);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::change::Reason;

    fn folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!("brief-generations-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn eight_days_of_writes_keep_the_newest_seven_dated_copies() {
        let data = folder();
        let mut database = Database::open(&data.join("finance-state.sqlite3")).unwrap();
        let mut state = crate::storage::empty_state().unwrap();
        for (index, date) in (1..=8).map(|n| format!("2026-11-{n:02}")).enumerate() {
            state.revision = index as u64;
            database.checkpoint(&state, Reason::Start).unwrap();
            keep_daily(&database, &data, day(&date)).unwrap();
            // A second write the same day keeps the copy the day began with.
            state.revision += 100;
            database.checkpoint(&state, Reason::Start).unwrap();
            keep_daily(&database, &data, day(&date)).unwrap();
        }
        let mut kept = entries(&data.join(DIRECTORY))
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        kept.sort();
        assert_eq!(
            kept,
            (2..=8)
                .map(|n| format!("finance-state.2026-11-{n:02}.sqlite3"))
                .collect::<Vec<_>>()
        );
        let newest = Database::open(&data.join(DIRECTORY).join(daily_name(day("2026-11-08"))))
            .unwrap()
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(newest.revision, 7, "the copy holds the day's first state");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&data.join(DIRECTORY)), 0o700);
            assert_eq!(
                mode(&data.join(DIRECTORY).join(daily_name(day("2026-11-08")))),
                0o600
            );
        }
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn names_read_back_and_strangers_are_neither_pruned_nor_erased() {
        assert_eq!(
            daily("finance-state.2026-11-11.sqlite3"),
            Some(day("2026-11-11"))
        );
        assert_eq!(daily("finance-state.2026-11-11.abc.tmp"), None);
        assert_eq!(daily("finance-state.backup.sqlite3"), None);
        assert_eq!(pre_migration("finance-state.pre-v22.sqlite3"), Some(22));
        assert_eq!(pre_migration("finance-state.pre-vX.sqlite3"), None);

        let data = folder();
        let backups = directory(&data).unwrap();
        let stranger = backups.join("notes.txt");
        fs::write(&stranger, b"not Brief's").unwrap();
        for version in [18, 19, 20, 21, 22] {
            fs::write(backups.join(pre_migration_name(version)), b"").unwrap();
        }
        prune(&backups, pre_migration, PRE_MIGRATION_KEPT).unwrap();
        let mut kept = entries(&backups)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        kept.sort();
        assert_eq!(
            kept,
            [
                "finance-state.pre-v20.sqlite3",
                "finance-state.pre-v21.sqlite3",
                "finance-state.pre-v22.sqlite3",
                "notes.txt"
            ]
        );

        let release = data
            .join(RELEASES)
            .join("pre-release-abc1234-20261110-090000");
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("finance-state.sqlite3"), b"copy").unwrap();
        erase(&data).unwrap();
        assert!(stranger.exists(), "a file Brief didn't write stays");
        assert!(!backups.join(pre_migration_name(22)).exists());
        assert!(!data.join(RELEASES).exists());
        fs::remove_dir_all(data).unwrap();
    }
}
