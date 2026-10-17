use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use chrono::NaiveDate;
use rusqlite::{
    backup::Backup, params, Connection, DatabaseName, OpenFlags, OptionalExtension, Transaction,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    providers::http::ProviderDiagnostic,
    settings::FinanceSettings,
    statements::StatementFile,
    storage::{
        change::{self, Change, Checkpoint, Reason},
        Annotation, FinanceState,
    },
    summary::{Closed, Event, SessionRecord},
};

mod bank_statements;
mod blobs;
mod changesets;
mod graph;
mod journal;
mod migrations;

pub use bank_statements::{BankStatement, StatementCheck};
pub(crate) use blobs::sha256;
pub use changesets::LogSummary;
pub(crate) use changesets::{Changeset, CHECKPOINT};

const COLLECTIONS: [&str; 11] = [
    "accounts",
    "holdings",
    "transactions",
    "trades",
    "accountMovements",
    "transactionChanges",
    "observedNetWorthHistory",
    "netWorthHistory",
    "accountBalanceHistory",
    "brokeragePerformance",
    "possibleDuplicateAccounts",
];
const DATABASE_SCHEMA_VERSION: u32 = 21;
const DATABASE_APPLICATION_ID: u32 = 0x4252_4946; // BRIF

pub struct Database {
    connection: Connection,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, ts_rs::TS)]
#[serde(default, rename_all = "camelCase")]
pub struct SyncDiagnostics {
    pub(crate) phase: String,
    #[serde(default)]
    pub(crate) providers: Vec<ProviderDiagnostic>,
    #[serde(default)]
    pub(crate) warning_count: usize,
    /// What each provider used, failed or not: counts and timings, never names or amounts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) runs: Vec<ProviderRun>,
    #[serde(default)]
    pub(crate) stages: StageTimes,
    /// The size of the snapshot the renderer received, in bytes of JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) snapshot_bytes: Option<u64>,
    /// How many saved transactions the refresh added, settled, removed, or amended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) changes: Option<ChangeCounts>,
}

/// Counts of what a refresh changed in saved transactions.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, ts_rs::TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ChangeCounts {
    pub(crate) added: u32,
    pub(crate) settled: u32,
    pub(crate) removed: u32,
    pub(crate) amended: u32,
    pub(crate) backfilled: u32,
}

impl ChangeCounts {
    pub(crate) fn of(change: &crate::finance_contract::Change) -> Self {
        use crate::finance_contract::TransactionChangeKind as Kind;
        let count = |kind: Kind| {
            change
                .transaction_changes
                .iter()
                .filter(|change| change.kind == kind)
                .count() as u32
        };
        Self {
            added: count(Kind::Added),
            settled: count(Kind::Settled),
            removed: count(Kind::Removed),
            amended: count(Kind::Amended),
            backfilled: u32::try_from(change.backfilled).unwrap_or(u32::MAX),
        }
    }
}

/// What one provider used during a refresh.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, ts_rs::TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderRun {
    pub(crate) provider: String,
    pub(crate) requests: u32,
    pub(crate) retries: u32,
    pub(crate) bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) duration_ms: Option<u64>,
}

/// How long each stage of a refresh took, in milliseconds.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, ts_rs::TS)]
#[serde(default, rename_all = "camelCase")]
pub struct StageTimes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) fetch_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) project_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) commit_ms: Option<u64>,
}

/// Whole milliseconds, saturating.
pub(crate) fn millis(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

impl SyncDiagnostics {
    pub(crate) fn new(
        phase: &str,
        providers: Vec<ProviderDiagnostic>,
        warning_count: usize,
    ) -> Self {
        Self {
            phase: phase.chars().take(32).collect(),
            providers: providers.into_iter().take(24).collect(),
            warning_count: warning_count.min(999),
            ..Self::default()
        }
    }

    /// These diagnostics with each provider's usage and the time spent fetching.
    pub(crate) fn with_usage(
        mut self,
        usage: BTreeMap<String, crate::providers::http::Usage>,
        fetch_ms: u64,
    ) -> Self {
        self.runs = usage
            .into_iter()
            .take(12)
            .map(|(provider, usage)| ProviderRun {
                provider: provider.chars().take(32).collect(),
                requests: usage.requests,
                retries: usage.retries,
                bytes: usage.bytes,
                duration_ms: usage.duration_ms,
            })
            .collect();
        self.stages.fetch_ms = Some(fetch_ms);
        self
    }
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub(crate) id: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
    pub(crate) outcome: String,
    pub(crate) warnings: Vec<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) details: SyncDiagnostics,
}

