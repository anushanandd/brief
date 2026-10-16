//! The encrypted originals, in their own database beside the finance one. Files are only added
//! or removed, never changed, so they need no copy before each finance commit; what Brief read
//! from them lives in the finance database and is backed up with it.
use std::{collections::BTreeSet, path::Path};

use rusqlite::{params, Connection, OptionalExtension};

use crate::database::secure_file;

pub const FILE_NAME: &str = "documents.sqlite3";

/// The fingerprint of the key that sealed a backup's files, and that key under a passphrase.
pub type BackupKey = (String, Option<Vec<u8>>);

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS document_files (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    key_fingerprint TEXT NOT NULL CHECK (length(key_fingerprint) = 16),
    sealed BLOB NOT NULL
) STRICT";

pub struct DocumentFiles {
    connection: Connection,
}

fn error(error: rusqlite::Error) -> String {
    format!("Local documents database error: {error}")
}

impl DocumentFiles {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(error)?;
        connection.execute(SCHEMA, []).map_err(error)?;
        secure_file(path)?;
        Ok(Self { connection })
    }

    pub fn put(&self, id: &str, key_fingerprint: &str, sealed: &[u8]) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO document_files(id, key_fingerprint, sealed) VALUES (?1, ?2, ?3)",
                params![id, key_fingerprint, sealed],
            )
            .map(drop)
            .map_err(error)
    }

    /// The sealed bytes and the fingerprint of the key that sealed them.
    pub fn get(&self, id: &str) -> Result<Option<(Vec<u8>, String)>, String> {
        self.connection
            .query_row(
                "SELECT sealed, key_fingerprint FROM document_files WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(error)
    }

    pub fn ids(&self) -> Result<BTreeSet<String>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM document_files")
            .map_err(error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error);
        ids
    }

    /// Removes every file no document names, such as one deleted or one a restore left out.
    pub fn keep_only(&self, ids: &BTreeSet<&str>) -> Result<(), String> {
        for id in self.ids()? {
            if !ids.contains(id.as_str()) {
                self.connection
                    .execute("DELETE FROM document_files WHERE id = ?1", [&id])
                    .map_err(error)?;
            }
        }
        Ok(())
    }

    /// Copies every file into the backup at `destination`, beside the finance tables.
    pub fn copy_into(&self, destination: &Path) -> Result<usize, String> {
        let backup = Connection::open(destination).map_err(error)?;
        backup.execute(SCHEMA, []).map_err(error)?;
        let mut statement = self
            .connection
            .prepare("SELECT id, key_fingerprint, sealed FROM document_files")
            .map_err(error)?;
        let mut rows = statement.query([]).map_err(error)?;
        let mut copied = 0;
        while let Some(row) = rows.next().map_err(error)? {
            backup
                .execute(
                    "INSERT OR REPLACE INTO document_files(id, key_fingerprint, sealed) VALUES (?1, ?2, ?3)",
                    params![
                        row.get::<_, String>(0).map_err(error)?,
                        row.get::<_, String>(1).map_err(error)?,
                        row.get::<_, Vec<u8>>(2).map_err(error)?
                    ],
                )
                .map_err(error)?;
            copied += 1;
        }
        Ok(copied)
    }

    /// Names the key that sealed a backup's files, and carries it sealed under a passphrase
    /// when the user gave one.
    pub fn write_backup_key(
        destination: &Path,
        fingerprint: &str,
        wrapped: Option<&[u8]>,
    ) -> Result<(), String> {
        let backup = Connection::open(destination).map_err(error)?;
        backup
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS document_key (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    fingerprint TEXT NOT NULL,
                    wrapped BLOB
                ) STRICT;",
            )
            .map_err(error)?;
        backup
            .execute(
                "INSERT OR REPLACE INTO document_key(id, fingerprint, wrapped) VALUES (1, ?1, ?2)",
                params![fingerprint, wrapped],
            )
            .map(drop)
            .map_err(error)
    }

    /// The fingerprint of the key that sealed a backup's files, and that key under a passphrase.
    pub fn read_backup_key(source: &Path) -> Result<Option<BackupKey>, String> {
        let backup =
            Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(error)?;
        let present = backup
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'document_key')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(error)?;
        if !present {
            return Ok(None);
        }
        backup
            .query_row(
                "SELECT fingerprint, wrapped FROM document_key WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(error)
    }

    /// The files a backup carries, as `(id, key fingerprint, sealed)`.
    pub fn read_backup(source: &Path) -> Result<Vec<(String, String, Vec<u8>)>, String> {
        let backup =
            Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(error)?;
        let present = backup
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'document_files')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(error)?;
        if !present {
            return Ok(Vec::new());
        }
        let mut statement = backup
            .prepare("SELECT id, key_fingerprint, sealed FROM document_files")
            .map_err(error)?;
        let files = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error);
        files
    }
}
