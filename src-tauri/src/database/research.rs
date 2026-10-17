//! The research log: every query sent to the web search research uses, written before it runs
//! and completed after, with the pages it returned; and what each rule source said when last
//! read, so a changed page flags its rule. Saved beside the finances so backups and erasure
//! cover it.
use rusqlite::{params, OptionalExtension};

use super::{db_error, json_error, Database};
use crate::research::{Entry, EntryKind, SourceCheck, Status};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS research_log (
       id TEXT PRIMARY KEY,
       kind TEXT NOT NULL CHECK (kind IN ('search', 'recheck')),
       query TEXT NOT NULL,
       subject TEXT,
       started_at TEXT NOT NULL,
       finished_at TEXT,
       status TEXT NOT NULL CHECK (status IN ('running', 'done', 'failed')),
       error TEXT,
       pages_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(pages_json))
     ) STRICT;
     CREATE INDEX IF NOT EXISTS research_log_started ON research_log(started_at DESC);
     CREATE TABLE IF NOT EXISTS research_sources (
       url TEXT PRIMARY KEY,
       hash TEXT NOT NULL CHECK (length(hash) = 64),
       checked_at TEXT NOT NULL,
       changed_at TEXT
     ) STRICT;";

/// The most entries the log keeps; older ones are dropped as new ones arrive.
const KEPT: usize = 500;

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Search => "search",
        EntryKind::Recheck => "recheck",
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Running => "running",
        Status::Done => "done",
        Status::Failed => "failed",
    }
}

impl Database {
    /// Logs a query about to run, so it is visible before anything leaves the Mac.
    pub fn begin_research(&self, entry: &Entry) -> Result<(), String> {
        let transaction = self.connection.unchecked_transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO research_log(id, kind, query, subject, started_at, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
                params![
                    entry.id,
                    kind_name(entry.kind),
                    entry.query,
                    entry.subject,
                    entry.started_at
                ],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM research_log WHERE id NOT IN
                   (SELECT id FROM research_log ORDER BY started_at DESC, id DESC LIMIT ?1)",
                [KEPT as i64],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    /// Completes a logged query with what came back, or why it failed.
    pub fn finish_research(&self, entry: &Entry) -> Result<(), String> {
        self.connection
            .execute(
                "UPDATE research_log SET finished_at = ?2, status = ?3, error = ?4, pages_json = ?5
                 WHERE id = ?1",
                params![
                    entry.id,
                    entry.finished_at,
                    status_name(entry.status),
                    entry.error,
                    serde_json::to_string(&entry.pages).map_err(json_error)?,
                ],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// The log, newest first.
    pub fn research_log(&self) -> Result<Vec<Entry>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, kind, query, subject, started_at, finished_at, status, error, pages_json
                 FROM research_log ORDER BY started_at DESC, id DESC",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(db_error)?;
        rows.map(|row| {
            let (id, kind, query, subject, started_at, finished_at, status, error, pages) =
                row.map_err(db_error)?;
            Ok(Entry {
                id,
                kind: match kind.as_str() {
                    "recheck" => EntryKind::Recheck,
                    _ => EntryKind::Search,
                },
                query,
                subject,
                started_at,
                finished_at,
                status: match status.as_str() {
                    "done" => Status::Done,
                    "failed" => Status::Failed,
                    _ => Status::Running,
                },
                error,
                pages: serde_json::from_str(&pages).map_err(json_error)?,
            })
        })
        .collect()
    }

    /// What a source said when last read.
    pub fn source_check(&self, url: &str) -> Result<Option<SourceCheck>, String> {
        self.connection
            .query_row(
                "SELECT url, hash, checked_at, changed_at FROM research_sources WHERE url = ?1",
                [url],
                |row| {
                    Ok(SourceCheck {
                        url: row.get(0)?,
                        hash: row.get(1)?,
                        checked_at: row.get(2)?,
                        changed_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(db_error)
    }

    pub fn source_checks(&self) -> Result<Vec<SourceCheck>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT url, hash, checked_at, changed_at FROM research_sources ORDER BY url")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(SourceCheck {
                    url: row.get(0)?,
                    hash: row.get(1)?,
                    checked_at: row.get(2)?,
                    changed_at: row.get(3)?,
                })
            })
            .map_err(db_error)?;
        rows.map(|row| row.map_err(db_error)).collect()
    }

    pub fn save_source_check(&self, check: &SourceCheck) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO research_sources(url, hash, checked_at, changed_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(url) DO UPDATE SET hash = excluded.hash,
                   checked_at = excluded.checked_at, changed_at = excluded.changed_at",
                params![check.url, check.hash, check.checked_at, check.changed_at],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// Clears the log and source checks, as erasure does.
    pub(super) fn clear_research(transaction: &rusqlite::Transaction<'_>) -> Result<(), String> {
        transaction
            .execute_batch("DELETE FROM research_log; DELETE FROM research_sources;")
            .map_err(db_error)
    }
}
