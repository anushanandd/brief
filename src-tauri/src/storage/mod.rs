//! Staging, commits, recovery, and backups for the finance database.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{
    database::{Database, SyncDiagnostics, SyncRun},
    finance_contract::{PerformancePoint, Point, Recovery, Snapshot},
    providers::{PlaidCache, ProviderDataCache},
    summary::{CalibrationSummary, Closed, Kept},
};

mod annotations;
mod backup;
mod bank_statements;
pub mod change;
pub mod documents;
mod legacy;
mod statements;
mod validate;

pub use annotations::{splits_divide, Annotation, Split};
pub(crate) use bank_statements::OPENED_DIRECTORY as OPENED_STATEMENTS;
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
    /// Imported statement files. Unlike provider caches they can't be fetched again.
    #[serde(default)]
    pub statements: Vec<crate::statements::StatementFile>,
    /// What Brief read from the user's documents. The encrypted originals are kept in
    /// `documents.sqlite3` beside the finance database.
    #[serde(default)]
    pub documents: Vec<crate::documents::Document>,
}

/// What a refresh fetched and noticed, waiting for its projection to commit.
pub struct Staged {
    pub refresh: change::Refresh,
    pub diagnostics: SyncDiagnostics,
}

struct PendingRefresh {
    id: String,
    started_at: String,
    revision: u64,
    expires: Instant,
    refresh: change::Refresh,
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
    /// `places.sqlite3`, the disposable cache of Overture searches; see `places::cache`.
    pub places_path: PathBuf,
    /// What the latest replay of the change log found, once a launch has checked.
    pub log_check: Option<change::LogCheck>,
}

/// Insights depend on the clock as well as saved data: benefit windows roll over at midnight in
/// their own time zones. Every zone's offset is a whole number of quarter hours, so a view
/// built in one quarter hour holds until the next.
const VIEW_CLOCK_STEP_SECONDS: i64 = 15 * 60;

struct CachedView {
    key: u64,
    /// The view as Rust reads it.
    view: std::sync::Arc<crate::insights::View>,
    /// The same view as the renderer receives it.
    json: Value,
    /// The object graph of this view, built on the first read that needs it.
    graph: Option<std::sync::Arc<crate::objects::Graph>>,
}

/// The view as the renderer receives it: the snapshot with the edits and insights it was
/// read with.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "FinanceSnapshot")]
struct RendererView<'a> {
    #[serde(flatten)]
    snapshot: &'a Snapshot,
    settings: &'a crate::settings::FinanceSettings,
    /// Accounts the user closed, which the snapshot above no longer lists.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    closed_accounts: &'a [crate::financial_engine::manual::ClosedAccount],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    insights: Option<&'a crate::insights::Insights>,
    places: &'a crate::places::matching::Status,
    documents: &'a [crate::documents::Document],
}

