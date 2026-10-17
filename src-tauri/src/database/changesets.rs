//! The change log, the source of truth for the finance state.
//!
//! Every write appends one changeset: what changed (a refresh's fetched provider data, a
//! statement import, an edit), stored in `blobs`, and the SHA-256 of the state it produced.
//! The saved tables are that state, kept so a read never waits on replay; replaying the
//! changesets from the latest checkpoint through `storage::change::apply` must reproduce them,
//! and the hashes say where it doesn't. Each changeset names the one before it and carries
//! this database's device id and its own sequence number there, and its id is the hash of all
//! of it, so the log is a chain a second device could merge later without a new format. Rows
//! are never updated or deleted; only erasure drops the log.
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

use super::{blobs, db_error, json_error};

/// Raise when a change's body changes shape; replay reads each changeset by its format.
pub(crate) const FORMAT: u32 = 1;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS changesets (
       seq INTEGER PRIMARY KEY AUTOINCREMENT,
       id TEXT NOT NULL UNIQUE CHECK (length(id) = 64),
       parent TEXT CHECK (parent IS NULL OR length(parent) = 64),
       device TEXT NOT NULL CHECK (length(device) BETWEEN 1 AND 64),
       device_seq INTEGER NOT NULL CHECK (device_seq >= 1),
       recorded_at TEXT NOT NULL,
       format INTEGER NOT NULL,
       kind TEXT NOT NULL CHECK (length(kind) BETWEEN 1 AND 32),
       body TEXT NOT NULL REFERENCES blobs(hash),
       state TEXT NOT NULL CHECK (length(state) = 64),
       UNIQUE (device, device_seq)
     ) STRICT;
     CREATE INDEX IF NOT EXISTS changesets_kind ON changesets(kind, seq);
     CREATE TRIGGER IF NOT EXISTS changesets_no_update BEFORE UPDATE ON changesets
       BEGIN SELECT RAISE(ABORT, 'The change log is append-only'); END;
     CREATE TRIGGER IF NOT EXISTS changesets_no_delete BEFORE DELETE ON changesets
       BEGIN SELECT RAISE(ABORT, 'The change log is append-only'); END;
     CREATE TABLE IF NOT EXISTS log_device (
       id INTEGER PRIMARY KEY CHECK (id = 1),
       device TEXT NOT NULL CHECK (length(device) BETWEEN 1 AND 64)
     ) STRICT;";

/// The kind of the changeset that holds a whole state; replay starts at the latest one.
pub(crate) const CHECKPOINT: &str = "checkpoint";

/// One changeset's header; its body is read on demand.
#[derive(Clone, Debug)]
pub(crate) struct Changeset {
    pub(crate) seq: i64,
    pub(crate) format: u32,
    pub(crate) kind: String,
    pub(crate) body: String,
    pub(crate) state: String,
}

/// What the log holds, for Diagnostics.
#[derive(Clone, Debug, Default, Serialize, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LogSummary {
    pub changesets: u64,
    pub checkpoints: u64,
    /// Changesets since the latest checkpoint, which replay applies.
    pub since_checkpoint: u64,
    /// The compressed size of everything the changesets carry.
    pub stored_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub first_recorded_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_recorded_at: Option<String>,
}

/// This database's device id, made once.
fn device(connection: &Connection) -> Result<String, String> {
    connection
        .execute(
            "INSERT OR IGNORE INTO log_device(id, device) VALUES (1, ?1)",
            [uuid::Uuid::new_v4().to_string()],
        )
        .map_err(db_error)?;
    connection
        .query_row("SELECT device FROM log_device WHERE id = 1", [], |row| {
            row.get(0)
        })
        .map_err(db_error)
}

/// Appends a changeset whose change is `body`, producing the state that hashes to `state`.
pub(super) fn append(
    connection: &Connection,
    kind: &str,
    body: &Value,
    state: &str,
) -> Result<(), String> {
    let device = device(connection)?;
    let body = blobs::put(connection, body)?;
    let (parent, device_seq) = connection
        .query_row(
            "SELECT (SELECT id FROM changesets ORDER BY seq DESC LIMIT 1),
                    (SELECT coalesce(max(device_seq), 0) + 1 FROM changesets WHERE device = ?1)",
            [&device],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(db_error)?;
    let recorded_at = chrono::Utc::now().to_rfc3339();
    let header = json!([
        parent,
        device,
        device_seq,
        recorded_at,
        FORMAT,
        kind,
        body,
        state
    ]);
    let id = blobs::sha256(&serde_json::to_vec(&header).map_err(json_error)?);
    connection
        .prepare_cached(
            "INSERT INTO changesets(id, parent, device, device_seq, recorded_at, format, kind, body, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .and_then(|mut insert| {
            insert.execute(params![
                id,
                parent,
                device,
                device_seq,
                recorded_at,
                FORMAT,
                kind,
                body,
                state
            ])
        })
        .map_err(db_error)?;
    Ok(())
}