impl SyncRun {
    /// A run that ends now. Its warnings are kept only as a count.
    pub(crate) fn ended(
        id: &str,
        started_at: &str,
        outcome: &str,
        warnings: usize,
        error_code: Option<&str>,
        details: SyncDiagnostics,
    ) -> Self {
        Self {
            id: id.into(),
            started_at: started_at.into(),
            finished_at: chrono::Utc::now().to_rfc3339(),
            outcome: outcome.into(),
            warnings: if warnings == 0 {
                Vec::new()
            } else {
                vec![format!("{warnings} warning(s)")]
            },
            error_code: error_code.map(str::to_owned),
            details,
        }
    }
}

const INSERT_ANNOTATION: &str = "INSERT OR REPLACE INTO annotations(transaction_id, category, reviewed, benefit_confirmed, tags_json, splits_json, refund_of, other, benefit_id, note, people_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";

fn write_documents(
    transaction: &Transaction<'_>,
    documents: &[crate::documents::Document],
) -> Result<(), String> {
    transaction
        .execute("DELETE FROM documents", [])
        .map_err(db_error)?;
    for document in documents {
        transaction
            .prepare_cached("INSERT INTO documents(id, value_json) VALUES (?1, ?2)")
            .map_err(db_error)?
            .execute(params![
                document.id,
                serde_json::to_string(document).map_err(json_error)?
            ])
            .map_err(db_error)?;
    }
    Ok(())
}

fn write_annotations(
    transaction: &Transaction<'_>,
    annotations: &BTreeMap<String, Annotation>,
) -> Result<(), String> {
    transaction
        .execute("DELETE FROM annotations", [])
        .map_err(db_error)?;
    for (id, annotation) in annotations {
        write_annotation(transaction, id, annotation)?;
    }
    Ok(())
}

fn write_account_links(
    transaction: &Transaction<'_>,
    links: &BTreeMap<String, String>,
) -> Result<(), String> {
    transaction
        .execute("DELETE FROM account_links", [])
        .map_err(db_error)?;
    for (plaid, snaptrade) in links {
        transaction
            .prepare_cached(
                "INSERT INTO account_links(plaid_account_id, snaptrade_account_id) VALUES (?1, ?2)",
            )
            .map_err(db_error)?
            .execute(params![plaid, snaptrade])
            .map_err(db_error)?;
    }
    Ok(())
}

/// Appends `change` and the hash of the state it produced, then a periodic checkpoint when
/// replay would otherwise grow long.
fn record(
    transaction: &Transaction<'_>,
    next: &FinanceState,
    change: &Change,
) -> Result<(), String> {
    let state = change::state_hash(next)?;
    changesets::append(transaction, change.kind(), &change.body()?, &state)?;
    if !matches!(change, Change::Checkpoint(_))
        && changesets::since_checkpoint_count(transaction)? >= change::CHECKPOINT_INTERVAL
    {
        let checkpoint = Change::Checkpoint(Checkpoint {
            reason: Reason::Periodic,
            state: Box::new(next.clone()),
        });
        changesets::append(transaction, checkpoint.kind(), &checkpoint.body()?, &state)?;
    }
    Ok(())
}

