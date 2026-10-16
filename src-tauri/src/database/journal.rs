//! The change log: an append-only record of each change to what Plaid reported (transactions
//! and account balances) and to the user's annotations and imported statements.
//!
//! Plaid replaces a pending transaction with a new posted one and deletes what disappears, so
//! a tip, a dropped hold, or an amended charge leaves no trace in the saved cache. Each write
//! compares what the database held with what it is about to hold, inside the same SQLite
//! transaction, and appends the difference. A write that fails appends nothing. Nothing
//! rebuilds state from the log: it is evidence for reconciliation and audit, never read to
//! compute a result. Rows are never updated or deleted; only erasure drops the table.
use std::{borrow::Cow, collections::BTreeMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::{db_error, json_error, read_annotations};
use crate::{
    providers::PlaidCache,
    statements::StatementFile,
    storage::{Annotation, FinanceState},
};

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

/// Each tracked record as saved, keyed by source and id.
type Evidence<'a> = BTreeMap<(&'static str, &'a str), Cow<'a, Value>>;

/// The tracked part of the state the database holds.
#[derive(Default)]
struct Stored {
    plaid: PlaidCache,
    annotations: BTreeMap<String, Annotation>,
    statements: Vec<(String, Value)>,
}

impl Stored {
    /// What the database holds, or `None` before its first saved state.
    fn read(connection: &Connection) -> Result<Option<Self>, String> {
        let Some(plaid) = connection
            .query_row(
                "SELECT plaid_cache_json FROM state_meta WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)?
        else {
            return Ok(None);
        };
        let statements = connection
            .prepare(
                "SELECT id, account_id, file_name, imported_at, confirmed_json FROM statement_files",
            )
            .and_then(|mut query| {
                query
                    .query_map([], |row| {
                        let confirmed = row.get::<_, String>(4)?;
                        let file = (row.get(1)?, row.get(2)?, row.get(3)?);
                        Ok((row.get::<_, String>(0)?, file, confirmed))
                    })?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(db_error)?
            .into_iter()
            .map(|(id, (account, name, imported), confirmed): (_, (String, String, String), _)| {
                let confirmed = serde_json::from_str(&confirmed).map_err(json_error)?;
                Ok((id, statement(&account, &name, &imported, confirmed)))
            })
            .collect::<Result<_, String>>()?;
        Ok(Some(Self {
            plaid: serde_json::from_str(&plaid).map_err(json_error)?,
            annotations: read_annotations(connection)?,
            statements,
        }))
    }

    fn evidence(&self) -> Result<Evidence<'_>, String> {
        evidence(
            &self.plaid,
            &self.annotations,
            self.statements
                .iter()
                .map(|(id, value)| (id.as_str(), value)),
        )
    }
}

/// A statement file is tracked by its contents' SHA-256, with the rows the user confirmed; its
/// bytes stay in `statement_files`.
fn statement(account_id: &str, file_name: &str, imported_at: &str, confirmed: Value) -> Value {
    json!({
        "accountId": account_id,
        "fileName": file_name,
        "importedAt": imported_at,
        "confirmedRows": confirmed,
    })
}

fn statement_file(file: &StatementFile) -> Result<Value, String> {
    let confirmed = serde_json::to_value(&file.confirmed_rows).map_err(json_error)?;
    Ok(statement(
        &file.account_id,
        &file.file_name,
        &file.imported_at,
        confirmed,
    ))
}

fn evidence<'a>(
    plaid: &'a PlaidCache,
    annotations: &'a BTreeMap<String, Annotation>,
    statements: impl Iterator<Item = (&'a str, &'a Value)>,
) -> Result<Evidence<'a>, String> {
    let id = |value: &'a Value, field| value.get(field).and_then(Value::as_str);
    let mut evidence = plaid
        .transactions()
        .filter_map(|transaction| {
            let key = ("plaid_transaction", id(transaction, "transaction_id")?);
            Some((key, Cow::Borrowed(transaction)))
        })
        .chain(plaid.accounts().filter_map(|account| {
            let key = ("plaid_balance", id(account, "account_id")?);
            Some((key, Cow::Borrowed(account.get("balances")?)))
        }))
        .chain(statements.map(|(id, value)| (("statement", id), Cow::Borrowed(value))))
        .collect::<Evidence>();
    evidence.extend(annotation_evidence(annotations)?);
    Ok(evidence)
}

fn annotation_evidence(annotations: &BTreeMap<String, Annotation>) -> Result<Evidence<'_>, String> {
    annotations
        .iter()
        .map(|(id, annotation)| {
            let value = serde_json::to_value(annotation).map_err(json_error)?;
            Ok((("annotation", id.as_str()), Cow::Owned(value)))
        })
        .collect()
}

