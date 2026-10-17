//! The finance database's schema, created or migrated forward in place, and one-time scrubs of
//! what older versions saved.
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::*;

const DOCUMENTS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    value_json TEXT NOT NULL CHECK (json_valid(value_json))
) STRICT;";

pub(super) fn migrate_schema(connection: &Connection, version: u32) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;",
        )
        .map_err(db_error)?;
    let has_state = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'state_meta')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if version == 0 && !has_state {
        return connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE state_meta (
                   id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL,
                   revision INTEGER NOT NULL CHECK (revision >= 0),
                   plaid_cache_json TEXT NOT NULL CHECK (json_valid(plaid_cache_json)),
                   provider_data_json TEXT NOT NULL CHECK (json_valid(provider_data_json))
                 ) STRICT;
                 CREATE TABLE snapshot_scalar (
                   key TEXT PRIMARY KEY, value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE TABLE snapshot_entity (
                   section TEXT NOT NULL, ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
                   entity_id TEXT NOT NULL, value_json TEXT NOT NULL CHECK (json_valid(value_json)),
                   PRIMARY KEY (section, ordinal), UNIQUE (section, entity_id)
                 ) STRICT;
                 CREATE TABLE annotations (
                   transaction_id TEXT PRIMARY KEY, category TEXT,
                   reviewed INTEGER CHECK (reviewed IN (0, 1)),
                   benefit_confirmed INTEGER CHECK (benefit_confirmed IN (0, 1)),
                   tags_json TEXT CHECK (tags_json IS NULL OR json_valid(tags_json)),
                   splits_json TEXT CHECK (splits_json IS NULL OR json_valid(splits_json)),
                   refund_of TEXT, other INTEGER CHECK (other IN (0, 1)), benefit_id TEXT,
                   note TEXT,
                   people_json TEXT CHECK (people_json IS NULL OR json_valid(people_json))
                 ) STRICT;
                 CREATE TABLE account_links (
                   plaid_account_id TEXT PRIMARY KEY CHECK (plaid_account_id LIKE 'plaid:%'),
                   snaptrade_account_id TEXT NOT NULL UNIQUE CHECK (snaptrade_account_id LIKE 'snaptrade:%')
                 ) STRICT;
                 CREATE TABLE sync_runs (
                   id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT NOT NULL,
                   outcome TEXT NOT NULL CHECK (outcome IN ('committed', 'failed')),
                   warnings_json TEXT NOT NULL CHECK (json_valid(warnings_json)), error_code TEXT,
                   details_json TEXT NOT NULL CHECK (json_valid(details_json))
                 ) STRICT;
                 CREATE TABLE workspace (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE INDEX sync_runs_finished_at ON sync_runs(finished_at DESC);
                 CREATE TABLE finance_settings (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE TABLE category_rules (
                   id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal >= 0),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS statement_files (
                   id TEXT PRIMARY KEY CHECK (length(id) = 64),
                   account_id TEXT NOT NULL CHECK (length(account_id) BETWEEN 1 AND 220),
                   file_name TEXT NOT NULL CHECK (length(file_name) BETWEEN 1 AND 255),
                   imported_at TEXT NOT NULL,
                   contents BLOB NOT NULL CHECK (length(contents) BETWEEN 1 AND 5242880),
                   confirmed_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(confirmed_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS market_sessions (
                   session TEXT PRIMARY KEY CHECK (length(session) = 10),
                   method INTEGER NOT NULL,
                   computed_at TEXT NOT NULL,
                   record_json TEXT NOT NULL CHECK (json_valid(record_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS market_events (
                   id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 300),
                   session TEXT NOT NULL CHECK (length(session) = 10),
                   family TEXT NOT NULL,
                   symbol TEXT,
                   record_json TEXT NOT NULL CHECK (json_valid(record_json))
                 ) STRICT;
                 CREATE INDEX IF NOT EXISTS market_events_session ON market_events(session);
                 PRAGMA application_id = 1112688966;
                 PRAGMA user_version = 15;
                 COMMIT;",
            )
            .and_then(|()| connection.execute_batch(super::bank_statements::SCHEMA))
            .and_then(|()| connection.execute_batch(super::journal::SCHEMA))
            .and_then(|()| connection.execute_batch(DOCUMENTS_SCHEMA))
            .and_then(|()| connection.execute_batch(super::blobs::SCHEMA))
            .and_then(|()| connection.execute_batch(super::changesets::SCHEMA))
            .map_err(db_error);
    }

    if version < 3 {
        connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS workspace (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 DROP INDEX IF EXISTS account_links_snaptrade_unique;
                 PRAGMA user_version = 3;
                 COMMIT;",
            )
            .map_err(db_error)?;
    }
    if version < 4 {
        let has_details = connection
            .prepare("SELECT 1 FROM pragma_table_info('sync_runs') WHERE name = 'details_json'")
            .and_then(|mut statement| statement.exists([]))
            .map_err(db_error)?;
        connection
            .execute_batch(if has_details {
                "PRAGMA user_version = 4;"
            } else {
                "BEGIN IMMEDIATE;
                 ALTER TABLE sync_runs ADD COLUMN details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details_json));
                 PRAGMA user_version = 4;
                 COMMIT;"
            })
            .map_err(db_error)?;
    }
    if version < 6 {
        // Version 6 moves settings that change financial results out of renderer storage.
        connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS finance_settings (
                   id INTEGER PRIMARY KEY CHECK (id = 1),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS category_rules (
                   id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal >= 0),
                   value_json TEXT NOT NULL CHECK (json_valid(value_json))
                 ) STRICT;
                 COMMIT;",
            )
            .map_err(db_error)?;
    }
    // Version 7 adds tags, splits, and refund links to annotations. The columns are checked
    // first so a database that already has them (a fresh one, or a retried upgrade) is left alone.
    let has_tags = connection
        .prepare("SELECT 1 FROM pragma_table_info('annotations') WHERE name = 'tags_json'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_tags {
        connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE annotations ADD COLUMN tags_json TEXT CHECK (tags_json IS NULL OR json_valid(tags_json));
                 ALTER TABLE annotations ADD COLUMN splits_json TEXT CHECK (splits_json IS NULL OR json_valid(splits_json));
                 ALTER TABLE annotations ADD COLUMN refund_of TEXT;
                 COMMIT;",
            )
            .map_err(db_error)?;
    }
    // Version 13 marks transactions that are someone else's money.
    let has_other = connection
        .prepare("SELECT 1 FROM pragma_table_info('annotations') WHERE name = 'other'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_other {
        connection
            .execute(
                "ALTER TABLE annotations ADD COLUMN other INTEGER CHECK (other IN (0, 1))",
                [],
            )
            .map_err(db_error)?;
    }
    // Version 15 assigns a transaction to a card benefit by hand.
    let has_benefit_id = connection
        .prepare("SELECT 1 FROM pragma_table_info('annotations') WHERE name = 'benefit_id'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_benefit_id {
        connection
            .execute("ALTER TABLE annotations ADD COLUMN benefit_id TEXT", [])
            .map_err(db_error)?;
    }
    // Version 16 keeps a note on a transaction.
    let has_note = connection
        .prepare("SELECT 1 FROM pragma_table_info('annotations') WHERE name = 'note'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_note {
        connection
            .execute("ALTER TABLE annotations ADD COLUMN note TEXT", [])
            .map_err(db_error)?;
    }
    // Version 18 keeps the people chosen for a transaction.
    let has_people = connection
        .prepare("SELECT 1 FROM pragma_table_info('annotations') WHERE name = 'people_json'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_people {
        connection
            .execute(
                "ALTER TABLE annotations ADD COLUMN people_json TEXT CHECK (people_json IS NULL OR json_valid(people_json))",
                [],
            )
            .map_err(db_error)?;
    }
    // Version 8 saves imported statement files; version 9, each market session's layers;
    // version 10, each session's events; version 12 drops version 11's told ledger and kept
    // narratives, retired with the generated narrative.
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS statement_files (
                   id TEXT PRIMARY KEY CHECK (length(id) = 64),
                   account_id TEXT NOT NULL CHECK (length(account_id) BETWEEN 1 AND 220),
                   file_name TEXT NOT NULL CHECK (length(file_name) BETWEEN 1 AND 255),
                   imported_at TEXT NOT NULL,
                   contents BLOB NOT NULL CHECK (length(contents) BETWEEN 1 AND 5242880)
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS market_sessions (
                   session TEXT PRIMARY KEY CHECK (length(session) = 10),
                   method INTEGER NOT NULL,
                   computed_at TEXT NOT NULL,
                   record_json TEXT NOT NULL CHECK (json_valid(record_json))
                 ) STRICT;
                 CREATE TABLE IF NOT EXISTS market_events (
                   id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 300),
                   session TEXT NOT NULL CHECK (length(session) = 10),
                   family TEXT NOT NULL,
                   symbol TEXT,
                   record_json TEXT NOT NULL CHECK (json_valid(record_json))
                 ) STRICT;
                 CREATE INDEX IF NOT EXISTS market_events_session ON market_events(session);
                 DROP TABLE IF EXISTS told;
                 DROP TABLE IF EXISTS narratives;",
        )
        .map_err(db_error)?;
    // Version 17 keeps the statement rows the user confirmed Plaid never reported.
    let has_confirmed = connection
        .prepare("SELECT 1 FROM pragma_table_info('statement_files') WHERE name = 'confirmed_json'")
        .and_then(|mut statement| statement.exists([]))
        .map_err(db_error)?;
    if !has_confirmed {
        connection
            .execute(
                "ALTER TABLE statement_files ADD COLUMN confirmed_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(confirmed_json))",
                [],
            )
            .map_err(db_error)?;
    }
    // Version 14 saves bank statement PDFs and when each connection was last checked.
    connection
        .execute_batch(super::bank_statements::SCHEMA)
        .map_err(db_error)?;
    // Version 17 kept a per-record change log, superseded by version 20's; its rows stay.
    connection
        .execute_batch(super::journal::SCHEMA)
        .map_err(db_error)?;
    // Version 19 keeps what Brief read from the user's documents; the originals live
    // encrypted beside this database.
    connection
        .execute_batch(DOCUMENTS_SCHEMA)
        .map_err(db_error)?;
    // Version 20 keeps the log of changesets the state is replayed from; `Storage` checkpoints
    // what the database already holds as its first entry.
    connection
        .execute_batch(super::blobs::SCHEMA)
        .and_then(|()| connection.execute_batch(super::changesets::SCHEMA))
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "PRAGMA application_id = {DATABASE_APPLICATION_ID};"
        ))
        .map_err(db_error)
}

