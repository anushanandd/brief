use std::{collections::BTreeMap, fs, path::Path};

use rusqlite::{params, Connection, DatabaseName, OpenFlags, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::storage::{Annotation, FinanceState};

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
const DATABASE_SCHEMA_VERSION: u32 = 3;
const DATABASE_APPLICATION_ID: u32 = 0x4252_4946; // BRIF

pub struct Database {
    connection: Connection,
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
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(db_error)?;
        secure_file(path)?;
        let version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .map_err(db_error)?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err("The finance database requires a newer version of Brief".into());
        }
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = DELETE;
                 PRAGMA synchronous = FULL;
                 BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS state_meta (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   schema_version INTEGER NOT NULL,
                   revision INTEGER NOT NULL CHECK (revision >= 0),
                   plaid_cache_json TEXT NOT NULL CHECK (json_valid(plaid_cache_json)),
                   provider_data_json TEXT NOT NULL CHECK (json_valid(provider_data_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS snapshot_scalar (
                   key TEXT PRIMARY KEY,
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS snapshot_entity (
                   section TEXT NOT NULL,
                   ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
                   entity_id TEXT NOT NULL,
                   value_json TEXT NOT NULL CHECK (json_valid(value_json)),
                   PRIMARY KEY (section, ordinal),
                   UNIQUE (section, entity_id)
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS annotations (
                   transaction_id TEXT PRIMARY KEY,
                   category TEXT,
                   reviewed INTEGER CHECK (reviewed IN (0, 1)),
                   benefit_confirmed INTEGER CHECK (benefit_confirmed IN (0, 1))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS account_links (
                   plaid_account_id TEXT PRIMARY KEY CHECK (plaid_account_id LIKE 'plaid:%'),
                   snaptrade_account_id TEXT NOT NULL UNIQUE CHECK (snaptrade_account_id LIKE 'snaptrade:%')
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS sync_runs (
                   id TEXT PRIMARY KEY,
                   started_at TEXT NOT NULL,
                   finished_at TEXT NOT NULL,
                   outcome TEXT NOT NULL CHECK (outcome IN ('committed', 'failed')),
                   warnings_json TEXT NOT NULL CHECK (json_valid(warnings_json)),
                   error_code TEXT
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS workspace (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE INDEX IF NOT EXISTS sync_runs_finished_at ON sync_runs(finished_at DESC);
                 DROP INDEX IF EXISTS account_links_snaptrade_unique;
                 PRAGMA application_id = 1112688966;
                 PRAGMA user_version = 3;
                 COMMIT;",
            )
            .map_err(db_error)?;
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
        Ok(Some(FinanceState {
            schema_version,
            revision,
            snapshot: Value::Object(snapshot),
            plaid_cache: serde_json::from_str(&plaid_cache).map_err(json_error)?,
            provider_data: serde_json::from_str(&provider_data).map_err(json_error)?,
            annotations,
            account_links,
            workspace: self
                .connection
                .query_row("SELECT value_json FROM workspace WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(db_error)?
                .map(|json| serde_json::from_str(&json).map_err(json_error))
                .transpose()?
                .unwrap_or_default(),
        }))
    }

    pub fn replace(&mut self, state: &FinanceState) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_state(&transaction, state)?;
        transaction.commit().map_err(db_error)
    }

    pub fn replace_with_sync_run(
        &mut self,
        state: &FinanceState,
        id: &str,
        started_at: &str,
        finished_at: &str,
        warnings: &[String],
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_state(&transaction, state)?;
        write_sync_run(
            &transaction,
            id,
            started_at,
            finished_at,
            "committed",
            warnings,
            None,
        )?;
        transaction.commit().map_err(db_error)
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
            transaction
                .prepare_cached("INSERT INTO annotations(transaction_id, category, reviewed, benefit_confirmed) VALUES (?1, ?2, ?3, ?4)").map_err(db_error)?.execute(params![id, annotation.category, annotation.reviewed, annotation.benefit_confirmed],
                )
                .map_err(db_error)?;
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

    pub fn record_sync_run(
        &mut self,
        id: &str,
        started_at: &str,
        finished_at: &str,
        outcome: &str,
        warnings: &[String],
        error_code: Option<&str>,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        write_sync_run(
            &transaction,
            id,
            started_at,
            finished_at,
            outcome,
            warnings,
            error_code,
        )?;
        transaction.commit().map_err(db_error)
    }

    pub fn backup(&self, path: &Path) -> Result<(), String> {
        self.connection
            .backup(DatabaseName::Main, path, None)
            .map_err(db_error)?;
        secure_file(path)
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

    #[cfg(test)]
    pub fn replace_workspace(
        &mut self,
        workspace: &crate::workspace::Workspace,
    ) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO workspace(id, value_json) VALUES (1, ?1)",
                [serde_json::to_string(workspace).map_err(json_error)?],
            )
            .map_err(db_error)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn replace_review(
        &mut self,
        id: &str,
        annotation: Option<&Annotation>,
        workspace: &crate::workspace::Workspace,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        if let Some(annotation) = annotation {
            transaction.execute("INSERT OR REPLACE INTO annotations(transaction_id, category, reviewed, benefit_confirmed) VALUES (?1, ?2, ?3, ?4)", params![id, annotation.category, annotation.reviewed, annotation.benefit_confirmed]).map_err(db_error)?;
        } else {
            transaction
                .execute("DELETE FROM annotations WHERE transaction_id = ?1", [id])
                .map_err(db_error)?;
        }
        transaction
            .execute(
                "INSERT OR REPLACE INTO workspace(id, value_json) VALUES (1, ?1)",
                [serde_json::to_string(workspace).map_err(json_error)?],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    pub fn sync_runs(&self) -> Result<Vec<SyncRun>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id, started_at, finished_at, outcome, warnings_json, error_code FROM sync_runs ORDER BY finished_at DESC LIMIT 100")
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
                ))
            })
            .map_err(db_error)?;
        let mut runs = Vec::new();
        for row in rows {
            let (id, started_at, finished_at, outcome, warnings, error_code) =
                row.map_err(db_error)?;
            runs.push(SyncRun {
                id,
                started_at,
                finished_at,
                outcome,
                warnings: serde_json::from_str(&warnings).map_err(json_error)?,
                error_code,
            });
        }
        Ok(runs)
    }

    fn load_annotations(&self) -> Result<BTreeMap<String, Annotation>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT transaction_id, category, reviewed, benefit_confirmed FROM annotations ORDER BY transaction_id",
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
                    },
                ))
            })
            .map_err(db_error)?;
        let mut values = BTreeMap::new();
        for row in rows {
            let (id, annotation) = row.map_err(db_error)?;
            values.insert(id, annotation);
        }
        Ok(values)
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

fn write_sync_run(
    transaction: &Transaction<'_>,
    id: &str,
    started_at: &str,
    finished_at: &str,
    outcome: &str,
    warnings: &[String],
    error_code: Option<&str>,
) -> Result<(), String> {
    let warnings = serde_json::to_string(warnings).map_err(json_error)?;
    transaction
        .execute(
            "INSERT OR REPLACE INTO sync_runs(id, started_at, finished_at, outcome, warnings_json, error_code) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, started_at, finished_at, outcome, warnings, error_code],
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

fn write_state(transaction: &Transaction<'_>, state: &FinanceState) -> Result<(), String> {
    transaction
        .execute(
            "INSERT OR REPLACE INTO workspace(id, value_json) VALUES (1, ?1)",
            [serde_json::to_string(&state.workspace).map_err(json_error)?],
        )
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
    let snapshot = state
        .snapshot
        .as_object()
        .ok_or("Snapshot must be an object")?;
    for (key, value) in snapshot {
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
        transaction.prepare_cached("INSERT INTO annotations(transaction_id, category, reviewed, benefit_confirmed) VALUES (?1, ?2, ?3, ?4)").map_err(db_error)?.execute(params![id, annotation.category, annotation.reviewed, annotation.benefit_confirmed],
        ).map_err(db_error)?;
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

fn secure_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Could not protect the local finance database: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_one_migration_preserves_data_and_the_unique_constraint() {
        let path =
            std::env::temp_dir().join(format!("brief-migration-{}.sqlite3", uuid::Uuid::new_v4()));
        let mut db = Database::open(&path).unwrap();
        let mut state = crate::storage::empty_state().unwrap();
        state
            .account_links
            .insert("plaid:one".into(), "snaptrade:one".into());
        db.replace(&state).unwrap();
        db.connection.execute_batch("CREATE UNIQUE INDEX account_links_snaptrade_unique ON account_links(snaptrade_account_id); PRAGMA user_version = 1;").unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.load().unwrap().unwrap().account_links,
            state.account_links
        );
        assert!(db
            .connection
            .execute(
                "INSERT INTO account_links VALUES ('plaid:two', 'snaptrade:one')",
                []
            )
            .is_err());
        let redundant: u32 = db
            .connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'account_links_snaptrade_unique'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(redundant, 0);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn normalized_store_round_trips_and_rejects_duplicate_entity_ids() {
        let mut database = Database::open(Path::new(":memory:")).unwrap();
        let schema_version = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap();
        assert_eq!(schema_version, DATABASE_SCHEMA_VERSION);
        let mut state = crate::storage::empty_state().unwrap();
        state.snapshot["transactions"] = serde_json::json!([{
            "id": "one", "merchant": "Shop", "category": "Other", "date": "2026-09-09",
            "amount": -1, "account": "Card", "pending": false
        }]);
        database.replace(&state).unwrap();
        let loaded = database.load().unwrap().unwrap();
        assert_eq!(
            loaded.snapshot["transactions"],
            state.snapshot["transactions"]
        );
        assert_eq!(loaded.snapshot["netWorth"], state.snapshot["netWorth"]);
        assert_eq!(loaded.snapshot["accountLinks"], serde_json::json!({}));

        state.snapshot["transactions"] = serde_json::json!([
            { "id": "same" }, { "id": "same" }
        ]);
        assert!(database.replace(&state).is_err());
        assert_eq!(database.load().unwrap().unwrap().snapshot, loaded.snapshot);

        state.snapshot["transactions"] = serde_json::json!([]);
        state.account_links = BTreeMap::from([
            ("plaid:one".into(), "snaptrade:same".into()),
            ("plaid:two".into(), "snaptrade:same".into()),
        ]);
        assert!(database.replace(&state).is_err());
    }
}
