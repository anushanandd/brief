//! Every file Brief keeps in its data folder, what each is for, and each database's tables,
//! for the Storage page. A file Brief doesn't know is listed and flagged, so nothing sits
//! there unaccounted for. Tables open read-only and page through at most `PAGE` rows; values
//! are shown as stored, with long text cut and blobs named by size.
use std::{fs, path::Path};

use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde::Serialize;
use ts_rs::TS;

pub const PAGE: usize = 100;
const MAX_CELL_CHARS: usize = 500;

/// What a known file is for, matched by name.
fn purpose(name: &str) -> Option<&'static str> {
    let base = ["-wal", "-shm", "-journal"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name);
    if base != name {
        return purpose(base).map(|_| "SQLite's working file for the database beside it");
    }
    Some(match name {
        "finance-state.sqlite3" => {
            "Your finance database: the change log, and the tables read from it"
        }
        "finance-state.backup.sqlite3" => {
            "The copy of the finance database saved before the latest change"
        }
        "finance-state.lock" => "Keeps a second Brief from opening the finance database",
        "documents.sqlite3" => "Your documents' originals, encrypted with this Mac's key",
        "news.sqlite3" => "Saved news, filings, and request limits; a disposable cache",
        "places.sqlite3" => "Businesses found near each purchase; a disposable cache",
        "market-prices.sqlite3" => "Price history for charts; a disposable cache",
        "startup-market.sqlite3" => "The market values shown at launch; a disposable cache",
        "embeddings.sqlite3" => "Search's on-device text embeddings; a disposable cache",
        crate::storage::OPENED_STATEMENTS => "Statement PDFs opened for reading",
        _ if name.starts_with("finance-state.retained-") && name.ends_with(".sqlite3") => {
            "A finance database kept when a restore replaced it"
        }
        _ => return None,
    })
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "StoredTable")]
pub struct Table {
    pub name: String,
    pub rows: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "StoredFile")]
pub struct File {
    pub name: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub modified: Option<String>,
    /// What it is for; absent for a file Brief doesn't know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub purpose: Option<String>,
    pub directory: bool,
    /// A database's tables, with their row counts.
    pub tables: Vec<Table>,
}

fn read_only(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "This database can't be opened".to_string())
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn table_names(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .map_err(|error| error.to_string())?;
    let names = statement
        .query_map([], |row| row.get(0))
        .and_then(Iterator::collect)
        .map_err(|error| error.to_string())?;
    Ok(names)
}

fn tables(path: &Path) -> Vec<Table> {
    let Ok(connection) = read_only(path) else {
        return Vec::new();
    };
    table_names(&connection)
        .unwrap_or_default()
        .into_iter()
        .map(|name| Table {
            rows: connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM {}", quoted(&name)),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_or(0, |count| count.max(0) as u64),
            name,
        })
        .collect()
}

/// Every file in Brief's data folder, by name.
pub fn files(directory: &Path) -> Result<Vec<File>, String> {
    let entries = fs::read_dir(directory).map_err(|_| "Brief's data folder can't be read")?;
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = entry.metadata().ok()?;
            let database = metadata.is_file() && name.ends_with(".sqlite3");
            Some(File {
                purpose: purpose(&name).map(str::to_owned),
                bytes: metadata.len(),
                modified: metadata
                    .modified()
                    .ok()
                    .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()),
                directory: metadata.is_dir(),
                tables: if database {
                    tables(&entry.path())
                } else {
                    Vec::new()
                },
                name,
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(files)
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "StoredRows")]
pub struct Rows {
    pub columns: Vec<String>,
    /// Each cell as text; null stays null, and a blob reads as its size.
    pub rows: Vec<Vec<Option<String>>>,
    pub total: u64,
}

fn cell(value: ValueRef<'_>) -> Option<String> {
    let text = match value {
        ValueRef::Null => return None,
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => return Some(format!("{} bytes", bytes.len())),
    };
    Some(if text.chars().count() > MAX_CELL_CHARS {
        format!("{}…", text.chars().take(MAX_CELL_CHARS).collect::<String>())
    } else {
        text
    })
}

/// A page of one table of one of Brief's databases, opened read-only. Only a database listed
/// in the folder and a table it has can be read.
pub fn rows(directory: &Path, file: &str, table: &str, offset: usize) -> Result<Rows, String> {
    let known = files(directory)?
        .into_iter()
        .any(|listed| listed.name == file && listed.tables.iter().any(|t| t.name == table));
    if !known {
        return Err("No such table".into());
    }
    let connection = read_only(&directory.join(file))?;
    let total = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", quoted(table)),
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        .max(0) as u64;
    let mut statement = connection
        .prepare(&format!(
            "SELECT * FROM {} LIMIT {PAGE} OFFSET ?1",
            quoted(table)
        ))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let width = columns.len();
    let rows = statement
        .query_map([offset as i64], |row| {
            (0..width)
                .map(|index| row.get_ref(index).map(cell))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .and_then(Iterator::collect)
        .map_err(|error| error.to_string())?;
    Ok(Rows {
        columns,
        rows,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_file_flags_strangers_and_pages_only_listed_tables() {
        let directory =
            std::env::temp_dir().join(format!("brief-inventory-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let connection = Connection::open(directory.join("places.sqlite3")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE searches (key TEXT, data BLOB);
                 INSERT INTO searches VALUES ('a', x'0102'), (NULL, NULL);",
            )
            .unwrap();
        drop(connection);
        fs::write(directory.join("stray.sqlite3"), b"").unwrap();
        fs::write(directory.join("places.sqlite3-wal"), b"").unwrap();

        let files = files(&directory).unwrap();
        let names = files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            ["places.sqlite3", "places.sqlite3-wal", "stray.sqlite3"]
        );
        assert_eq!(
            files[0].tables,
            [Table {
                name: "searches".into(),
                rows: 2
            }]
        );
        assert!(files[1]
            .purpose
            .as_deref()
            .unwrap()
            .starts_with("SQLite's working file"));
        assert_eq!(files[2].purpose, None);

        let page = rows(&directory, "places.sqlite3", "searches", 0).unwrap();
        assert_eq!(page.columns, ["key", "data"]);
        assert_eq!(
            page.rows,
            [
                vec![Some("a".into()), Some("2 bytes".into())],
                vec![None, None]
            ]
        );
        assert_eq!(page.total, 2);
        assert!(rows(&directory, "places.sqlite3", "missing", 0).is_err());
        assert!(rows(&directory, "../places.sqlite3", "searches", 0).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
