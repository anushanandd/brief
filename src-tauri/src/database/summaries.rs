//! Saved investment summaries: the latest of each scope (the portfolio, or one holding) and
//! period start, with the build version and the hash of the inputs it was made from
//! (`summary::inputs_key`), so a page shows it at once and a request whose inputs match asks
//! nothing of the network. Results, not edits: never in the change log, kept a few weeks,
//! backed up with the database, and erased with the finances.
use rusqlite::{params, OptionalExtension};

use super::{db_error, Database};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS investment_summaries (
       scope TEXT NOT NULL CHECK (length(scope) BETWEEN 1 AND 12),
       period_start TEXT NOT NULL CHECK (length(period_start) = 10),
       version INTEGER NOT NULL CHECK (version >= 0),
       inputs TEXT NOT NULL CHECK (length(inputs) = 64),
       complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
       saved_at TEXT NOT NULL,
       summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
       PRIMARY KEY (scope, period_start)
     ) STRICT;";

/// Summaries saved longer ago than this are dropped when another is saved: a period start that
/// old is no longer one a page asks for.
const KEPT_DAYS: i64 = 21;

/// One saved summary as the database holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedSummary {
    pub version: u32,
    pub inputs: String,
    /// Whether every part loaded; a partial summary is retried sooner.
    pub complete: bool,
    pub saved_at: chrono::DateTime<chrono::Utc>,
    pub json: String,
}

impl Database {
    /// The summary saved for `scope` and `start`, if any.
    pub fn saved_summary(
        &self,
        scope: &str,
        start: chrono::NaiveDate,
    ) -> Result<Option<SavedSummary>, String> {
        let row = self
            .connection
            .query_row(
                "SELECT version, inputs, complete, saved_at, summary_json FROM investment_summaries
                 WHERE scope = ?1 AND period_start = ?2",
                params![scope, start.to_string()],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        // A row whose time no longer reads is as good as missing: it is made again.
        Ok(row.and_then(|(version, inputs, complete, saved_at, json)| {
            Some(SavedSummary {
                version,
                inputs,
                complete,
                saved_at: chrono::DateTime::parse_from_rfc3339(&saved_at)
                    .ok()?
                    .to_utc(),
                json,
            })
        }))
    }

    /// Saves the latest summary of `scope` and `start` in place of the one before, and drops
    /// summaries no page asks for any more.
    pub fn save_summary(
        &mut self,
        scope: &str,
        start: chrono::NaiveDate,
        saved: &SavedSummary,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT OR REPLACE INTO investment_summaries
                   (scope, period_start, version, inputs, complete, saved_at, summary_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    scope,
                    start.to_string(),
                    saved.version,
                    saved.inputs,
                    saved.complete,
                    saved.saved_at.to_rfc3339(),
                    saved.json,
                ],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM investment_summaries WHERE saved_at < ?1",
                [(saved.saved_at - chrono::Duration::days(KEPT_DAYS)).to_rfc3339()],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    pub(super) fn clear_summaries(transaction: &rusqlite::Transaction<'_>) -> Result<(), String> {
        transaction
            .execute("DELETE FROM investment_summaries", [])
            .map(|_| ())
            .map_err(db_error)
    }
}
