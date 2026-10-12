//! Staging, commits, recovery, and backups for the finance database.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    database::{Database, SyncDiagnostics, SyncRun},
    finance_contract::{PerformancePoint, Point, Recovery, Snapshot},
    providers::{PlaidCache, ProviderDataCache, ProviderSync},
    summary::{CalibrationSummary, Closed, Kept},
};

mod annotations;
mod backup;
mod legacy;
mod statements;
mod validate;

pub use annotations::{splits_divide, Annotation, Split};
use legacy::*;
pub use statements::{StatementDraft, StatementPreview, StatementSummary};
use validate::*;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceState {
    pub(crate) schema_version: u32,
    pub revision: u64,
    pub snapshot: Snapshot,
    pub plaid_cache: PlaidCache,
    pub provider_data: ProviderDataCache,
    #[serde(default)]
    pub annotations: BTreeMap<String, Annotation>,
    #[serde(default)]
    pub account_links: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: crate::settings::FinanceSettings,
    #[serde(default)]
    pub rules: Vec<crate::rules::CategoryRule>,
    /// Imported statement files. Unlike provider caches they can't be fetched again.
    #[serde(default)]
    pub statements: Vec<crate::statements::StatementFile>,
}

/// What a refresh fetched and noticed, waiting for its projection to commit.
pub struct Staged {
    pub plaid_cache: PlaidCache,
    pub provider_data: ProviderDataCache,
    pub warnings: Vec<String>,
    pub diagnostics: SyncDiagnostics,
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
    /// When the retained copy was last refreshed for a small edit, and at which revision.
    edit_backup: Option<(Instant, u64)>,
    /// The last derived view, reused while nothing it depends on has changed.
    view: std::cell::RefCell<Option<CachedView>>,
}

/// Insights depend on the clock as well as saved data (benefit windows roll over in their own
/// time zones), so a derived view is reused only briefly.
const VIEW_LIFETIME: Duration = Duration::from_secs(30);

struct CachedView {
    key: u64,
    built: Instant,
    /// The view as Rust reads it.
    view: std::sync::Arc<crate::insights::View>,
    /// The same view as the renderer receives it.
    json: Value,
}

/// The view as the renderer receives it: the snapshot with the edits and insights it was
/// read with.
#[derive(Serialize)]
struct RendererView<'a> {
    #[serde(flatten)]
    snapshot: &'a Snapshot,
    settings: &'a crate::settings::FinanceSettings,
    rules: &'a [crate::rules::CategoryRule],
    #[serde(skip_serializing_if = "Option::is_none")]
    insights: Option<&'a crate::insights::Insights>,
}

/// Small edits (review, categories, settings, rules, links) share one retained copy for this
/// long, so a review session does not rewrite the whole database for every keystroke.
const EDIT_BACKUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

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
        snapshot: serde_json::from_str(include_str!("../../../src/data/empty.json"))
            .map_err(|error| error.to_string())?,
        plaid_cache: PlaidCache::default(),
        provider_data: ProviderDataCache::default(),
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
        settings: Default::default(),
        rules: Vec::new(),
        statements: Vec::new(),
    })
}

