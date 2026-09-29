use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use std::io::Write;

use crate::{
    database::{Database, SyncDiagnostics, SyncRun},
    finance_contract::{PerformancePoint, Point, Snapshot},
    providers::{PlaidCache, ProviderDataCache, ProviderSync},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceState {
    pub(crate) schema_version: u32,
    pub revision: u64,
    pub snapshot: Value,
    pub plaid_cache: PlaidCache,
    pub provider_data: ProviderDataCache,
    #[serde(default)]
    pub annotations: BTreeMap<String, Annotation>,
    #[serde(default)]
    pub account_links: BTreeMap<String, String>,
    #[serde(default)]
    pub workspace: crate::workspace::Workspace,
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Annotation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reviewed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) benefit_confirmed: Option<bool>,
}

struct PendingRefresh {
    id: String,
    started_at: String,
    revision: u64,
    expires: Instant,
    plaid_cache: PlaidCache,
    provider_data: ProviderDataCache,
    warnings: Vec<String>,
    diagnostics: SyncDiagnostics,
}

pub(crate) enum CommitError {
    Validation(String),
    Persistence(String),
}

impl CommitError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_failed",
            Self::Persistence(_) => "commit_failed",
        }
    }
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message) | Self::Persistence(message) => formatter.write_str(message),
        }
    }
}

pub struct Storage {
    _process_lock: fs::File,
    path: PathBuf,
    database: Database,
    pub data: FinanceState,
    pending: Option<PendingRefresh>,
    recovery: Option<Recovery>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Recovery {
    message: String,
    can_restore: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub updated_at: String,
    pub revision: u64,
    pub schema_version: u32,
    pub accounts: usize,
    pub transactions: usize,
    pub holdings: usize,
}

pub(crate) fn empty_state() -> Result<FinanceState, String> {
    Ok(FinanceState {
        schema_version: 1,
        revision: 0,
        snapshot: serde_json::from_str(include_str!("../../src/data/empty.json"))
            .map_err(|error| error.to_string())?,
        plaid_cache: PlaidCache::default(),
        provider_data: ProviderDataCache::default(),
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
        workspace: Default::default(),
    })
}

fn validate_state(data: FinanceState) -> Result<FinanceState, String> {
    data.workspace.validate()?;
    if data.annotations.iter().any(|(id, annotation)| {
        id.is_empty()
            || id.len() > 200
            || annotation
                .category
                .as_ref()
                .is_some_and(|category| category.trim().is_empty() || category.len() > 100)
    }) {
        return Err("Invalid saved annotations".into());
    }
    if data.schema_version != 1 {
        return Err("This financial data requires a newer version of Brief".into());
    }
    let mut linked_snaptrade_accounts = BTreeSet::new();
    if data.account_links.iter().any(|(plaid, snaptrade)| {
        !plaid.starts_with("plaid:")
            || !snaptrade.starts_with("snaptrade:")
            || plaid.len() > 220
            || snaptrade.len() > 220
            || !linked_snaptrade_accounts.insert(snaptrade)
    }) {
        return Err("Invalid canonical account links".into());
    }
    validate_snapshot(&data.snapshot)?;
    Ok(data)
}

fn load_database(path: &Path) -> Result<(Database, FinanceState), String> {
    let database = Database::open(path)?;
    let data = database
        .load()?
        .ok_or_else(|| "The finance database has no committed state".to_string())?;
    Ok((database, validate_state(data)?))
}

const LEGACY_FINANCE_FILES: [&str; 5] = [
    "finance-state.json",
    "finance-state.backup.json",
    "finance-snapshot.json",
    "plaid-cache.json",
    "provider-data.json",
];

fn protect_directory(directory: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Could not protect local financial data: {error}"))?;
    }
    for name in [
        "finance-state.sqlite3",
        "finance-state.backup.sqlite3",
        "market-prices.sqlite3",
        "news.sqlite3",
        "startup-market.sqlite3",
    ] {
        crate::database::secure_file(&directory.join(name))?;
    }
    Ok(())
}

fn remove_migrated_legacy_files(directory: &Path) {
    for name in LEGACY_FINANCE_FILES {
        let _ = fs::remove_file(directory.join(name));
    }
}

fn clear_legacy_diagnostics(data: &mut FinanceState) {
    data.snapshot["syncWarnings"] = Value::Array(Vec::new());
    if let Some(statuses) = data.snapshot["providerStatus"].as_object_mut() {
        for status in statuses.values_mut() {
            status["error"] = Value::Null;
        }
    }
    data.plaid_cache.clear_legacy_diagnostics();
    data.provider_data.clear_legacy_diagnostics();
}

fn legacy_state(directory: &Path) -> Result<FinanceState, String> {
    let state_path = directory.join("finance-state.json");
    if state_path.exists() {
        let mut data = read(&state_path)?;
        clear_legacy_diagnostics(&mut data);
        return Ok(data);
    }
    let snapshot_path = directory.join("finance-snapshot.json");
    let mut data = FinanceState {
        schema_version: 1,
        revision: 0,
        snapshot: if snapshot_path.exists() {
            read(&snapshot_path)?
        } else {
            serde_json::from_str(include_str!("../../src/data/empty.json"))
                .map_err(|error| error.to_string())?
        },
        plaid_cache: read_or_default(&directory.join("plaid-cache.json"))?,
        provider_data: read_or_default(&directory.join("provider-data.json"))?,
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
        workspace: Default::default(),
    };
    clear_legacy_diagnostics(&mut data);
    Ok(data)
}

impl Storage {
    pub fn cancel_pending_refresh(&mut self) {
        self.pending = None;
    }

    pub fn sync_runs(&self) -> Result<Vec<SyncRun>, String> {
        self.database.sync_runs()
    }

    pub fn record_failed_sync(
        &mut self,
        id: &str,
        started_at: &str,
        error_code: &str,
        diagnostics: SyncDiagnostics,
    ) {
        let _ = self.database.record_sync_run(
            id,
            started_at,
            &chrono::Utc::now().to_rfc3339(),
            "failed",
            &[],
            Some(error_code),
            &diagnostics,
        );
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        let mut snapshot = crate::financial_engine::apply_annotations(
            &self.data.snapshot,
            &self.data.annotations,
        )?;
        if let Some(recovery) = &self.recovery {
            snapshot["recovery"] = serde_json::to_value(recovery).unwrap_or(Value::Null);
        }
        Ok(snapshot)
    }