fn write_annotation(
    transaction: &Transaction<'_>,
    id: &str,
    annotation: &Annotation,
) -> Result<(), String> {
    let tags = annotation
        .tags
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_error)?;
    let splits = annotation
        .splits
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_error)?;
    let people = annotation
        .people
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_error)?;
    transaction
        .prepare_cached(INSERT_ANNOTATION)
        .map_err(db_error)?
        .execute(params![
            id,
            annotation.category,
            annotation.reviewed,
            annotation.benefit_confirmed,
            tags,
            splits,
            annotation.refund_of,
            annotation.other,
            annotation.benefit_id,
            annotation.note,
            people,
        ])
        .map_err(db_error)?;
    Ok(())
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut connection = Connection::open(path).map_err(db_error)?;
        secure_file(path)?;
        let version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .map_err(db_error)?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err("The finance database requires a newer version of Brief".into());
        }
        migrations::migrate_schema(&connection, version)?;
        if version < 5 {
            migrations::scrub_legacy_diagnostics(&mut connection)?;
        }
        if version < DATABASE_SCHEMA_VERSION {
            connection
                .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION)
                .map_err(db_error)?;
        }
        Ok(Self { connection })
    }

    pub fn load(&self) -> Result<Option<FinanceState>, String> {
        let meta = self
            .connection
            .query_row(
                "SELECT schema_version, revision, plaid_cache_json, provider_data_json FROM state_meta WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        let Some((schema_version, revision, plaid_cache, provider_data)) = meta else {
            return Ok(None);
        };
        let mut snapshot = Map::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT key, value_json FROM snapshot_scalar ORDER BY key")
                .map_err(db_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(db_error)?;
            for row in rows {
                let (key, json) = row.map_err(db_error)?;
                snapshot.insert(key, serde_json::from_str(&json).map_err(json_error)?);
            }
        }
        for section in COLLECTIONS {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT value_json FROM snapshot_entity WHERE section = ?1 ORDER BY ordinal",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map([section], |row| row.get::<_, String>(0))
                .map_err(db_error)?;
            let mut values = Vec::new();
            for row in rows {
                values.push(
                    serde_json::from_str::<Value>(&row.map_err(db_error)?).map_err(json_error)?,
                );
            }
            snapshot.insert(section.into(), values.into());
        }
        let annotations = self.load_annotations()?;
        let account_links = self.load_account_links()?;
        snapshot.insert(
            "accountLinks".into(),
            serde_json::to_value(&account_links).map_err(json_error)?,
        );
        let settings = self
            .connection
            .query_row(
                "SELECT value_json FROM finance_settings WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)?
            .map(|json| serde_json::from_str::<FinanceSettings>(&json).map_err(json_error))
            .transpose()?
            .unwrap_or_default();
        let statements = self.load_statements()?;
        let documents = self.load_documents()?;
        Ok(Some(FinanceState {
            schema_version,
            revision,
            settings,
            statements,
            documents,
            snapshot: serde_json::from_value(Value::Object(snapshot))
                .map_err(|error| format!("Invalid financial snapshot: {error}"))?,
            plaid_cache: serde_json::from_str(&plaid_cache).map_err(json_error)?,
            provider_data: serde_json::from_str(&provider_data).map_err(json_error)?,
            annotations,
            account_links,
        }))
    }

    /// Saves `next`, the state `change` produced, and appends the change to the log, in one
    /// transaction: a failure saves neither.
    pub fn commit(&mut self, next: &FinanceState, change: &Change) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        record(&transaction, next, change)?;
        match change {
            Change::Annotations { .. } => write_annotations(&transaction, &next.annotations)?,
            Change::Settings(_) => write_settings(&transaction, &next.settings)?,
            Change::AccountLinks(_) => write_account_links(&transaction, &next.account_links)?,
            Change::Documents(_) => write_documents(&transaction, &next.documents)?,
            Change::Checkpoint(_) | Change::Refresh(_) | Change::Statements(_) => {
                write_state(&transaction, next)?
            }
        }
        transaction.commit().map_err(db_error)
    }

    /// Saves a whole state, which the log records as a checkpoint.
    pub fn checkpoint(&mut self, state: &FinanceState, reason: Reason) -> Result<(), String> {
        let change = Change::Checkpoint(Checkpoint {
            reason,
            state: Box::new(state.clone()),
        });
        self.commit(state, &change)
    }

    /// Rewrites the tables from the state the log replays to. The tables are the log's cache,
    /// so nothing is appended: the log already holds this state.
    pub fn rebuild_tables(&mut self, state: &FinanceState) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_state(&transaction, state)?;
        transaction.commit().map_err(db_error)
    }

    /// Whether the log has begun.
    pub fn has_log(&self) -> Result<bool, String> {
        Ok(changesets::latest(&self.connection)?.is_some())
    }

    /// The hash of the state the latest changeset produced.
    pub fn logged_state(&self) -> Result<Option<String>, String> {
        Ok(changesets::latest(&self.connection)?.map(|changeset| changeset.state))
    }

    /// The latest checkpoint and every changeset since, oldest first, for replay.
    pub(crate) fn since_checkpoint(&self) -> Result<Vec<Changeset>, String> {
        changesets::since_checkpoint(&self.connection)
    }

    /// The change a changeset carried.
    pub(crate) fn change(&self, changeset: &Changeset) -> Result<Change, String> {
        if changeset.format > changesets::FORMAT {
            return Err("The change log was written by a newer version of Brief".into());
        }
        Change::from_stored(
            &changeset.kind,
            changesets::body(&self.connection, changeset)?,
        )
    }

    pub fn log_summary(&self) -> Result<LogSummary, String> {
        changesets::summary(&self.connection)
    }

    /// Opens the finance database to read the log beside the open connection.
    pub fn open_read_only(path: &Path) -> Result<Self, String> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(db_error)?;
        Ok(Self { connection })
    }

    pub fn erase(&mut self, state: &FinanceState) -> Result<(), String> {
        self.connection
            .pragma_update(None, "secure_delete", true)
            .map_err(db_error)?;
        let transaction = self.connection.transaction().map_err(db_error)?;
        changesets::clear(&transaction)?;
        record(
            &transaction,
            state,
            &Change::Checkpoint(Checkpoint {
                reason: Reason::Erase,
                state: Box::new(state.clone()),
            }),
        )?;
        write_state(&transaction, state)?;
        transaction
            .execute("DELETE FROM sync_runs", [])
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM market_sessions", [])
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM market_events", [])
            .map_err(db_error)?;
        Self::clear_graph(&transaction)?;
        transaction
            .execute("DELETE FROM bank_statements", [])
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM bank_statement_checks", [])
            .map_err(db_error)?;
        journal::clear(&transaction)?;
        transaction.commit().map_err(db_error)?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA secure_delete = OFF;")
            .map_err(db_error)
    }

    pub fn record_sync_run(&mut self, run: &SyncRun) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_sync_run(&transaction, run)?;
        transaction.commit().map_err(db_error)
    }

    pub fn backup(&self, path: &Path) -> Result<(), String> {
        self.connection
            .backup(DatabaseName::Main, path, None)
            .map_err(db_error)?;
        secure_file(path)
    }

    pub fn copy_backup(source: &Path, destination: &Path) -> Result<(), String> {
        let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| "Backup cannot be read".to_string())?;
        connection
            .backup(DatabaseName::Main, destination, None)
            .map_err(|_| "Backup cannot be copied".to_string())?;
        secure_file(destination)
    }

    pub fn restore_from(&mut self, source: &Database) -> Result<(), String> {
        Backup::new(&source.connection, &mut self.connection)
            .and_then(|backup| backup.run_to_completion(128, Duration::from_millis(10), None))
            .map_err(db_error)
    }

    pub fn validate_backup(path: &Path) -> Result<(), String> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| "Backup is not a readable SQLite database".to_string())?;
        let application_id = connection
            .query_row("PRAGMA application_id", [], |row| row.get::<_, u32>(0))
            .map_err(db_error)?;
        let has_brief_state = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'state_meta')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(db_error)?;
        if application_id != DATABASE_APPLICATION_ID && !(application_id == 0 && has_brief_state) {
            return Err("This is not a Brief backup".into());
        }
        let version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .map_err(db_error)?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err("This backup requires a newer version of Brief".into());
        }
        let integrity = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        if integrity != "ok" {
            return Err("The backup failed SQLite integrity validation".into());
        }
        let foreign_key_errors = connection
            .prepare("PRAGMA foreign_key_check")
            .and_then(|mut statement| statement.exists([]))
            .map_err(db_error)?;
        if foreign_key_errors {
            return Err("The backup contains invalid references".into());
        }
        Ok(())
    }

    /// One JSON column's rows, in the query's order. A row that no longer reads as `T` is
    /// left out, so what it held is computed again.
    fn json_rows<T: DeserializeOwned>(
        &self,
        sql: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Vec<T>, String> {
        let mut statement = self.connection.prepare(sql).map_err(db_error)?;
        let rows = statement
            .query_map(parameters, |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        let mut records = Vec::new();
        for row in rows {
            if let Ok(record) = serde_json::from_str(&row.map_err(db_error)?) {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// Saved market sessions after `after` through `through`, oldest first.
    pub fn market_sessions(
        &self,
        after: NaiveDate,
        through: NaiveDate,
    ) -> Result<Vec<SessionRecord>, String> {
        self.json_rows(
            "SELECT record_json FROM market_sessions WHERE session > ?1 AND session <= ?2 ORDER BY session",
            params![after.to_string(), through.to_string()],
        )
    }

    /// The latest `limit` saved market sessions, newest first.
    pub fn latest_market_sessions(&self, limit: usize) -> Result<Vec<SessionRecord>, String> {
        self.json_rows(
            "SELECT record_json FROM market_sessions ORDER BY session DESC LIMIT ?1",
            [limit as i64],
        )
    }

    /// Saved events in sessions after `after` through `through`, oldest first.
    pub fn market_events(
        &self,
        after: NaiveDate,
        through: NaiveDate,
    ) -> Result<Vec<Event>, String> {
        self.json_rows(
            "SELECT record_json FROM market_events WHERE session > ?1 AND session <= ?2 ORDER BY session, id",
            params![after.to_string(), through.to_string()],
        )
    }

    /// Saves closed sessions and their events in one transaction. A session saved again (by
    /// another method) replaces its events with the new ones; an event of a session not being
    /// saved is ignored.
    pub fn save_market_sessions(&mut self, closed: &Closed) -> Result<(), String> {
        let sessions: std::collections::BTreeSet<NaiveDate> = closed
            .sessions
            .iter()
            .map(|record| record.session)
            .collect();
        let transaction = self.connection.transaction().map_err(db_error)?;
        for session in &sessions {
            transaction
                .execute(
                    "DELETE FROM market_events WHERE session = ?1",
                    [session.to_string()],
                )
                .map_err(db_error)?;
        }
        for event in closed
            .events
            .iter()
            .filter(|event| sessions.contains(&event.session))
        {
            transaction
                .prepare_cached("INSERT OR REPLACE INTO market_events(id, session, family, symbol, record_json) VALUES (?1, ?2, ?3, ?4, ?5)")
                .map_err(db_error)?
                .execute(params![
                    event.id,
                    event.session.to_string(),
                    event.family.name(),
                    event.symbol,
                    serde_json::to_string(event).map_err(json_error)?
                ])
                .map_err(db_error)?;
        }
        for record in &closed.sessions {
            transaction
                .prepare_cached("INSERT OR REPLACE INTO market_sessions(session, method, computed_at, record_json) VALUES (?1, ?2, ?3, ?4)")
                .map_err(db_error)?
                .execute(params![
                    record.session.to_string(),
                    record.method,
                    record.computed_at,
                    serde_json::to_string(record).map_err(json_error)?
                ])
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn sync_runs(&self) -> Result<Vec<SyncRun>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id, started_at, finished_at, outcome, warnings_json, error_code, details_json FROM sync_runs ORDER BY finished_at DESC LIMIT 100")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(db_error)?;
        let mut runs = Vec::new();
        for row in rows {
            let (id, started_at, finished_at, outcome, warnings, error_code, details) =
                row.map_err(db_error)?;
            runs.push(SyncRun {
                id,
                started_at,
                finished_at,
                outcome,
                warnings: serde_json::from_str(&warnings).map_err(json_error)?,
                error_code,
                details: serde_json::from_str(&details).map_err(json_error)?,
            });
        }
        Ok(runs)
    }

    fn load_annotations(&self) -> Result<BTreeMap<String, Annotation>, String> {
        read_annotations(&self.connection)
    }

    fn load_documents(&self) -> Result<Vec<crate::documents::Document>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT value_json FROM documents ORDER BY id")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        let mut documents = Vec::new();
        for row in rows {
            documents.push(serde_json::from_str(&row.map_err(db_error)?).map_err(json_error)?);
        }
        Ok(documents)
    }

    fn load_statements(&self) -> Result<Vec<StatementFile>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, account_id, file_name, imported_at, contents, confirmed_json FROM statement_files ORDER BY imported_at, id",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    StatementFile {
                        id: row.get(0)?,
                        account_id: row.get(1)?,
                        file_name: row.get(2)?,
                        imported_at: row.get(3)?,
                        contents: row.get(4)?,
                        confirmed_rows: Default::default(),
                    },
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(db_error)?;
        let mut files = Vec::new();
        for row in rows {
            let (mut file, confirmed) = row.map_err(db_error)?;
            file.confirmed_rows = serde_json::from_str(&confirmed).map_err(json_error)?;
            files.push(file);
        }
        Ok(files)
    }

    fn load_account_links(&self) -> Result<BTreeMap<String, String>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT plaid_account_id, snaptrade_account_id FROM account_links")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(db_error)?;
        let mut values = BTreeMap::new();
        for row in rows {
            let (plaid, snaptrade) = row.map_err(db_error)?;
            values.insert(plaid, snaptrade);
        }
        Ok(values)
    }
}

fn read_annotations(connection: &Connection) -> Result<BTreeMap<String, Annotation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT transaction_id, category, reviewed, benefit_confirmed, tags_json, splits_json, refund_of, other, benefit_id, note, people_json FROM annotations ORDER BY transaction_id",
        )
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                Annotation {
                    category: row.get(1)?,
                    reviewed: row.get(2)?,
                    benefit_confirmed: row.get(3)?,
                    tags: None,
                    splits: None,
                    refund_of: row.get(6)?,
                    other: row.get(7)?,
                    benefit_id: row.get(8)?,
                    note: row.get(9)?,
                    people: None,
                    clear_benefit: false,
                },
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(10)?,
            ))
        })
        .map_err(db_error)?;
    let mut values = BTreeMap::new();
    for row in rows {
        let (id, mut annotation, tags, splits, people) = row.map_err(db_error)?;
        annotation.tags = tags
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(json_error)?;
        annotation.splits = splits
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(json_error)?;
        annotation.people = people
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(json_error)?;
        values.insert(id, annotation);
    }
    Ok(values)
}

