//! Overture's places near each purchase, saved in `places.sqlite3`, a disposable cache. Each
//! search keeps only the candidates for one purchase key (an issuer's street address, or a
//! merchant name in a city), so the file stays small and holds nothing beyond what a match
//! needs. Matching reads it whenever the view is derived.
use std::{collections::BTreeMap, path::Path};

use rusqlite::{params, Connection};

use super::overture::OverturePlace;

/// A search older than this is made again, so closures and new businesses are seen.
const FRESH_DAYS: i64 = 180;

fn connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|_| "Place cache unavailable")?;
    crate::database::secure_file(path)?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| "Place cache unavailable")?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS searches (
                key TEXT PRIMARY KEY, release TEXT NOT NULL, searched_at TEXT NOT NULL
            ) STRICT;
            CREATE TABLE IF NOT EXISTS candidates (
                key TEXT NOT NULL REFERENCES searches(key) ON DELETE CASCADE,
                place TEXT NOT NULL CHECK (json_valid(place))
            ) STRICT;
            CREATE INDEX IF NOT EXISTS candidates_key ON candidates(key);
            PRAGMA foreign_keys = ON;",
        )
        .map_err(|_| "Place cache unavailable")?;
    Ok(connection)
}

/// The keys searched recently enough to stand.
pub fn searched(path: &Path, now: chrono::DateTime<chrono::Utc>) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let cutoff = (now - chrono::Duration::days(FRESH_DAYS)).to_rfc3339();
    let connection = connection(path)?;
    let mut statement = connection
        .prepare("SELECT key FROM searches WHERE searched_at >= ?1")
        .map_err(|_| "Place cache unavailable")?;
    let keys = statement
        .query_map([cutoff], |row| row.get(0))
        .and_then(Iterator::collect)
        .map_err(|_| "Place cache unavailable")?;
    Ok(keys)
}

/// Replaces one search's candidates.
pub fn save(
    path: &Path,
    release: &str,
    key: &str,
    candidates: &[OverturePlace],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let mut connection = connection(path)?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Place cache unavailable")?;
    let write = || -> rusqlite::Result<()> {
        transaction.execute("DELETE FROM candidates WHERE key = ?1", [key])?;
        transaction.execute(
            "INSERT OR REPLACE INTO searches (key, release, searched_at) VALUES (?1, ?2, ?3)",
            params![key, release, now.to_rfc3339()],
        )?;
        for candidate in candidates {
            let place = serde_json::to_string(candidate).unwrap_or_default();
            transaction.execute(
                "INSERT INTO candidates (key, place) VALUES (?1, ?2)",
                params![key, place],
            )?;
        }
        Ok(())
    };
    write().map_err(|_| "Place cache unavailable")?;
    transaction
        .commit()
        .map_err(|_| "Place cache unavailable".into())
}

/// Every saved search with its candidates; nothing when the cache is missing or unreadable,
/// since places are an addition to the view, never a requirement of it.
pub fn read(path: &Path) -> BTreeMap<String, Vec<OverturePlace>> {
    let mut searches = BTreeMap::<String, Vec<OverturePlace>>::new();
    if !path.exists() {
        return searches;
    }
    let Ok(connection) = connection(path) else {
        return searches;
    };
    let mut read = || -> rusqlite::Result<()> {
        let mut statement = connection.prepare("SELECT key FROM searches")?;
        for key in statement.query_map([], |row| row.get::<_, String>(0))? {
            searches.entry(key?).or_default();
        }
        let mut statement = connection.prepare("SELECT key, place FROM candidates")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, place) = row?;
            if let Ok(place) = serde_json::from_str(&place) {
                searches.entry(key).or_default().push(place);
            }
        }
        Ok(())
    };
    if read().is_err() {
        searches.clear();
    }
    searches
}

pub fn clear(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    connection(path)?
        .execute_batch("DELETE FROM candidates; DELETE FROM searches; VACUUM;")
        .map_err(|_| "Place cache unavailable".into())
}