    pub fn upgrade_projection(&mut self) -> Result<bool, String> {
        let version = self.data.snapshot["calculationVersion"].as_u64();
        if self.recovery.is_some()
            || self.pending.is_some()
            || version.is_some_and(|value| {
                value >= u64::from(crate::financial_engine::CALCULATION_VERSION)
            })
            || (self.data.provider_data.plaid.is_none()
                && self.data.provider_data.snaptrade.is_none())
        {
            return Ok(false);
        }

        self.reproject_cached(self.data.snapshot.clone())
    }

    fn reproject_cached(&mut self, previous: Value) -> Result<bool, String> {
        let updated_at = previous["updatedAt"]
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
        let now = updated_at
            .as_ref()
            .map(|value| value.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let calendar_date = previous["calendarDate"]
            .as_str()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .or_else(|| updated_at.as_ref().map(|value| value.date_naive()))
            .unwrap_or_else(|| now.date_naive());
        let sync = ProviderSync {
            sync_id: "cached-projection-upgrade".into(),
            balances_fresh: false,
            refreshed_account_ids: Vec::new(),
            previous_snapshot: previous.clone(),
            plaid: self.data.provider_data.plaid.clone().unwrap_or_default(),
            snaptrade: self
                .data
                .provider_data
                .snaptrade
                .clone()
                .unwrap_or_default(),
            benchmark_history: previous
                .get("benchmarkHistory")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
            security_history: self.data.provider_data.security_history.clone(),
        };
        let mut snapshot = serde_json::to_value(crate::financial_engine::project(
            &sync,
            &self.data.account_links,
            now,
            calendar_date,
        )?)
        .map_err(|error| error.to_string())?;
        let revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or("Snapshot revision overflow")?;
        snapshot["revision"] = revision.into();
        snapshot["providerStatus"] = serde_json::to_value(&self.data.provider_data.sync_status)
            .map_err(|error| error.to_string())?;
        if snapshot.get("lastChange").is_none() {
            if let Some(change) = previous.get("lastChange") {
                snapshot["lastChange"] = change.clone();
            }
        }
        let mut warnings = previous
            .get("syncWarnings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .chain(
                snapshot
                    .get("syncWarnings")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            )
            .collect::<Vec<_>>();
        warnings.sort();
        warnings.dedup();
        snapshot["syncWarnings"] =
            serde_json::to_value(warnings).map_err(|error| error.to_string())?;

        let next = validate_state(FinanceState {
            schema_version: self.data.schema_version,
            revision,
            snapshot,
            plaid_cache: self.data.plaid_cache.clone(),
            provider_data: self.data.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
            workspace: self.data.workspace.clone(),
        })?;
        self.database.backup(&self.backup_path())?;
        self.database.replace(&next)?;
        self.data = next;
        self.pending = None;
        Ok(true)
    }

    pub fn ensure_writable(&self) -> Result<(), String> {
        if self.recovery.is_some() {
            Err("Restore or start a new local snapshot before changing saved data".into())
        } else {
            Ok(())
        }
    }

    fn backup_path(&self) -> PathBuf {
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
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let write_result = (|| {
            let mut database = Database::open(&temporary)?;
            database.replace(&next)?;
            drop(database);
            fs::rename(&temporary, &self.path).map_err(|error| error.to_string())?;
            Ok::<_, String>(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result?;
        self.database = Database::open(&self.path)?;
        self.data = next;
        self.recovery = None;
        self.pending = None;
        // Primary is now valid. A failed backup is safe to retry on the next durable write.
        let _ = self.database.backup(&self.backup_path());
        self.snapshot()
    }

    pub fn new(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        protect_directory(&directory)?;
        let lock_path = directory.join("finance-state.lock");
        let process_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| error.to_string())?;
        crate::database::secure_file(&lock_path)?;
        process_lock.try_lock().map_err(|error| {
            format!("Cannot lock financial data; another Brief instance may be open: {error}")
        })?;
        let path = directory.join("finance-state.sqlite3");
        let backup_path = directory.join("finance-state.backup.sqlite3");
        let existing = path.exists();
        let legacy_present = LEGACY_FINANCE_FILES
            .iter()
            .any(|name| directory.join(name).exists());
        let loaded = if existing {
            load_database(&path)
        } else if backup_path.exists() {
            Err("The primary finance database is missing".into())
        } else {
            legacy_state(&directory)
                .and_then(validate_state)
                .and_then(|data| {
                    let mut database = Database::open(&path)?;
                    database.replace(&data)?;
                    let _ = database.backup(&backup_path);
                    Ok((database, data))
                })
        };
        let (database, data, recovery) = match loaded {
            Ok((database, data)) => (database, data, None),
            Err(_) => {
                let backup = if backup_path.exists() {
                    load_database(&backup_path).ok().map(|(_, data)| data)
                } else {
                    read(&directory.join("finance-state.backup.json"))
                        .and_then(validate_state)
                        .ok()
                };
                let recovery = Recovery {
                    message: if backup.is_some() { "The saved database could not be read or requires a newer app version. Showing a retained valid snapshot, read-only, until you choose recovery." } else { "The saved database could not be read or requires a newer app version. No valid backup was found. Original files will be retained if you start a new snapshot." }.into(),
                    can_restore: backup.is_some(),
                };
                (
                    Database::open(Path::new(":memory:"))?,
                    backup.unwrap_or(empty_state()?),
                    Some(recovery),
                )
            }
        };
        if recovery.is_none() && legacy_present && database.backup(&backup_path).is_ok() {
            remove_migrated_legacy_files(&directory);
        }
        Ok(Self {
            _process_lock: process_lock,
            path,
            database,
            data,
            pending: None,
            recovery,
        })
    }

    #[cfg(test)]
    pub fn stage(
        &mut self,
        id: &str,
        started_at: &str,
        revision: u64,
        plaid_cache: PlaidCache,
        provider_data: ProviderDataCache,
        warnings: Vec<String>,
    ) -> Result<String, String> {
        self.stage_with_diagnostics(
            id,
            started_at,
            revision,
            plaid_cache,
            provider_data,
            warnings,
            SyncDiagnostics::default(),
        )
    }

    pub fn stage_with_diagnostics(
        &mut self,
        id: &str,
        started_at: &str,
        revision: u64,
        plaid_cache: PlaidCache,
        provider_data: ProviderDataCache,
        warnings: Vec<String>,
        diagnostics: SyncDiagnostics,
    ) -> Result<String, String> {
        self.ensure_writable()?;
        if revision != self.data.revision {
            return Err("Financial data changed during refresh; refresh again".into());
        }
        self.pending = Some(PendingRefresh {
            id: id.into(),
            started_at: started_at.into(),
            revision,
            plaid_cache,
            provider_data,
            warnings,
            diagnostics,
            expires: Instant::now() + Duration::from_secs(120),
        });
        Ok(id.into())
    }

    pub fn commit(&mut self, id: &str, mut snapshot: Snapshot) -> Result<Value, CommitError> {
        self.ensure_writable().map_err(CommitError::Validation)?;
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| {
                pending.id == id
                    && pending.revision == self.data.revision
                    && pending.expires > Instant::now()
            })
            .ok_or_else(|| {
                CommitError::Validation("Refresh expired or was superseded; refresh again".into())
            })?;
        validate_projected_snapshot(&snapshot).map_err(CommitError::Validation)?;
        let revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or_else(|| CommitError::Validation("Snapshot revision overflow".into()))?;
        snapshot.revision = Some(revision);
        let mut warnings = pending.warnings.clone();
        warnings.extend(snapshot.sync_warnings);
        warnings.sort();
        warnings.dedup();
        snapshot.sync_warnings = warnings.clone();
        snapshot.provider_status = Some(pending.provider_data.sync_status.clone());
        let snapshot = serde_json::to_value(snapshot)
            .map_err(|error| CommitError::Validation(error.to_string()))?;
        let next = FinanceState {
            schema_version: 1,
            revision,
            snapshot: snapshot.clone(),
            plaid_cache: pending.plaid_cache.clone(),
            provider_data: pending.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
            workspace: self.data.workspace.clone(),
        };
        let sync_run_warnings = (!warnings.is_empty())
            .then(|| vec![format!("{} warning(s)", warnings.len())])
            .unwrap_or_default();
        let backup_path = self.backup_path();
        self.database
            .backup(&backup_path)
            .map_err(CommitError::Persistence)?;
        self.database
            .replace(&next)
            .map_err(CommitError::Persistence)?;
        let details = SyncDiagnostics::new(
            "committed",
            pending.diagnostics.providers.clone(),
            warnings.len(),
        );
        let _ = self.database.record_sync_run(
            &pending.id,
            &pending.started_at,
            &chrono::Utc::now().to_rfc3339(),
            "committed",
            &sync_run_warnings,
            None,
            &details,
        );
        self.data = next;
        self.pending = None;
        self.snapshot().map_err(CommitError::Persistence)
    }

    #[cfg(test)]
    fn commit_value(&mut self, id: &str, value: Value) -> Result<Value, String> {
        self.commit(id, crate::finance_contract::decode(value)?)
            .map_err(|error| error.to_string())
    }

    pub fn fail_pending(&mut self, id: &str, error_code: &str) {
        let Some(pending) = self.pending.as_ref().filter(|pending| pending.id == id) else {
            return;
        };
        let details = SyncDiagnostics::new(
            error_code.trim_end_matches("_failed"),
            pending.diagnostics.providers.clone(),
            pending.warnings.len(),
        );
        let _ = self.database.record_sync_run(
            id,
            &pending.started_at,
            &chrono::Utc::now().to_rfc3339(),
            "failed",
            &(!pending.warnings.is_empty())
                .then(|| vec![format!("{} warning(s)", pending.warnings.len())])
                .unwrap_or_default(),
            Some(error_code),
            &details,
        );
        self.pending = None;
    }

    pub fn save_annotations(
        &mut self,
        changes: BTreeMap<String, Annotation>,
        importing: bool,
    ) -> Result<BTreeMap<String, Annotation>, String> {
        if self.recovery.is_some() && importing {
            return Ok(self.data.annotations.clone());
        }
        self.ensure_writable()?;
        let mut next = self.data.annotations.clone();
        for (id, change) in changes {
            if id.is_empty()
                || id.len() > 200
                || change
                    .category
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 100)
            {
                return Err("Invalid transaction annotation".into());
            }
            let entry = next.entry(id).or_default();
            if change.category.is_some() && (!importing || entry.category.is_none()) {
                entry.category = change.category;
            }
            if change.reviewed.is_some() && (!importing || entry.reviewed.is_none()) {
                entry.reviewed = change.reviewed;
            }
            if change.benefit_confirmed.is_some()
                && (!importing || entry.benefit_confirmed.is_none())
            {
                entry.benefit_confirmed = change.benefit_confirmed;
            }
        }
        if next != self.data.annotations {
            self.database.backup(&self.backup_path())?;
            self.database.replace_annotations(&next)?;
            self.data.annotations = next;
        }
        Ok(self.data.annotations.clone())
    }

    #[cfg(test)]
    pub fn save_workspace(&mut self, workspace: crate::workspace::Workspace) -> Result<(), String> {
        self.ensure_writable()?;
        workspace.validate()?;
        if self.data.workspace != workspace {
            self.database.backup(&self.backup_path())?;
            self.database.replace_workspace(&workspace)?;
            self.data.workspace = workspace;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn review_transaction(
        &mut self,
        id: &str,
        annotation: Option<Annotation>,
        create_rule: bool,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        let transaction = self.data.snapshot["transactions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["id"].as_str() == Some(id))
            .ok_or("Transaction is no longer available")?;
        if id.is_empty()
            || id.len() > 200
            || annotation
                .as_ref()
                .and_then(|a| a.category.as_ref())
                .is_some_and(|category| category.trim().is_empty() || category.len() > 100)
        {
            return Err("Invalid transaction review".into());
        }
        let mut workspace = self.data.workspace.clone();
        if create_rule {
            let category = annotation
                .as_ref()
                .and_then(|a| a.category.clone())
                .ok_or("A merchant rule needs a category")?;
            let merchant = transaction["merchant"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            workspace.rules.insert(merchant, category);
            workspace.validate()?;
        }
        self.database.backup(&self.backup_path())?;
        self.database
            .replace_review(id, annotation.as_ref(), &workspace)?;
        if let Some(annotation) = annotation {
            self.data.annotations.insert(id.into(), annotation);
        } else {
            self.data.annotations.remove(id);
        }
        self.data.workspace = workspace;
        Ok(())
    }

    pub fn export_backup(&self, destination: &Path) -> Result<(), String> {
        // A new file only: a mistaken selection cannot overwrite a database or any user file.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(destination)
            .map_err(|_| "Choose a new filename; existing files are never overwritten")?;
        drop(file);
        let result = self
            .database
            .backup(destination)
            .and_then(|_| Database::validate_backup(destination));
        if result.is_err() {
            let _ = fs::remove_file(destination);
        }
        result
    }

    pub fn inspect_backup(&self, source: &Path) -> Result<BackupInfo, String> {
        let (temporary, database, state) = self.validated_backup_copy(source)?;
        let info = BackupInfo {
            updated_at: state.snapshot["updatedAt"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            revision: state.revision,
            schema_version: state.schema_version,
            accounts: state.snapshot["accounts"].as_array().map_or(0, Vec::len),
            transactions: state.snapshot["transactions"]
                .as_array()
                .map_or(0, Vec::len),
            holdings: state.snapshot["holdings"].as_array().map_or(0, Vec::len),
        };
        drop(database);
        let _ = fs::remove_file(temporary);
        Ok(info)
    }

    pub fn import_backup(&mut self, source: &Path) -> Result<crate::workspace::Workspace, String> {
        let (temporary, mut database, mut next) = self.validated_backup_copy(source)?;
        let result = (|| {
            next.revision = self
                .data
                .revision
                .max(next.revision)
                .checked_add(1)
                .ok_or("Revision overflow")?;
            next.snapshot["revision"] = next.revision.into();
            database.replace(&next)?;
            Database::validate_backup(&temporary)?;
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
                fs::rename(&temporary, &self.path).map_err(|_| "Could not install backup")?;
                self.database = Database::open(&self.path)?;
            } else {
                self.database.restore_from(&database)?;
                drop(database);
                let _ = fs::remove_file(&temporary);
            }
            self.data = next;
            self.pending = None;
            self.recovery = None;
            Ok(self.data.workspace.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn validated_backup_copy(
        &self,
        source: &Path,
    ) -> Result<(PathBuf, Database, FinanceState), String> {
        let metadata = fs::metadata(source).map_err(|_| "Backup cannot be read")?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err("Backup is empty or is not a file".into());
        }
        if metadata.len() > 512 * 1024 * 1024 {
            return Err("Backup exceeds 512 MB".into());
        }
        let temporary = self
            .path
            .with_extension(format!("{}.import", uuid::Uuid::new_v4()));
        let result = (|| {
            Database::copy_backup(source, &temporary)?;
            Database::validate_backup(&temporary)?;
            let database = Database::open(&temporary)?;
            let state = validate_state(database.load()?.ok_or("This is not a Brief backup")?)?;
            Ok((temporary.clone(), database, state))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn save_account_link(
        &mut self,
        plaid_account_id: String,
        snaptrade_account_id: Option<String>,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        if !plaid_account_id.starts_with("plaid:")
            || plaid_account_id.len() > 220
            || snaptrade_account_id
                .as_ref()
                .is_some_and(|id| !id.starts_with("snaptrade:") || id.len() > 220)
        {
            return Err("Invalid account link".into());
        }
        if let Some(target) = snaptrade_account_id.as_deref() {
            if self
                .data
                .account_links
                .iter()
                .any(|(plaid, snaptrade)| plaid != &plaid_account_id && snaptrade == target)
            {
                return Err("That SnapTrade account is already linked".into());
            }
            let confirmed_candidate = self.data.snapshot["possibleDuplicateAccounts"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|candidate| {
                    candidate["plaidAccountId"].as_str() == Some(&plaid_account_id)
                        && candidate["snaptradeAccountId"].as_str() == Some(target)
                });
            if !confirmed_candidate {
                return Err("That account pair is not a current duplicate candidate".into());
            }
        } else if !self.data.account_links.contains_key(&plaid_account_id) {
            return Err("That account link does not exist".into());
        }
        let mut next = self.data.account_links.clone();
        if let Some(snaptrade_account_id) = snaptrade_account_id {
            next.insert(plaid_account_id, snaptrade_account_id);
        } else {
            next.remove(&plaid_account_id);
        }
        if next != self.data.account_links {
            self.database.backup(&self.backup_path())?;
            self.database.replace_account_links(&next)?;
            self.data.account_links = next;
            self.pending = None;
        }
        Ok(())
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid local data in {}: {error}", path.display()))
}

fn read_or_default<T: Default + DeserializeOwned>(path: &Path) -> Result<T, String> {
    if path.exists() {
        read(path)
    } else {
        Ok(T::default())
    }
}

#[cfg(test)]
fn atomic_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| error.to_string())?;
    // Rename is the commit point: a directory flush failure must not report an uncommitted write.
    if let Some(parent) = path.parent() {
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

fn validate_snapshot(value: &Value) -> Result<(), String> {
    let snapshot: Snapshot = serde_json::from_value(value.clone())
        .map_err(|error| format!("Invalid financial snapshot: {error}"))?;
    validate_projected_snapshot(&snapshot)
}

fn validate_projected_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at)
        .map_err(|_| "Invalid snapshot timestamp")?;
    if snapshot
        .calendar_date
        .as_deref()
        .is_some_and(|date| !valid_date(date))
    {
        return Err("Invalid snapshot calendar date".into());
    }
    let ids = snapshot
        .accounts
        .iter()
        .map(|account| account.id.as_str())
        .collect::<BTreeSet<_>>();
    let total: f64 = snapshot
        .accounts
        .iter()
        .filter(|account| account.id != "all")
        .filter_map(|account| account.value)
        .sum();
    let all_accounts = snapshot
        .accounts
        .iter()
        .filter(|account| account.id == "all")
        .count();
    if all_accounts != 1
        || ids.len() != snapshot.accounts.len()
        || ids.contains("")
        || !total.is_finite()
        || (total - snapshot.net_worth).abs() > 0.011
        || snapshot.accounts.iter().any(|account| {
            account.id == "all"
                && account
                    .value
                    .is_none_or(|value| (value - total).abs() > 0.011)
        })
        || snapshot.net_worth_incomplete
            != snapshot
                .accounts
                .iter()
                .any(|account| account.id != "all" && account.value.is_none())
        || snapshot.holdings.iter().any(|holding| {
            holding.account_id == "all"
                || !ids.contains(holding.account_id.as_str())
                || [
                    holding.shares,
                    holding.price,
                    holding.value,
                    holding.cost_basis,
                    holding.unrealized_gain,
                    holding.daily_change_pct,
                    holding.weekly_change_pct,
                    holding.weekly_reference_price,
                    holding.total_change_pct,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || holding
                    .market_as_of
                    .as_deref()
                    .is_some_and(|timestamp| !valid_source_time(timestamp))
                || holding
                    .weekly_reference_date
                    .as_deref()
                    .is_some_and(|date| !valid_date(date))
                || holding.weekly_reference_price.is_some()
                    != holding.weekly_reference_date.is_some()
                || holding.weekly_change_pct.is_some() != holding.weekly_reference_price.is_some()
        })
        || snapshot.transactions.iter().any(|transaction| {
            transaction
                .account_id
                .as_ref()
                .is_some_and(|id| !ids.contains(id.as_str()))
        })
        || snapshot.trades.iter().any(|trade| {
            !ids.contains(trade.account_id.as_str())
                || !trade.amount.is_finite()
                || [
                    trade.realized_cost_basis,
                    trade.estimated_realized_gain,
                    trade.estimated_realized_gain_pct,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || trade
                    .realized_gain_method
                    .as_deref()
                    .is_some_and(|method| method != "estimated-fifo")
        })
        || snapshot.accounts.iter().any(|account| {
            account.value.is_some_and(|value| !value.is_finite())
                || [
                    account.known_cost_basis,
                    account.known_unrealized_gain,
                    account.known_unrealized_gain_pct,
                    account.investment_income_ytd,
                    account.sale_proceeds_ytd,
                    account.estimated_realized_gain_ytd,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
                || [
                    account.cost_basis_coverage.as_deref(),
                    account.realized_gain_coverage.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|coverage| !["complete", "partial", "unavailable"].contains(&coverage))
                || [
                    account.balance_as_of.as_deref(),
                    account.positions_as_of.as_deref(),
                    account.activity_as_of.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|timestamp| !valid_source_time(timestamp))
                || account
                    .balance_fetched_at
                    .as_deref()
                    .is_some_and(|timestamp| {
                        chrono::DateTime::parse_from_rfc3339(timestamp).is_err()
                    })
        })
    {
        return Err("Snapshot accounts, holdings, and net worth do not reconcile".into());
    }
    let transaction_ids = snapshot
        .transactions
        .iter()
        .map(|transaction| &transaction.id)
        .collect::<BTreeSet<_>>();
    if transaction_ids.len() != snapshot.transactions.len() {
        return Err("Duplicate transaction IDs".into());
    }
    if snapshot.transactions.iter().any(|transaction| {
        !transaction.amount.is_finite()
            || !valid_date(&transaction.date)
            || transaction
                .occurred_on
                .as_deref()
                .is_some_and(|date| !valid_date(date))
            || transaction
                .posted_on
                .as_deref()
                .is_some_and(|date| !valid_date(date))
    }) {
        return Err("Invalid transaction amounts or dates".into());
    }
    let trade_ids = snapshot
        .trades
        .iter()
        .map(|trade| &trade.id)
        .collect::<BTreeSet<_>>();
    if trade_ids.len() != snapshot.trades.len() {
        return Err("Duplicate trade IDs".into());
    }
    if snapshot.trades.iter().any(|trade| {
        !valid_date(&trade.date)
            || [Some(trade.amount), trade.units, trade.price]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite())
    }) {
        return Err("Invalid trade amounts or dates".into());
    }
    let movement_ids = snapshot
        .account_movements
        .iter()
        .map(|movement| &movement.id)
        .collect::<BTreeSet<_>>();
    if snapshot.account_movements.len() > 2_000
        || movement_ids.len() != snapshot.account_movements.len()
        || snapshot.account_movements.iter().any(|movement| {
            movement.id.is_empty()
                || movement.account_id.is_empty()
                || chrono::DateTime::parse_from_rfc3339(&movement.observed_at).is_err()
        })
    {
        return Err("Invalid account movement history".into());
    }
    if !valid_points(&snapshot.net_worth_history, true)
        || !valid_points(&snapshot.observed_net_worth_history, false)
        || !valid_points(&snapshot.benchmark_history, false)
    {
        return Err("Snapshot history is not ordered, unique, and finite".into());
    }
    let spending_total: f64 = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.value)
        .sum();
    let spending_percent: f64 = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.percent)
        .sum();
    let spending_names = snapshot
        .spending
        .categories
        .iter()
        .map(|category| category.name.as_str())
        .collect::<BTreeSet<_>>();
    if !snapshot.spending.month_total.is_finite()
        || snapshot.spending.month_total < 0.0
        || snapshot.spending.categories.iter().any(|category| {
            !category.value.is_finite()
                || category.value < 0.0
                || !category.percent.is_finite()
                || category.percent < 0.0
        })
        || (spending_total - snapshot.spending.month_total).abs() > 0.011
        || spending_names.len() != snapshot.spending.categories.len()
        || (snapshot.spending.month_total > 0.0 && (spending_percent - 100.0).abs() > 0.02)
        || (snapshot.spending.month_total == 0.0 && spending_percent.abs() > 0.001)
    {
        return Err("Spending totals do not reconcile".into());
    }
    if snapshot.brokerage_performance.iter().any(|performance| {
        (performance.account_id != "total" && !ids.contains(performance.account_id.as_str()))
            || !performance.current_value.is_finite()
            || performance
                .history_start
                .as_deref()
                .is_some_and(|date| !valid_date(date))
            || performance
                .performance_method
                .as_deref()
                .is_some_and(|method| {
                    ![
                        "value-only",
                        "value-with-comparisons",
                        "time-weighted",
                        "modified-dietz",
                    ]
                    .contains(&method)
                })
            || performance.history_source.as_deref().is_some_and(|source| {
                !["reported", "provider-estimated", "estimated", "unavailable"].contains(&source)
            })
            || !valid_performance_points(
                &performance.points,
                performance.performance_method.as_deref(),
            )
    }) {
        return Err("Brokerage performance is invalid or references an unknown account".into());
    }
    let balance_history_ids = snapshot
        .account_balance_history
        .iter()
        .map(|history| history.account_id.as_str())
        .collect::<BTreeSet<_>>();
    if balance_history_ids.len() != snapshot.account_balance_history.len()
        || snapshot.account_balance_history.iter().any(|history| {
            snapshot
                .accounts
                .iter()
                .find(|account| account.id == history.account_id)
                .is_none_or(|account| !["cash", "credit"].contains(&account.r#type.as_str()))
                || !history.current_value.is_finite()
                || history
                    .history_start
                    .as_deref()
                    .is_some_and(|date| !valid_date(date))
                || !matches!(
                    history.history_source.as_deref(),
                    Some("transaction-derived" | "unavailable")
                )
                || history.performance_method.as_deref() != Some("value-only")
                || !valid_performance_points(&history.points, Some("value-only"))
        })
    {
        return Err(
            "Account balance history is invalid or references an unsupported account".into(),
        );
    }
    let mut possible_duplicate_pairs = BTreeSet::new();
    if snapshot
        .possible_duplicate_accounts
        .iter()
        .any(|candidate| {
            candidate.description.trim().is_empty()
                || !candidate.plaid_account_id.starts_with("plaid:")
                || !candidate.snaptrade_account_id.starts_with("snaptrade:")
                || !ids.contains(candidate.plaid_account_id.as_str())
                || !ids.contains(candidate.snaptrade_account_id.as_str())
                || !possible_duplicate_pairs.insert((
                    candidate.plaid_account_id.as_str(),
                    candidate.snaptrade_account_id.as_str(),
                ))
        })
    {
        return Err("Invalid possible duplicate account candidates".into());
    }
    if snapshot
        .calculation_version
        .is_some_and(|version| version == 0)
    {
        return Err("Invalid financial calculation version".into());
    }
    if let Some(change) = &snapshot.last_change {
        let sum: f64 = change
            .account_changes
            .iter()
            .map(|account| account.change)
            .sum();
        if !change.net_worth_change.is_finite()
            || !change.previous_net_worth.is_finite()
            || (sum - change.net_worth_change).abs() > 0.011
            || chrono::DateTime::parse_from_rfc3339(&change.observed_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&change.previous_updated_at).is_err()
            || change
                .account_changes
                .iter()
                .any(|account| account.account_id.is_empty() || !account.change.is_finite())
            || change
                .new_transaction_ids
                .iter()
                .any(|id| !transaction_ids.contains(id))
        {
            return Err("Snapshot change does not reconcile".into());
        }
    }
    Ok(())
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn valid_source_time(value: &str) -> bool {
    valid_date(value) || chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

fn valid_points(points: &[Point], allow_legacy_empty: bool) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        if !point.value.is_finite()
            || (point.date == "—" && !(allow_legacy_empty && points.len() == 1))
        {
            return false;
        }
        if point.date != "—" && !valid_date(&point.date) {
            return false;
        }
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        ordered
    })
}

fn valid_performance_points(points: &[PerformancePoint], method: Option<&str>) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        valid_date(&point.date)
            && ordered
            && point.value.is_finite()
            && point.net_deposits.is_none_or(f64::is_finite)
            && point.sp500.is_none_or(f64::is_finite)
            && point.market_change.is_none_or(f64::is_finite)
            && point.market_change_pct.is_none_or(f64::is_finite)
            && if method == Some("value-only") {
                point.net_deposits.is_none()
                    && point.sp500.is_none()
                    && point.market_change.is_none()
                    && point.market_change_pct.is_none()
            } else {
                point.net_deposits.is_some()
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::SnapTradeData;

    #[test]
    fn old_history_corrections_are_removed_during_projection_upgrade() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["updatedAt"] = "2026-09-10T07:30:00Z".into();
        storage.data.snapshot["calculationVersion"] = 22.into();
        storage.data.snapshot["historyCorrections"] = serde_json::json!([{
            "accountId": "snaptrade:brokerage", "date": "2026-09-08", "value": 200
        }]);
        storage.data.provider_data.snaptrade = Some(SnapTradeData {
            accounts: vec![serde_json::json!({
                "id": "brokerage", "name": "Brokerage", "institution_name": "Broker",
                "balance": { "total": { "amount": 200, "currency": "USD" } }
            })],
            cash_balances: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "currency": { "code": "USD" }, "cash": 200
                })],
            )]),
            positions: BTreeMap::from([("brokerage".into(), vec![])]),
            balance_history: BTreeMap::from([(
                "brokerage".into(),
                vec![
                    serde_json::json!({ "date": "2026-09-07", "total_value": 200 }),
                    serde_json::json!({ "date": "2026-09-08", "total_value": 9200 }),
                ],
            )]),
            ..SnapTradeData::default()
        });
        assert!(storage.upgrade_projection().unwrap());
        assert!(storage.data.snapshot.get("historyCorrections").is_none());
        assert_eq!(
            storage.data.snapshot["netWorthHistory"]
                .as_array()
                .unwrap()
                .iter()
                .find(|point| point["date"] == "2026-09-08")
                .unwrap()["value"],
            9200.0
        );
        drop(storage);
        assert!(Storage::new(directory.clone())
            .unwrap()
            .data
            .snapshot
            .get("historyCorrections")
            .is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn older_calculation_versions_rebuild_from_the_committed_cache() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["updatedAt"] = "2026-09-10T07:30:00Z".into();
        storage.data.snapshot["calculationVersion"] = 4.into();
        storage.data.snapshot["benchmarkHistory"] = serde_json::json!([
            { "date": "2026-09-08", "value": 100 },
            { "date": "2026-09-10", "value": 101 }
        ]);
        storage.data.provider_data.snaptrade = Some(SnapTradeData {
            accounts: vec![serde_json::json!({
                "id": "brokerage", "name": "Brokerage", "institution_name": "Broker",
                "balance": { "total": { "amount": 200, "currency": "USD" } }
            })],
            cash_balances: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "currency": { "code": "USD" }, "cash": 200
                })],
            )]),
            positions: BTreeMap::from([("brokerage".into(), vec![])]),
            activities: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "id": "transfer", "type": "TRANSFER", "amount": 10,
                    "trade_date": "2026-09-09T15:30:00Z"
                })],
            )]),
            balance_history: BTreeMap::from([(
                "brokerage".into(),
                vec![serde_json::json!({
                    "date": "2026-09-08", "total_value": 190
                })],
            )]),
            history_complete: true,
            activity_complete: true,
            ..SnapTradeData::default()
        });

        assert!(storage.upgrade_projection().unwrap());
        assert_eq!(
            storage.data.snapshot["calculationVersion"],
            crate::financial_engine::CALCULATION_VERSION
        );
        let account = &storage.data.snapshot["brokeragePerformance"][1];
        assert_eq!(account["points"][1]["netDeposits"], 200.0);
        assert_eq!(account["points"][1]["sp500"], 201.9);
        drop(storage);
        assert_eq!(
            Storage::new(directory.clone()).unwrap().data.snapshot["calculationVersion"],
            crate::financial_engine::CALCULATION_VERSION
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupted_primary_opens_read_only_backup_and_restores_without_discarding_original() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let mut snapshot = storage.data.snapshot.clone();
        snapshot["updatedAt"] = "2026-09-01T12:00:00Z".into();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.commit_value(&id, snapshot.clone()).unwrap();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                1,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.commit_value(&id, snapshot).unwrap();
        fs::write(&storage.path, b"synthetic corrupt state").unwrap();
        drop(storage);
        let mut storage = Storage::new(directory.clone()).unwrap();
        assert_eq!(storage.data.revision, 1);
        assert_eq!(storage.snapshot().unwrap()["recovery"]["canRestore"], true);
        assert!(storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                1,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![]
            )
            .is_err());
        assert!(storage.save_annotations(BTreeMap::new(), true).is_ok());
        assert_eq!(fs::read(&storage.path).unwrap(), b"synthetic corrupt state");
        let restored = storage.recover(true).unwrap();
        assert!(restored.get("recovery").is_none());
        assert_eq!(load_database(&storage.path).unwrap().1.revision, 1);
        assert!(fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .any(
                |entry| entry.file_name().to_string_lossy().contains("retained-")
                    && fs::read(entry.path()).unwrap() == b"synthetic corrupt state"
            ));
        drop(storage);
        assert!(Storage::new(directory.clone()).unwrap().recovery.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_without_a_valid_backup_requires_an_explicit_fresh_start() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        fs::write(&storage.path, b"corrupt primary").unwrap();
        fs::write(storage.backup_path(), b"corrupt backup").unwrap();
        drop(storage);
        let mut storage = Storage::new(directory.clone()).unwrap();
        assert!(!storage.recovery.as_ref().unwrap().can_restore);
        assert!(storage.recover(true).is_err());
        assert!(storage.recover(false).is_ok());
        assert_eq!(storage.data.revision, 0);
        assert_eq!(
            fs::read_dir(&directory)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains("retained-"))
                .count(),
            2
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_missing_primary_does_not_overwrite_a_valid_retained_snapshot() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        fs::remove_file(&storage.path).unwrap();
        drop(storage);
        let storage = Storage::new(directory.clone()).unwrap();
        assert!(storage.recovery.as_ref().unwrap().can_restore);
        assert!(!storage.path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nullable_values_require_an_explicit_incomplete_subtotal() {
        let mut snapshot = empty_state().unwrap().snapshot;
        snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 100},
            {"id": "cash", "name": "Cash", "institution": "Bank", "type": "cash", "value": 100},
            {"id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": null}
        ]);
        snapshot["netWorth"] = 100.into();
        snapshot["holdings"] = serde_json::json!([{"ticker": "UNPRICED", "name": "Unpriced", "accountId": "cash", "shares": null, "price": null, "value": null, "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": "#000000"}]);
        assert!(validate_snapshot(&snapshot).is_err());
        snapshot["netWorthIncomplete"] = true.into();
        assert!(validate_snapshot(&snapshot).is_ok());
    }

    #[test]
    fn confirmed_account_links_are_one_to_one() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 0},
            {"id": "plaid:one", "name": "One", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "plaid:two", "name": "Two", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "snaptrade:same", "name": "Same", "institution": "Broker", "type": "brokerage", "value": 0}
        ]);
        storage.data.snapshot["possibleDuplicateAccounts"] = serde_json::json!([
            {"plaidAccountId": "plaid:one", "snaptradeAccountId": "snaptrade:same", "description": "Candidate one"},
            {"plaidAccountId": "plaid:two", "snaptradeAccountId": "snaptrade:same", "description": "Candidate two"}
        ]);
        storage
            .save_account_link("plaid:one".into(), Some("snaptrade:same".into()))
            .unwrap();
        assert!(storage
            .save_account_link("plaid:two".into(), Some("snaptrade:same".into()))
            .is_err());
        assert_eq!(storage.data.account_links.len(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_validation_retains_original_run_identity_and_committed_state() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let original = storage.data.snapshot.clone();
        let original_disk = storage.database.load().unwrap().unwrap().snapshot;
        let started = "2026-09-18T12:00:00Z";
        storage
            .stage(
                "synthetic-run",
                started,
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        let mut invalid: Snapshot = crate::finance_contract::decode(original.clone()).unwrap();
        invalid.net_worth = 999.0;
        let error = storage.commit("synthetic-run", invalid).unwrap_err();
        assert_eq!(error.code(), "validation_failed");
        storage.fail_pending("synthetic-run", error.code());
        assert_eq!(storage.data.snapshot, original);
        assert_eq!(
            storage.database.load().unwrap().unwrap().snapshot,
            original_disk
        );
        let runs = serde_json::to_value(storage.sync_runs().unwrap()).unwrap();
        assert_eq!(runs[0]["id"], "synthetic-run");
        assert_eq!(runs[0]["startedAt"], started);
        assert_eq!(runs[0]["errorCode"], "validation_failed");
        assert_eq!(runs[0]["details"]["phase"], "validation");
        assert!(storage.pending.is_none());
        drop(storage);
        fs::remove_dir_all(directory).unwrap();
    }

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("brief-storage-{}", uuid::Uuid::new_v4()))
    }
    #[test]
    fn refresh_is_atomic_validated_and_single_use() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let original = storage.data.snapshot.clone();
        let cache: PlaidCache = serde_json::from_value(
            serde_json::json!({"items": {"item": {"cursor": "next", "transactions": []}}}),
        )
        .unwrap();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                cache,
                ProviderDataCache::default(),
                vec!["Offline".into()],
            )
            .unwrap();
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
        assert!(storage
            .commit_value(&id, serde_json::json!({"netWorth": 999}))
            .is_err());
        let mut invalid = original.clone();
        invalid["netWorth"] = 999.into();
        assert!(storage.commit_value(&id, invalid).is_err());
        let result = storage.commit_value(&id, original.clone()).unwrap();
        assert_eq!(result["syncWarnings"][0], "Offline");
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 1);
        assert_eq!(
            serde_json::to_value(&storage.database.load().unwrap().unwrap().plaid_cache).unwrap()
                ["items"]["item"]["cursor"],
            "next"
        );
        assert!(storage.commit_value(&id, original).is_err());
        assert!(storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![]
            )
            .is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn superseded_refresh_and_unknown_versions_are_rejected() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let old = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        assert!(storage
            .commit_value(&old, storage.data.snapshot.clone())
            .is_err());
        storage.data.schema_version = 2;
        storage.database.replace(&storage.data).unwrap();
        drop(storage);
        let recovery = Storage::new(directory.clone()).unwrap();
        assert!(recovery.recovery.is_some());
        assert!(recovery.ensure_writable().is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn migrates_legacy_data_then_removes_plaintext_sources() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("finance-snapshot.json");
        let mut snapshot: Value =
            serde_json::from_str(include_str!("../../src/data/empty.json")).unwrap();
        snapshot["accounts"][0]["balanceAsOf"] = "2026-09-05".into();
        snapshot["syncWarnings"] = serde_json::json!(["raw account message"]);
        snapshot["providerStatus"] = serde_json::json!({
            "plaid": {"updatedAt": null, "error": "raw account message"}
        });
        atomic_write(&legacy, &snapshot).unwrap();
        fs::write(directory.join("news.sqlite3"), b"synthetic cache").unwrap();
        let storage = Storage::new(directory.clone()).unwrap();
        snapshot["syncWarnings"] = serde_json::json!([]);
        snapshot["providerStatus"]["plaid"]["error"] = Value::Null;
        assert_eq!(storage.data.snapshot, snapshot);
        assert!(!legacy.exists());
        assert!(storage.backup_path().exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&storage.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(storage.backup_path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(directory.join("news.sqlite3"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(storage);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inspecting_an_open_primary_uses_a_consistent_sqlite_backup() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        let info = storage.inspect_backup(&storage.path).unwrap();
        assert_eq!(info.revision, storage.data.revision);
        drop(storage);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_legacy_state_opens_recovery_without_creating_a_database() {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let legacy = directory.join("finance-state.json");
        let mut state = empty_state().unwrap();
        state.snapshot["netWorth"] = 999.into();
        atomic_write(&legacy, &state).unwrap();

        let storage = Storage::new(directory.clone()).unwrap();

        assert!(storage.recovery.is_some());
        assert!(!storage.path.exists());
        assert!(legacy.exists());
        assert!(storage.ensure_writable().is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_write_preserves_disk_memory_and_pending_refresh() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        let mut invalid = storage.data.snapshot.clone();
        invalid["possibleDuplicateAccounts"] = serde_json::json!([
            {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "one"},
            {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "duplicate"}
        ]);
        assert!(storage.commit_value(&id, invalid).is_err());
        assert_eq!(storage.data.revision, 0);
        assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
        storage
            .commit_value(&id, storage.data.snapshot.clone())
            .unwrap();
        assert_eq!(storage.data.revision, 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn annotations_survive_refresh_and_legacy_import_never_overwrites_edits() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        let edits = BTreeMap::from([(
            "transaction".into(),
            Annotation {
                category: Some("Travel".into()),
                reviewed: Some(true),
                benefit_confirmed: Some(true),
            },
        )]);
        storage.save_annotations(edits, false).unwrap();
        let legacy = BTreeMap::from([(
            "transaction".into(),
            Annotation {
                category: Some("Other".into()),
                reviewed: Some(false),
                benefit_confirmed: Some(false),
            },
        )]);
        storage.save_annotations(legacy, true).unwrap();
        storage
            .commit_value(&id, storage.data.snapshot.clone())
            .unwrap();
        drop(storage);
        let restored = Storage::new(directory.clone()).unwrap();
        assert_eq!(
            restored.data.annotations["transaction"].category.as_deref(),
            Some("Travel")
        );
        assert_eq!(
            restored.data.annotations["transaction"].reviewed,
            Some(true)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn expired_refresh_does_not_commit() {
        let directory = directory();
        let mut storage = Storage::new(directory.clone()).unwrap();
        let id = storage
            .stage(
                &uuid::Uuid::new_v4().to_string(),
                &chrono::Utc::now().to_rfc3339(),
                0,
                PlaidCache::default(),
                ProviderDataCache::default(),
                vec![],
            )
            .unwrap();
        storage.pending.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert!(storage
            .commit_value(&id, storage.data.snapshot.clone())
            .is_err());
        assert_eq!(storage.data.revision, 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_one_app_instance_can_own_the_store() {
        let directory = directory();
        let storage = Storage::new(directory.clone()).unwrap();
        assert!(Storage::new(directory.clone()).is_err());
        drop(storage);
        assert!(Storage::new(directory.clone()).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }
}