fn validate_state(data: FinanceState) -> Result<FinanceState, String> {
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
    let mut statement_ids = BTreeSet::new();
    if data.statements.len() > crate::statements::MAX_FILES
        || data
            .statements
            .iter()
            .any(|file| file.validate().is_err() || !statement_ids.insert(file.id.as_str()))
    {
        return Err("Invalid saved statements".into());
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

impl Storage {
    pub fn cancel_pending_refresh(&mut self) {
        self.pending = None;
    }

    pub fn sync_runs(&self) -> Result<Vec<SyncRun>, String> {
        self.database.sync_runs()
    }

    /// What the finance database keeps for a summary of `period`: its saved sessions and
    /// events, and the splits and dividends committed with the last refresh.
    pub fn summary_kept(&self, period: crate::session::Period) -> Result<Kept, String> {
        Ok(Kept {
            sessions: self.database.market_sessions(period.start, period.end)?,
            events: self.database.market_events(period.start, period.end)?,
            corporate_actions: self.data.provider_data.corporate_actions.clone(),
        })
    }

    /// Saves closed sessions' records and their events together; results, not edits, so the
    /// revision doesn't change.
    pub fn save_market_sessions(&mut self, closed: &Closed) -> Result<(), String> {
        if closed.sessions.is_empty() || self.recovery.is_some() {
            return Ok(());
        }
        self.database.save_market_sessions(closed)
    }

    /// How often the layered model's thresholds fired over the latest year of saved sessions.
    pub fn calibration(&self) -> Result<CalibrationSummary, String> {
        Ok(crate::summary::calibration(
            &self.database.latest_market_sessions(250)?,
        ))
    }

    pub fn record_failed_sync(
        &mut self,
        id: &str,
        started_at: &str,
        error_code: &str,
        diagnostics: SyncDiagnostics,
    ) {
        let _ = self.database.record_sync_run(&SyncRun::ended(
            id,
            started_at,
            "failed",
            0,
            Some(error_code),
            diagnostics,
        ));
    }

    /// The annotation-aware snapshot with insights, as the renderer reads it.
    pub fn snapshot(&self) -> Result<Value, String> {
        self.view(|view| view.json.clone())
    }

    /// The annotation-aware snapshot and its insights, as Rust reads them.
    pub fn read_view(&self) -> Result<std::sync::Arc<crate::insights::View>, String> {
        self.view(|cached| cached.view.clone())
    }

    /// Deriving the view is the expensive part of every read, so it is reused until saved
    /// data, the calendar day, or its lifetime changes.
    fn view<T>(&self, read: impl FnOnce(&CachedView) -> T) -> Result<T, String> {
        let key = self.view_key()?;
        let fresh = self
            .view
            .borrow()
            .as_ref()
            .is_some_and(|view| view.key == key && view.built.elapsed() < VIEW_LIFETIME);
        if !fresh {
            let (view, json) = self.derive_snapshot()?;
            *self.view.borrow_mut() = Some(CachedView {
                key,
                built: Instant::now(),
                view: std::sync::Arc::new(view),
                json,
            });
        }
        let view = self.view.borrow();
        Ok(read(
            view.as_ref().ok_or("The derived view is unavailable")?,
        ))
    }

    /// Identifies everything a derived view is computed from except the time of day.
    fn view_key(&self) -> Result<u64, String> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.data.revision.hash(&mut hasher);
        self.data.snapshot.updated_at.hash(&mut hasher);
        self.data.snapshot.transactions.len().hash(&mut hasher);
        self.recovery.is_some().hash(&mut hasher);
        chrono::Local::now().date_naive().hash(&mut hasher);
        serde_json::to_vec(&(
            &self.data.annotations,
            &self.data.rules,
            &self.data.settings,
            &self.data.account_links,
        ))
        .map_err(|error| error.to_string())?
        .hash(&mut hasher);
        Ok(hasher.finish())
    }

    /// The committed snapshot with saved edits applied and its insights derived, as the
    /// renderer reads it.
    fn derive_snapshot(&self) -> Result<(crate::insights::View, Value), String> {
        let mut snapshot = self.data.snapshot.clone();
        snapshot.recovery = self.recovery.clone();
        crate::financial_engine::apply_annotations(
            &mut snapshot,
            &self.data.annotations,
            &self.data.rules,
        )?;
        // Insights are a view of saved data; failing to derive them never hides the snapshot.
        let insights = crate::insights::derive(
            &mut snapshot,
            &self.data.settings,
            chrono::Utc::now(),
            &crate::insights::calendar::LocalZone,
        )
        .inspect_err(|_| {
            snapshot
                .sync_warnings
                .push("Expected activity is unavailable for this snapshot.".into())
        })
        .ok();
        let view = serde_json::to_value(RendererView {
            snapshot: &snapshot,
            settings: &self.data.settings,
            rules: &self.data.rules,
            insights: insights.as_ref(),
        })
        .map_err(|error| error.to_string())?;
        Ok((
            crate::insights::View {
                snapshot,
                insights,
                account_names: self.data.settings.account_names.clone(),
            },
            view,
        ))
    }

    /// Replaces durable finance settings. An import only fills values that are still unset.
    pub fn save_settings(
        &mut self,
        settings: crate::settings::FinanceSettings,
        importing: bool,
    ) -> Result<Value, String> {
        if self.recovery.is_some() && importing {
            return self.snapshot();
        }
        self.ensure_writable()?;
        let settings = settings.normalized()?;
        let next = if importing {
            self.data.settings.merge_missing(settings).normalized()?
        } else {
            settings
        };
        if next != self.data.settings {
            self.backup_before_edit()?;
            self.database.replace_settings(&next)?;
            self.data.settings = next;
        }
        self.snapshot()
    }

    /// Replaces the ordered category rules; they apply when the snapshot is read.
    pub fn save_rules(&mut self, rules: Vec<crate::rules::CategoryRule>) -> Result<Value, String> {
        self.ensure_writable()?;
        let rules = crate::rules::validate(rules)?;
        if rules != self.data.rules {
            self.backup_before_edit()?;
            self.database.replace_rules(&rules)?;
            self.data.rules = rules;
        }
        self.snapshot()
    }

    pub fn upgrade_projection(&mut self) -> Result<bool, String> {
        if self.recovery.is_some()
            || self.pending.is_some()
            || self
                .data
                .snapshot
                .calculation_version
                .is_some_and(|version| version >= crate::financial_engine::CALCULATION_VERSION)
            || (self.data.provider_data.plaid.is_none()
                && self.data.provider_data.snaptrade.is_none())
        {
            return Ok(false);
        }
        self.reproject_cached(self.data.statements.clone())?;
        Ok(true)
    }

    /// Projects saved provider data again, without the network, with these statements.
    fn reproject_cached(
        &mut self,
        statements: Vec<crate::statements::StatementFile>,
    ) -> Result<(), String> {
        let previous = &self.data.snapshot;
        let updated_at = chrono::DateTime::parse_from_rfc3339(&previous.updated_at).ok();
        let now = updated_at
            .map(|value| value.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let calendar_date = previous
            .calendar_date
            .as_deref()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .or_else(|| updated_at.map(|value| value.date_naive()))
            .unwrap_or_else(|| now.date_naive());
        let json =
            |value: Result<Value, serde_json::Error>| value.map_err(|error| error.to_string());
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
            benchmark_history: json(serde_json::to_value(&previous.benchmark_history))?,
            security_history: self.data.provider_data.security_history.clone(),
            corporate_actions: self.data.provider_data.corporate_actions.clone(),
            statements: statements.clone(),
        };
        let mut snapshot =
            crate::financial_engine::project(&sync, &self.data.account_links, now, calendar_date)?;
        let revision = self
            .data
            .revision
            .checked_add(1)
            .ok_or("Snapshot revision overflow")?;
        snapshot.revision = Some(revision);
        snapshot.provider_status = Some(self.data.provider_data.sync_status.clone());
        if snapshot.last_change.is_none() {
            snapshot.last_change = previous.last_change.clone();
        }
        // Statement warnings come from this projection; earlier ones may no longer apply.
        let mut warnings = previous
            .sync_warnings
            .iter()
            .filter(|warning| !warning.starts_with("Statement"))
            .cloned()
            .chain(std::mem::take(&mut snapshot.sync_warnings))
            .collect::<Vec<_>>();
        warnings.sort();
        warnings.dedup();
        snapshot.sync_warnings = warnings;

        let next = validate_state(FinanceState {
            schema_version: self.data.schema_version,
            revision,
            snapshot,
            plaid_cache: self.data.plaid_cache.clone(),
            provider_data: self.data.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
            settings: self.data.settings.clone(),
            rules: self.data.rules.clone(),
            statements,
        })?;
        self.database.backup(&self.backup_path())?;
        self.database.replace(&next)?;
        self.data = next;
        self.pending = None;
        Ok(())
    }

    pub fn is_recovering(&self) -> bool {
        self.recovery.is_some()
    }

    pub fn ensure_writable(&self) -> Result<(), String> {
        if self.recovery.is_some() {
            Err("Restore or start a new local snapshot before changing saved data".into())
        } else {
            Ok(())
        }
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
                let next_step = if backup.is_some() {
                    "Showing a retained valid snapshot, read-only, until you choose recovery."
                } else {
                    "No valid backup was found. Original files will be retained if you start a \
                     new snapshot."
                };
                let recovery = Recovery {
                    message: format!(
                        "The saved database could not be read or requires a newer app version. \
                         {next_step}"
                    ),
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
            edit_backup: None,
            view: std::cell::RefCell::new(None),
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
        self.stage_refresh(
            id,
            started_at,
            revision,
            Staged {
                plaid_cache,
                provider_data,
                warnings,
                diagnostics: SyncDiagnostics::default(),
            },
        )
    }

    /// Holds a refresh's fetched caches until its projection commits, if the committed revision
    /// is still the one it started from.
    pub fn stage_refresh(
        &mut self,
        id: &str,
        started_at: &str,
        revision: u64,
        staged: Staged,
    ) -> Result<String, String> {
        self.ensure_writable()?;
        let Staged {
            plaid_cache,
            provider_data,
            warnings,
            diagnostics,
        } = staged;
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
        validate_snapshot(&snapshot).map_err(CommitError::Validation)?;
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
        let next = FinanceState {
            schema_version: 1,
            revision,
            snapshot,
            plaid_cache: pending.plaid_cache.clone(),
            provider_data: pending.provider_data.clone(),
            annotations: self.data.annotations.clone(),
            account_links: self.data.account_links.clone(),
            settings: self.data.settings.clone(),
            rules: self.data.rules.clone(),
            statements: self.data.statements.clone(),
        };
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
        let _ = self.database.record_sync_run(&SyncRun::ended(
            &pending.id,
            &pending.started_at,
            "committed",
            warnings.len(),
            None,
            details,
        ));
        self.data = next;
        self.pending = None;
        self.snapshot().map_err(CommitError::Persistence)
    }

    /// Commits a snapshot given in any serializable form, as a test builds one.
    #[cfg(test)]
    fn commit_value(&mut self, id: &str, value: impl Serialize) -> Result<Value, String> {
        let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
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
        let _ = self.database.record_sync_run(&SyncRun::ended(
            id,
            &pending.started_at,
            "failed",
            pending.warnings.len(),
            Some(error_code),
            details,
        ));
        self.pending = None;
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
            let confirmed_candidate =
                self.data
                    .snapshot
                    .possible_duplicate_accounts
                    .iter()
                    .any(|candidate| {
                        candidate.plaid_account_id == plaid_account_id
                            && candidate.snaptrade_account_id == target
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
            self.backup_before_edit()?;
            self.database.replace_account_links(&next)?;
            self.data.account_links = next;
            self.pending = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