/// Small edits (review, categories, settings, links) share one retained copy for this
/// long, so a review session does not rewrite the whole database for every keystroke.
const EDIT_BACKUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupInfo {
    pub updated_at: String,
    pub revision: u64,
    pub schema_version: u32,
    pub accounts: usize,
    pub transactions: usize,
    pub holdings: usize,
    pub documents: usize,
    /// Whether the documents' originals open here: `thisMac` (sealed with this Mac's key),
    /// `passphrase` (their key is sealed under a passphrase), `otherMac` (they can't), or
    /// `none` when the backup carries no originals.
    pub document_key: &'static str,
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
        statements: Vec::new(),
        documents: Vec::new(),
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
    for document in &data.documents {
        document.validate()?;
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

/// The committed snapshot with saved edits applied, places matched, evidence graded, and its
/// insights derived: the view as Rust reads it, and as the renderer receives it. The browser
/// preview's seed view is made by the same function.
pub(crate) fn derive_view(
    data: &FinanceState,
    recovery: Option<Recovery>,
    searches: &BTreeMap<String, Vec<crate::places::overture::OverturePlace>>,
    now: chrono::DateTime<chrono::Utc>,
    zone: &dyn crate::insights::calendar::Zone,
) -> Result<(crate::insights::View, Value), String> {
    let mut snapshot = data.snapshot.clone();
    snapshot.recovery = recovery;
    let closed_accounts =
        crate::financial_engine::manual::adjust_snapshot(&mut snapshot, &data.settings);
    crate::financial_engine::apply_annotations(&mut snapshot, &data.annotations)?;
    crate::places::matching::apply(&mut snapshot.transactions, searches);
    let places = crate::places::matching::status(&snapshot.transactions, searches);
    crate::quality::apply(&mut snapshot);
    // Insights are a view of saved data; failing to derive them never hides the snapshot.
    let insights = crate::insights::derive_with_documents(
        &mut snapshot,
        &data.settings,
        &data.documents,
        now,
        zone,
    )
    .inspect_err(|_| {
        snapshot
            .sync_warnings
            .push("Expected activity is unavailable for this snapshot.".into())
    })
    .ok();
    let view = serde_json::to_value(RendererView {
        snapshot: &snapshot,
        settings: &data.settings,
        closed_accounts: &closed_accounts,
        insights: insights.as_ref(),
        places: &places,
        documents: &data.documents,
    })
    .map_err(|error| error.to_string())?;
    Ok((
        crate::insights::View {
            snapshot,
            insights,
            account_names: data.settings.account_names.clone(),
            spending_account_id: data.settings.spending_account_id.clone(),
        },
        view,
    ))
}

/// Opens a finance database at the state its change log holds (`change::authoritative`),
/// bringing its tables back to the log when they strayed.
fn load_database(path: &Path) -> Result<(Database, FinanceState), String> {
    let mut database = Database::open(path)?;
    let (data, tables) = change::authoritative(&database)?;
    let data = validate_state(data)?;
    match tables {
        change::Tables::Current => {}
        change::Tables::Rebuild => database.rebuild_tables(&data)?,
        change::Tables::Checkpoint => database.checkpoint(&data, change::Reason::Upgrade)?,
    }
    Ok((database, data))
}

/// A backup's state as its change log holds it. Opening brings an older backup's schema up
/// to date, as before; its tables are left as they are.
fn read_backup(path: &Path) -> Result<FinanceState, String> {
    let (data, _) = change::authoritative(&Database::open(path)?)?;
    validate_state(data)
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

    /// The saved finance settings.
    pub fn settings(&self) -> &crate::settings::FinanceSettings {
        &self.data.settings
    }

    /// The research log, newest first, and what each rule source said when last read.
    pub fn research(
        &self,
    ) -> Result<
        (
            Vec<crate::research::Entry>,
            Vec<crate::research::SourceCheck>,
        ),
        String,
    > {
        Ok((
            self.database.research_log()?,
            self.database.source_checks()?,
        ))
    }

    /// Logs a research query before it runs; results, not edits, so the revision is unchanged.
    pub fn begin_research(&self, entry: &crate::research::Entry) -> Result<(), String> {
        self.ensure_writable()?;
        self.database.begin_research(entry)
    }

    pub fn finish_research(&self, entry: &crate::research::Entry) -> Result<(), String> {
        self.ensure_writable()?;
        self.database.finish_research(entry)
    }

    pub fn source_check(&self, url: &str) -> Result<Option<crate::research::SourceCheck>, String> {
        self.database.source_check(url)
    }

    pub fn save_source_check(&self, check: &crate::research::SourceCheck) -> Result<(), String> {
        self.ensure_writable()?;
        self.database.save_source_check(check)
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
    /// data or the quarter hour changes.
    fn view<T>(&self, read: impl FnOnce(&CachedView) -> T) -> Result<T, String> {
        let key = self.view_key()?;
        let fresh = self
            .view
            .borrow()
            .as_ref()
            .is_some_and(|view| view.key == key);
        if !fresh {
            let (view, json) = self.derive_snapshot()?;
            *self.view.borrow_mut() = Some(CachedView {
                key,
                view: std::sync::Arc::new(view),
                json,
                graph: None,
            });
        }
        let view = self.view.borrow();
        Ok(read(
            view.as_ref().ok_or("The derived view is unavailable")?,
        ))
    }

    /// The object graph of the current view. Building it also saves it to the graph's tables,
    /// which the readers of single objects use.
    pub fn graph(&self) -> Result<std::sync::Arc<crate::objects::Graph>, String> {
        if let Some(graph) = self.view(|cached| cached.graph.clone())? {
            return Ok(graph);
        }
        let view = self.read_view()?;
        let graph = std::sync::Arc::new(crate::objects::graph(&self.data, &view));
        let hash = graph.hash()?;
        // A failed save leaves the tables as they were; the next read compares and retries.
        if self.database.graph_hash().ok().flatten() != Some(hash.clone()) {
            let _ = self.database.write_graph(&graph, &hash);
        }
        if let Some(cached) = self.view.borrow_mut().as_mut() {
            cached.graph = Some(graph.clone());
        }
        Ok(graph)
    }

    /// One object with its links and facts, read from the graph's tables. Every read compares
    /// the object with the graph built from the view; tables that differ (a save that failed)
    /// are rewritten first, so a reader never sees them drift.
    pub fn object(&self, id: &str) -> Result<Option<crate::objects::Detail>, String> {
        let graph = self.graph()?;
        let built = graph.objects.iter().find(|candidate| candidate.id == id);
        if self.database.graph_object(id)?.as_ref() != built {
            self.database.write_graph(&graph, &graph.hash()?)?;
        }
        let Some(object) = self.database.graph_object(id)? else {
            return Ok(None);
        };
        Ok(Some(crate::objects::Detail {
            links: self.database.graph_links(id)?,
            facts: self.database.graph_facts(Some(id))?,
            provider: crate::objects::provider_record(&self.data, &object),
            object,
        }))
    }

    /// Identifies everything a derived view is computed from, with the clock to the quarter hour.
    fn view_key(&self) -> Result<u64, String> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.data.revision.hash(&mut hasher);
        self.data.snapshot.updated_at.hash(&mut hasher);
        self.data.snapshot.transactions.len().hash(&mut hasher);
        self.recovery.is_some().hash(&mut hasher);
        chrono::Local::now().date_naive().hash(&mut hasher);
        (chrono::Utc::now().timestamp() / VIEW_CLOCK_STEP_SECONDS).hash(&mut hasher);
        // Saved place searches change matches without changing saved data.
        fs::metadata(&self.places_path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .hash(&mut hasher);
        serde_json::to_vec(&(
            &self.data.annotations,
            &self.data.settings,
            &self.data.account_links,
            &self.data.documents,
        ))
        .map_err(|error| error.to_string())?
        .hash(&mut hasher);
        Ok(hasher.finish())
    }

    /// The committed snapshot with saved edits applied and its insights derived, as the
    /// renderer reads it.
    fn derive_snapshot(&self) -> Result<(crate::insights::View, Value), String> {
        let searches = crate::places::cache::read(&self.places_path);
        derive_view(
            &self.data,
            self.recovery.clone(),
            &searches,
            chrono::Utc::now(),
            &crate::insights::calendar::LocalZone,
        )
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
            self.record(change::Change::Settings(Box::new(next)))?;
        }
        self.snapshot()
    }

    /// Applies `change`, saves the result, and appends the change to the log.
    fn record(&mut self, change: change::Change) -> Result<(), String> {
        self.begin_log()?;
        let next = change::apply(&self.data, &change)?;
        self.database.commit(&next, &change)?;
        self.data = next;
        Ok(())
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
        // Replay starts from here: earlier changes would project differently under this code.
        let next = change::reproject(&self.data, self.data.statements.clone())?;
        self.database.backup(&self.backup_path())?;
        self.database.checkpoint(&next, change::Reason::Upgrade)?;
        self.data = next;
        self.pending = None;
        Ok(true)
    }

    /// Projects saved provider data again, without the network, with these statements.
    fn reproject_cached(
        &mut self,
        statements: Vec<crate::statements::StatementFile>,
    ) -> Result<(), String> {
        self.database.backup(&self.backup_path())?;
        self.record(change::Change::Statements(statements))?;
        self.pending = None;
        Ok(())
    }

    pub fn is_recovering(&self) -> bool {
        self.recovery.is_some()
    }

    /// The finance database's file, which the replay check reads through its own connection.
    pub fn database_path(&self) -> &Path {
        &self.path
    }

    pub fn log_summary(&self) -> Result<crate::database::LogSummary, String> {
        self.database.log_summary()
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
                    database.checkpoint(&data, change::Reason::Start)?;
                    let _ = database.backup(&backup_path);
                    Ok((database, data))
                })
        };
        let (database, data, recovery) = match loaded {
            Ok((database, data)) => (database, data, None),
            Err(_) => {
                let backup = if backup_path.exists() {
                    read_backup(&backup_path).ok()
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
        let mut storage = Self {
            _process_lock: process_lock,
            path,
            database,
            data,
            pending: None,
            recovery,
            edit_backup: None,
            view: std::cell::RefCell::new(None),
            places_path: directory.join("places.sqlite3"),
            log_check: None,
        };
        // A failure here is retried before the next write.
        let _ = storage.begin_log();
        Ok(storage)
    }

    /// Starts the log from the saved state when it hasn't begun: a database saved before the
    /// log existed, or one whose first checkpoint failed.
    fn begin_log(&mut self) -> Result<(), String> {
        if self.recovery.is_none() && !self.database.has_log()? {
            self.database
                .checkpoint(&self.data, change::Reason::Start)?;
        }
        Ok(())
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
        let committed_at = chrono::DateTime::parse_from_rfc3339(started_at)
            .map(|time| time.with_timezone(&chrono::Utc))
            .unwrap_or_default();
        self.stage_refresh(
            id,
            started_at,
            revision,
            Staged {
                refresh: change::Refresh {
                    committed_at,
                    calendar_date: committed_at.date_naive(),
                    balances_fresh: true,
                    refreshed_account_ids: Vec::new(),
                    benchmark: Value::Array(Vec::new()),
                    warnings,
                    plaid_cache,
                    provider_data,
                },
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
            refresh,
            diagnostics,
        } = staged;
        if revision != self.data.revision {
            return Err("Financial data changed during refresh; refresh again".into());
        }
        self.pending = Some(PendingRefresh {
            id: id.into(),
            started_at: started_at.into(),
            revision,
            refresh,
            diagnostics,
            expires: Instant::now() + Duration::from_secs(120),
        });
        Ok(id.into())
    }

    /// Commits a staged refresh with the snapshot `change::project_refresh` made from it.
    pub fn commit(&mut self, id: &str, snapshot: Snapshot) -> Result<Value, CommitError> {
        self.ensure_writable().map_err(CommitError::Validation)?;
        self.begin_log().map_err(CommitError::Persistence)?;
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
        let next = change::finish_refresh(&self.data, &pending.refresh, snapshot)
            .map_err(CommitError::Validation)?;
        let committing = Instant::now();
        let backup_path = self.backup_path();
        self.database
            .backup(&backup_path)
            .map_err(CommitError::Persistence)?;
        self.database
            .commit(
                &next,
                &change::Change::Refresh(Box::new(pending.refresh.clone())),
            )
            .map_err(CommitError::Persistence)?;
        let warnings = next.snapshot.sync_warnings.len();
        let mut details = SyncDiagnostics {
            phase: "committed".into(),
            warning_count: warnings.min(999),
            changes: next
                .snapshot
                .last_change
                .as_ref()
                .map(crate::database::ChangeCounts::of),
            ..pending.diagnostics.clone()
        };
        details.stages.commit_ms = Some(crate::database::millis(committing.elapsed()));
        let (id, started_at) = (pending.id.clone(), pending.started_at.clone());
        self.data = next;
        self.pending = None;
        let snapshot = self.snapshot();
        details.snapshot_bytes = snapshot
            .as_ref()
            .ok()
            .and_then(|value| serde_json::to_vec(value).ok())
            .map(|bytes| bytes.len() as u64);
        let _ = self.database.record_sync_run(&SyncRun::ended(
            &id,
            &started_at,
            "committed",
            warnings,
            None,
            details,
        ));
        snapshot.map_err(CommitError::Persistence)
    }

    /// Adds what each provider used, and the fetch time, to the staged refresh's record.
    pub fn note_usage(
        &mut self,
        id: &str,
        usage: std::collections::BTreeMap<String, crate::providers::http::Usage>,
        fetch_ms: u64,
    ) {
        if let Some(pending) = self.pending.as_mut().filter(|pending| pending.id == id) {
            pending.diagnostics =
                std::mem::take(&mut pending.diagnostics).with_usage(usage, fetch_ms);
        }
    }

    /// Adds the projection time to the staged refresh's record.
    pub fn note_projection(&mut self, id: &str, elapsed: std::time::Duration) {
        if let Some(pending) = self.pending.as_mut().filter(|pending| pending.id == id) {
            pending.diagnostics.stages.project_ms = Some(crate::database::millis(elapsed));
        }
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
            pending.refresh.warnings.len(),
        );
        let _ = self.database.record_sync_run(&SyncRun::ended(
            id,
            &pending.started_at,
            "failed",
            pending.refresh.warnings.len(),
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
            self.record(change::Change::AccountLinks(next))?;
            self.pending = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
