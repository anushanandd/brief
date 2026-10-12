use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use chrono::NaiveDate;
use rusqlite::{
    backup::Backup, params, Connection, DatabaseName, OpenFlags, OptionalExtension, Transaction,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    providers::http::ProviderDiagnostic,
    rules::CategoryRule,
    settings::FinanceSettings,
    statements::StatementFile,
    storage::{Annotation, FinanceState},
    summary::{Closed, Event, SessionRecord},
};

mod migrations;

const COLLECTIONS: [&str; 10] = [
    "accounts",
    "holdings",
    "transactions",
    "trades",
    "accountMovements",
    "observedNetWorthHistory",
    "netWorthHistory",
    "accountBalanceHistory",
    "brokeragePerformance",
    "possibleDuplicateAccounts",
];
const DATABASE_SCHEMA_VERSION: u32 = 13;
const DATABASE_APPLICATION_ID: u32 = 0x4252_4946; // BRIF

pub struct Database {
    connection: Connection,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SyncDiagnostics {
    pub(crate) phase: String,
    #[serde(default)]
    pub(crate) providers: Vec<ProviderDiagnostic>,
    #[serde(default)]
    pub(crate) warning_count: usize,
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
        }
    }
}

#[derive(Clone, Serialize)]
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

const INSERT_ANNOTATION: &str = "INSERT OR REPLACE INTO annotations(transaction_id, category, reviewed, benefit_confirmed, tags_json, splits_json, refund_of, other) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

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
        let rules = self.load_rules()?;
        let statements = self.load_statements()?;
        Ok(Some(FinanceState {
            schema_version,
            revision,
            settings,
            rules,
            statements,
            snapshot: serde_json::from_value(Value::Object(snapshot))
                .map_err(|error| format!("Invalid financial snapshot: {error}"))?,
            plaid_cache: serde_json::from_str(&plaid_cache).map_err(json_error)?,
            provider_data: serde_json::from_str(&provider_data).map_err(json_error)?,
            annotations,
            account_links,
        }))
    }

    pub fn replace(&mut self, state: &FinanceState) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_state(&transaction, state)?;
        transaction.commit().map_err(db_error)
    }

    pub fn erase(&mut self, state: &FinanceState) -> Result<(), String> {
        self.connection
            .pragma_update(None, "secure_delete", true)
            .map_err(db_error)?;
        let transaction = self.connection.transaction().map_err(db_error)?;
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
        transaction.commit().map_err(db_error)?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA secure_delete = OFF;")
            .map_err(db_error)
    }

    pub fn replace_annotations(
        &mut self,
        annotations: &BTreeMap<String, Annotation>,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        transaction
            .execute("DELETE FROM annotations", [])
            .map_err(db_error)?;
        for (id, annotation) in annotations {
            write_annotation(&transaction, id, annotation)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn replace_account_links(
        &mut self,
        links: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        transaction
            .execute("DELETE FROM account_links", [])
            .map_err(db_error)?;
        for (plaid, snaptrade) in links {
            transaction
                .prepare_cached("INSERT INTO account_links(plaid_account_id, snaptrade_account_id) VALUES (?1, ?2)").map_err(db_error)?.execute(params![plaid, snaptrade],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    pub fn replace_settings(&mut self, settings: &FinanceSettings) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_settings(&transaction, settings)?;
        transaction.commit().map_err(db_error)
    }

    pub fn replace_rules(&mut self, rules: &[CategoryRule]) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_rules(&transaction, rules)?;
        transaction.commit().map_err(db_error)
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
        let mut statement = self
            .connection
            .prepare(
                "SELECT transaction_id, category, reviewed, benefit_confirmed, tags_json, splits_json, refund_of, other FROM annotations ORDER BY transaction_id",
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
                    },
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(db_error)?;
        let mut values = BTreeMap::new();
        for row in rows {
            let (id, mut annotation, tags, splits) = row.map_err(db_error)?;
            annotation.tags = tags
                .map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(json_error)?;
            annotation.splits = splits
                .map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(json_error)?;
            values.insert(id, annotation);
        }
        Ok(values)
    }

    fn load_rules(&self) -> Result<Vec<CategoryRule>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT value_json FROM category_rules ORDER BY ordinal")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        let mut rules = Vec::new();
        for row in rows {
            rules.push(serde_json::from_str(&row.map_err(db_error)?).map_err(json_error)?);
        }
        Ok(rules)
    }

    fn load_statements(&self) -> Result<Vec<StatementFile>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, account_id, file_name, imported_at, contents FROM statement_files ORDER BY imported_at, id",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(StatementFile {
                    id: row.get(0)?,
                    account_id: row.get(1)?,
                    file_name: row.get(2)?,
                    imported_at: row.get(3)?,
                    contents: row.get(4)?,
                })
            })
            .map_err(db_error)?;
        rows.collect::<Result<_, _>>().map_err(db_error)
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

fn write_rules(transaction: &Transaction<'_>, rules: &[CategoryRule]) -> Result<(), String> {
    transaction
        .execute("DELETE FROM category_rules", [])
        .map_err(db_error)?;
    for (ordinal, rule) in rules.iter().enumerate() {
        transaction
            .prepare_cached(
                "INSERT INTO category_rules(id, ordinal, value_json) VALUES (?1, ?2, ?3)",
            )
            .map_err(db_error)?
            .execute(params![
                rule.id,
                ordinal,
                serde_json::to_string(rule).map_err(json_error)?
            ])
            .map_err(db_error)?;
    }
    Ok(())
}

fn write_state(transaction: &Transaction<'_>, state: &FinanceState) -> Result<(), String> {
    write_settings(transaction, &state.settings)?;
    write_rules(transaction, &state.rules)?;
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
    transaction
        .execute("DELETE FROM annotations", [])
        .map_err(db_error)?;
    for (id, annotation) in &state.annotations {
        write_annotation(transaction, id, annotation)?;
    }
    transaction
        .execute("DELETE FROM statement_files", [])
        .map_err(db_error)?;
    for file in &state.statements {
        transaction
            .prepare_cached(
                "INSERT INTO statement_files(id, account_id, file_name, imported_at, contents) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(db_error)?
            .execute(params![
                file.id,
                file.account_id,
                file.file_name,
                file.imported_at,
                file.contents
            ])
            .map_err(db_error)?;
    }
    transaction
        .execute("DELETE FROM account_links", [])
        .map_err(db_error)?;
    for (plaid, snaptrade) in &state.account_links {
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
        "accounts" | "transactions" | "trades" | "accountMovements" => "id",
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