fn header(row: &rusqlite::Row<'_>) -> rusqlite::Result<Changeset> {
    Ok(Changeset {
        seq: row.get(0)?,
        format: row.get(1)?,
        kind: row.get(2)?,
        body: row.get(3)?,
        state: row.get(4)?,
    })
}

const COLUMNS: &str = "seq, format, kind, body, state";

/// The latest changeset, if any.
pub(super) fn latest(connection: &Connection) -> Result<Option<Changeset>, String> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM changesets ORDER BY seq DESC LIMIT 1"),
            [],
            header,
        )
        .optional()
        .map_err(db_error)
}

/// The latest checkpoint and every changeset after it, oldest first; empty before the first.
pub(super) fn since_checkpoint(connection: &Connection) -> Result<Vec<Changeset>, String> {
    let mut query = connection
        .prepare(&format!(
            "SELECT {COLUMNS} FROM changesets
             WHERE seq >= coalesce((SELECT max(seq) FROM changesets WHERE kind = ?1), 0)
             ORDER BY seq"
        ))
        .map_err(db_error)?;
    let rows = query
        .query_map([CHECKPOINT], header)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(rows)
}

/// How many changesets follow the latest checkpoint.
pub(super) fn since_checkpoint_count(connection: &Connection) -> Result<u64, String> {
    connection
        .query_row(
            "SELECT count(*) FROM changesets
             WHERE seq > coalesce((SELECT max(seq) FROM changesets WHERE kind = ?1), 0)",
            [CHECKPOINT],
            |row| row.get(0),
        )
        .map_err(db_error)
}

/// The change a changeset carried.
pub(super) fn body(connection: &Connection, changeset: &Changeset) -> Result<Value, String> {
    blobs::get(connection, &changeset.body)
}

/// What the log holds. It reads every stored value's size, so it is for Diagnostics only.
pub(super) fn summary(connection: &Connection) -> Result<LogSummary, String> {
    connection
        .query_row(
            "SELECT count(*),
                    count(*) FILTER (WHERE kind = ?1),
                    count(*) FILTER (WHERE seq > coalesce((SELECT max(seq) FROM changesets WHERE kind = ?1), 0)),
                    (SELECT coalesce(sum(length(data)), 0) FROM blobs),
                    min(recorded_at),
                    max(recorded_at)
             FROM changesets",
            [CHECKPOINT],
            |row| {
                Ok(LogSummary {
                    changesets: row.get(0)?,
                    checkpoints: row.get(1)?,
                    since_checkpoint: row.get(2)?,
                    stored_bytes: row.get(3)?,
                    first_recorded_at: row.get(4)?,
                    last_recorded_at: row.get(5)?,
                })
            },
        )
        .map_err(db_error)
}

/// Erasure drops the log with everything else and starts an empty one; the device id stays.
pub(super) fn clear(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(&format!(
            "DROP TABLE IF EXISTS changesets; DROP TABLE IF EXISTS blobs; {} {SCHEMA}",
            blobs::SCHEMA
        ))
        .map_err(db_error)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        database::Database,
        storage::change::{Change, Reason},
    };

    fn rows(database: &Database) -> Vec<(String, Option<String>, i64, String)> {
        let mut query = database
            .connection
            .prepare("SELECT id, parent, device_seq, kind FROM changesets ORDER BY seq")
            .unwrap();
        query
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn each_changeset_names_the_one_before_it() {
        let mut database = Database::open(Path::new(":memory:")).unwrap();
        let mut state = crate::storage::empty_state().unwrap();
        database.checkpoint(&state, Reason::Start).unwrap();
        state
            .settings
            .account_names
            .insert("plaid:card".into(), "Card".into());
        database
            .commit(&state, &Change::Settings(Box::new(state.settings.clone())))
            .unwrap();
        let rows = rows(&database);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, None);
        assert_eq!(rows[1].1.as_deref(), Some(rows[0].0.as_str()));
        assert_eq!((rows[0].2, rows[1].2), (1, 2));
        assert_eq!(
            (rows[0].3.as_str(), rows[1].3.as_str()),
            ("checkpoint", "settings")
        );
    }

    #[test]
    fn a_failed_write_appends_nothing_and_the_log_refuses_edits() {
        let mut database = Database::open(Path::new(":memory:")).unwrap();
        let mut state = crate::storage::empty_state().unwrap();
        database.checkpoint(&state, Reason::Start).unwrap();
        // Two accounts linked to one SnapTrade account break a unique constraint.
        state.account_links = std::collections::BTreeMap::from([
            ("plaid:one".into(), "snaptrade:same".into()),
            ("plaid:two".into(), "snaptrade:same".into()),
        ]);
        assert!(database
            .commit(&state, &Change::AccountLinks(state.account_links.clone()))
            .is_err());
        assert_eq!(rows(&database).len(), 1);
        for edit in [
            "UPDATE changesets SET kind = 'settings'",
            "DELETE FROM changesets",
        ] {
            assert!(database.connection.execute(edit, []).is_err());
        }
        assert_eq!(rows(&database).len(), 1);
    }
}