fn write_sync_run(transaction: &Transaction<'_>, run: &SyncRun) -> Result<(), String> {
    let warnings = serde_json::to_string(&run.warnings).map_err(json_error)?;
    let details = serde_json::to_string(&run.details).map_err(json_error)?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO sync_runs(id, started_at, finished_at, outcome, warnings_json, error_code, details_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![run.id, run.started_at, run.finished_at, run.outcome, warnings, run.error_code, details],
        )
        .map_err(db_error)?;
    transaction
        .execute(
            "DELETE FROM sync_runs WHERE id NOT IN (SELECT id FROM sync_runs ORDER BY finished_at DESC LIMIT 100)",
            [],
        )
        .map_err(db_error)?;
    Ok(())
}

fn write_settings(transaction: &Transaction<'_>, settings: &FinanceSettings) -> Result<(), String> {
    transaction
        .execute(
            "INSERT OR REPLACE INTO finance_settings(id, value_json) VALUES (1, ?1)",
            [serde_json::to_string(settings).map_err(json_error)?],
        )
        .map_err(db_error)?;
    Ok(())
}

fn write_state(transaction: &Transaction<'_>, state: &FinanceState) -> Result<(), String> {
    write_settings(transaction, &state.settings)?;
    // Category rules were retired; a database that still has the table keeps it empty.
    transaction
        .execute("DELETE FROM category_rules", [])
        .map_err(db_error)?;
    transaction
        .execute(
            "INSERT INTO state_meta(id, schema_version, revision, plaid_cache_json, provider_data_json)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET schema_version=excluded.schema_version, revision=excluded.revision,
               plaid_cache_json=excluded.plaid_cache_json, provider_data_json=excluded.provider_data_json",
            params![
                state.schema_version,
                state.revision,
                serde_json::to_string(&state.plaid_cache).map_err(json_error)?,
                serde_json::to_string(&state.provider_data).map_err(json_error)?,
            ],
        )
        .map_err(db_error)?;
    transaction
        .execute("DELETE FROM snapshot_scalar", [])
        .map_err(db_error)?;
    transaction
        .execute("DELETE FROM snapshot_entity", [])
        .map_err(db_error)?;
    let Value::Object(snapshot) = serde_json::to_value(&state.snapshot).map_err(json_error)? else {
        return Err("Snapshot must be an object".into());
    };
    for (key, value) in &snapshot {
        if key == "accountLinks" {
            continue;
        }
        if COLLECTIONS.contains(&key.as_str()) {
            for (ordinal, entity) in value
                .as_array()
                .ok_or("Snapshot collection must be an array")?
                .iter()
                .enumerate()
            {
                let id = entity_id(key, entity, ordinal);
                transaction.prepare_cached("INSERT INTO snapshot_entity(section, ordinal, entity_id, value_json) VALUES (?1, ?2, ?3, ?4)").map_err(db_error)?.execute(params![key, ordinal, id, serde_json::to_string(entity).map_err(json_error)?],
                ).map_err(db_error)?;
            }
        } else {
            transaction
                .prepare_cached("INSERT INTO snapshot_scalar(key, value_json) VALUES (?1, ?2)")
                .map_err(db_error)?
                .execute(params![
                    key,
                    serde_json::to_string(value).map_err(json_error)?
                ])
                .map_err(db_error)?;
        }
    }
    write_annotations(transaction, &state.annotations)?;
    transaction
        .execute("DELETE FROM statement_files", [])
        .map_err(db_error)?;
    for file in &state.statements {
        transaction
            .prepare_cached(
                "INSERT INTO statement_files(id, account_id, file_name, imported_at, contents, confirmed_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(db_error)?
            .execute(params![
                file.id,
                file.account_id,
                file.file_name,
                file.imported_at,
                file.contents,
                serde_json::to_string(&file.confirmed_rows).map_err(json_error)?
            ])
            .map_err(db_error)?;
    }
    write_documents(transaction, &state.documents)?;
    write_account_links(transaction, &state.account_links)
}

fn entity_id(section: &str, value: &Value, ordinal: usize) -> String {
    if section == "possibleDuplicateAccounts" {
        return match (
            value.get("plaidAccountId").and_then(Value::as_str),
            value.get("snaptradeAccountId").and_then(Value::as_str),
        ) {
            (Some(plaid), Some(snaptrade)) => format!("{plaid}|{snaptrade}"),
            _ => format!("{ordinal:08}"),
        };
    }
    let field = match section {
        "accounts" | "transactions" | "trades" | "accountMovements" | "transactionChanges" => "id",
        "accountBalanceHistory" | "brokeragePerformance" => "accountId",
        "observedNetWorthHistory" | "netWorthHistory" => "date",
        _ => "",
    };
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{ordinal:08}"))
}

fn db_error(error: rusqlite::Error) -> String {
    format!("Local finance database error: {error}")
}

fn json_error(error: serde_json::Error) -> String {
    format!("Invalid local finance data: {error}")
}

pub(crate) fn secure_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Could not protect a local data file: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