/// The saved state's revision; 0 before the first.
fn revision(connection: &Connection) -> Result<u64, String> {
    connection
        .query_row("SELECT revision FROM state_meta WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()
        .map(Option::unwrap_or_default)
        .map_err(db_error)
}

/// Appends what changed from `before` to `after`. A record new to the log is `added`, or
/// `baseline` when it was saved before the log began.
fn append(
    connection: &Connection,
    revision: u64,
    before: &Evidence,
    after: &Evidence,
    new_kind: &str,
) -> Result<(), String> {
    let recorded_at = chrono::Utc::now().to_rfc3339();
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO ledger_events(recorded_at, revision, source, kind, entity_id, payload_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .map_err(db_error)?;
    for (key @ (source, id), value) in after {
        let kind = match before.get(key) {
            None => new_kind,
            Some(previous) if previous != value => "modified",
            Some(_) => continue,
        };
        let payload = serde_json::to_string(value).map_err(json_error)?;
        insert
            .execute(params![recorded_at, revision, source, kind, id, payload])
            .map_err(db_error)?;
    }
    for (source, id) in before.keys().filter(|key| !after.contains_key(*key)) {
        insert
            .execute(params![
                recorded_at,
                revision,
                source,
                "removed",
                id,
                None::<String>
            ])
            .map_err(db_error)?;
    }
    Ok(())
}

/// Logs the change from the stored state to `next`, in the transaction that writes it.
pub(super) fn record_state(connection: &Connection, next: &FinanceState) -> Result<(), String> {
    let stored = Stored::read(connection)?;
    let new_kind = if stored.is_some() {
        "added"
    } else {
        "baseline"
    };
    let stored = stored.unwrap_or_default();
    let statements = next
        .statements
        .iter()
        .map(|file| Ok((file.id.as_str(), statement_file(file)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let after = evidence(
        &next.plaid_cache,
        &next.annotations,
        statements.iter().map(|(id, value)| (*id, value)),
    )?;
    append(
        connection,
        next.revision,
        &stored.evidence()?,
        &after,
        new_kind,
    )
}

/// Logs the change from the stored annotations to `next`, in the transaction that writes them.
pub(super) fn record_annotations(
    connection: &Connection,
    next: &BTreeMap<String, Annotation>,
) -> Result<(), String> {
    let stored = read_annotations(connection)?;
    append(
        connection,
        revision(connection)?,
        &annotation_evidence(&stored)?,
        &annotation_evidence(next)?,
        "added",
    )
}

/// Records what a database already held when it gained the log.
pub(super) fn baseline(connection: &Connection) -> Result<(), String> {
    let Some(stored) = Stored::read(connection)? else {
        return Ok(());
    };
    append(
        connection,
        revision(connection)?,
        &Evidence::new(),
        &stored.evidence()?,
        "baseline",
    )
}

/// Erasure drops the log with everything else and starts an empty one.
pub(super) fn clear(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(&format!("DROP TABLE IF EXISTS ledger_events; {SCHEMA}"))
        .map_err(db_error)
}

#[cfg(test)]
mod tests;
