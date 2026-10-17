//! Bank statement PDFs from Plaid Statements, kept in the finance database so backups carry
//! them and erasure removes them, and when each connection was last checked for new ones.
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{db_error, Database};

/// The largest statement the database holds, matching the check before saving.
const MAX_BYTES: i64 = 25 * 1024 * 1024;

/// One saved statement, without its contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BankStatement {
    pub id: String,
    pub item_id: String,
    pub account_id: String,
    pub account_name: String,
    pub year: i32,
    pub month: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub date_posted: Option<String>,
    pub fetched_at: String,
    pub bytes: u64,
}

/// When a connection's statements were last listed, and when Brief last asked Plaid to fetch
/// new ones from the bank.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatementCheck {
    pub listed_at: Option<String>,
    pub requested_at: Option<String>,
}

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS bank_statements (
       id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128),
       item_id TEXT NOT NULL CHECK (length(item_id) BETWEEN 1 AND 128),
       account_id TEXT NOT NULL CHECK (account_id LIKE 'plaid:%' AND length(account_id) <= 220),
       account_name TEXT NOT NULL CHECK (length(account_name) <= 120),
       year INTEGER NOT NULL CHECK (year BETWEEN 2010 AND 2100),
       month INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
       date_posted TEXT,
       sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
       fetched_at TEXT NOT NULL,
       contents BLOB NOT NULL CHECK (length(contents) BETWEEN 5 AND 26214400)
     ) STRICT;
     CREATE INDEX IF NOT EXISTS bank_statements_account ON bank_statements(account_id);
     CREATE TABLE IF NOT EXISTS bank_statement_checks (
       item_id TEXT PRIMARY KEY CHECK (length(item_id) BETWEEN 1 AND 128),
       listed_at TEXT,
       requested_at TEXT
     ) STRICT;";

impl Database {
    /// Every saved statement, newest first.
    pub fn bank_statements(&self) -> Result<Vec<BankStatement>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, item_id, account_id, account_name, year, month, date_posted, fetched_at, length(contents)
                 FROM bank_statements ORDER BY year DESC, month DESC, account_name, id",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(BankStatement {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    account_id: row.get(2)?,
                    account_name: row.get(3)?,
                    year: row.get(4)?,
                    month: row.get(5)?,
                    date_posted: row.get(6)?,
                    fetched_at: row.get(7)?,
                    bytes: row.get::<_, i64>(8)?.max(0) as u64,
                })
            })
            .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    /// One statement's PDF.
    pub fn bank_statement_contents(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        self.connection
            .query_row(
                "SELECT contents FROM bank_statements WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)
    }

    /// Saves a statement once; a statement already saved is left as it is.
    pub fn save_bank_statement(
        &mut self,
        statement: &BankStatement,
        sha256: &str,
        contents: &[u8],
    ) -> Result<(), String> {
        if contents.len() as i64 > MAX_BYTES {
            return Err("Statement is too large to save".into());
        }
        self.connection
            .execute(
                "INSERT OR IGNORE INTO bank_statements
                   (id, item_id, account_id, account_name, year, month, date_posted, sha256, fetched_at, contents)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    statement.id,
                    statement.item_id,
                    statement.account_id,
                    statement.account_name,
                    statement.year,
                    statement.month,
                    statement.date_posted,
                    sha256,
                    statement.fetched_at,
                    contents,
                ],
            )
            .map(|_| ())
            .map_err(db_error)
    }

    pub fn statement_check(&self, item_id: &str) -> Result<StatementCheck, String> {
        Ok(self
            .connection
            .query_row(
                "SELECT listed_at, requested_at FROM bank_statement_checks WHERE item_id = ?1",
                [item_id],
                |row| {
                    Ok(StatementCheck {
                        listed_at: row.get(0)?,
                        requested_at: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(db_error)?
            .unwrap_or_default())
    }

    /// Records a check, keeping the earlier time of whichever step didn't run.
    pub fn record_statement_check(
        &mut self,
        item_id: &str,
        check: &StatementCheck,
    ) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO bank_statement_checks(item_id, listed_at, requested_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(item_id) DO UPDATE SET
                   listed_at = COALESCE(excluded.listed_at, listed_at),
                   requested_at = COALESCE(excluded.requested_at, requested_at)",
                params![item_id, check.listed_at, check.requested_at],
            )
            .map(|_| ())
            .map_err(db_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement(id: &str, year: i32, month: u32) -> BankStatement {
        BankStatement {
            id: id.into(),
            item_id: "item-synthetic".into(),
            account_id: "plaid:checking".into(),
            account_name: "Synthetic Checking".into(),
            year,
            month,
            date_posted: Some(format!("{year}-{month:02}-28")),
            fetched_at: "2026-10-12T12:00:00Z".into(),
            bytes: 0,
        }
    }

    #[test]
    fn statements_are_saved_once_listed_newest_first_and_read_back() {
        let directory =
            std::env::temp_dir().join(format!("brief-statements-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut database = Database::open(&directory.join("finance.sqlite3")).unwrap();
        let digest = "a".repeat(64);
        database
            .save_bank_statement(&statement("july", 2026, 7), &digest, b"%PDF-july")
            .unwrap();
        database
            .save_bank_statement(&statement("august", 2026, 8), &digest, b"%PDF-august")
            .unwrap();
        database
            .save_bank_statement(&statement("august", 2026, 8), &digest, b"%PDF-replaced")
            .unwrap();
        let saved = database.bank_statements().unwrap();
        assert_eq!(
            saved
                .iter()
                .map(|statement| statement.id.as_str())
                .collect::<Vec<_>>(),
            vec!["august", "july"]
        );
        assert_eq!(saved[0].bytes, 11);
        assert_eq!(
            database.bank_statement_contents("august").unwrap().unwrap(),
            b"%PDF-august"
        );
        assert!(database
            .bank_statement_contents("missing")
            .unwrap()
            .is_none());

        database
            .record_statement_check(
                "item-synthetic",
                &StatementCheck {
                    listed_at: Some("2026-10-12T12:00:00Z".into()),
                    requested_at: Some("2026-10-01T12:00:00Z".into()),
                },
            )
            .unwrap();
        database
            .record_statement_check(
                "item-synthetic",
                &StatementCheck {
                    listed_at: Some("2026-10-13T12:00:00Z".into()),
                    requested_at: None,
                },
            )
            .unwrap();
        assert_eq!(
            database.statement_check("item-synthetic").unwrap(),
            StatementCheck {
                listed_at: Some("2026-10-13T12:00:00Z".into()),
                requested_at: Some("2026-10-01T12:00:00Z".into()),
            }
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