pub(super) fn scrub_legacy_diagnostics(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection.transaction().map_err(db_error)?;
    if let Some((plaid_cache, provider_data)) = transaction
        .query_row(
            "SELECT plaid_cache_json, provider_data_json FROM state_meta WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(db_error)?
    {
        let mut plaid_cache: Value = serde_json::from_str(&plaid_cache).map_err(json_error)?;
        if let Some(items) = plaid_cache.get_mut("items").and_then(Value::as_object_mut) {
            for item in items.values_mut().filter_map(Value::as_object_mut) {
                item.remove("error");
                if let Some(data) = item.get_mut("data").and_then(Value::as_object_mut) {
                    data.insert("warnings".into(), Value::Array(Vec::new()));
                }
            }
        }
        let mut provider_data: Value = serde_json::from_str(&provider_data).map_err(json_error)?;
        if let Some(statuses) = provider_data
            .get_mut("syncStatus")
            .and_then(Value::as_object_mut)
        {
            for status in statuses.values_mut().filter_map(Value::as_object_mut) {
                status.insert("error".into(), Value::Null);
            }
        }
        for provider in ["plaid", "snaptrade"] {
            if let Some(data) = provider_data
                .get_mut(provider)
                .and_then(Value::as_object_mut)
            {
                data.insert("warnings".into(), Value::Array(Vec::new()));
            }
        }
        transaction
            .execute(
                "UPDATE state_meta SET plaid_cache_json = ?1, provider_data_json = ?2 WHERE id = 1",
                params![
                    serde_json::to_string(&plaid_cache).map_err(json_error)?,
                    serde_json::to_string(&provider_data).map_err(json_error)?,
                ],
            )
            .map_err(db_error)?;
    }
    transaction
        .execute(
            "UPDATE snapshot_scalar SET value_json = '[]' WHERE key = 'syncWarnings'",
            [],
        )
        .map_err(db_error)?;
    if let Some(saved) = transaction
        .query_row(
            "SELECT value_json FROM snapshot_scalar WHERE key = 'providerStatus'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error)?
    {
        let mut statuses: Value = serde_json::from_str(&saved).map_err(json_error)?;
        if let Some(statuses) = statuses.as_object_mut() {
            for status in statuses.values_mut().filter_map(Value::as_object_mut) {
                status.insert("error".into(), Value::Null);
            }
        }
        transaction
            .execute(
                "UPDATE snapshot_scalar SET value_json = ?1 WHERE key = 'providerStatus'",
                [serde_json::to_string(&statuses).map_err(json_error)?],
            )
            .map_err(db_error)?;
    }
    transaction
        .execute("UPDATE sync_runs SET warnings_json = '[]'", [])
        .map_err(db_error)?;
    transaction
        .execute_batch("PRAGMA user_version = 5;")
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)
}
