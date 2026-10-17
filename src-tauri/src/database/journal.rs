//! The per-record change log that versions 17 to 19 appended to: each change to what Plaid
//! reported (transactions and balances) and to annotations and imported statements.
//!
//! The changeset log (`changesets`) records every whole record each change carried, so this
//! one is no longer written. Its rows are history and stay as they are; triggers still refuse
//! updates and deletes, and only erasure drops the table.
use rusqlite::Connection;

use super::db_error;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ledger_events (
       seq INTEGER PRIMARY KEY AUTOINCREMENT,
       recorded_at TEXT NOT NULL,
       revision INTEGER NOT NULL CHECK (revision >= 0),
       source TEXT NOT NULL CHECK (source IN ('plaid_transaction', 'plaid_balance', 'annotation', 'statement')),
       kind TEXT NOT NULL CHECK (kind IN ('baseline', 'added', 'modified', 'removed')),
       entity_id TEXT NOT NULL CHECK (length(entity_id) BETWEEN 1 AND 300),
       payload_json TEXT CHECK (payload_json IS NULL OR json_valid(payload_json)),
       CHECK ((kind = 'removed') = (payload_json IS NULL))
     ) STRICT;
     CREATE INDEX IF NOT EXISTS ledger_events_entity ON ledger_events(source, entity_id, seq);
     CREATE TRIGGER IF NOT EXISTS ledger_events_no_update BEFORE UPDATE ON ledger_events
       BEGIN SELECT RAISE(ABORT, 'The change log is append-only'); END;
     CREATE TRIGGER IF NOT EXISTS ledger_events_no_delete BEFORE DELETE ON ledger_events
       BEGIN SELECT RAISE(ABORT, 'The change log is append-only'); END;";

/// Erasure drops the log with everything else and starts an empty one.
pub(super) fn clear(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(&format!("DROP TABLE IF EXISTS ledger_events; {SCHEMA}"))
        .map_err(db_error)
}
