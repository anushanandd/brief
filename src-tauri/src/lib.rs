use std::{fs, path::Path, sync::Mutex};

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use tauri::{Manager, State};

const SEED_SNAPSHOT: &str = include_str!("../../src/data/seed.json");

struct AppState {
    connection: Mutex<Connection>,
}

fn initialize_database(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS finance_snapshots (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          payload TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_runs (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          provider TEXT NOT NULL,
          status TEXT NOT NULL,
          completed_at TEXT NOT NULL
        );
        ",
    )?;

    let existing: Option<i64> = connection
        .query_row("SELECT id FROM finance_snapshots WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?;

    if existing.is_none() {
        let seed: Value = serde_json::from_str(SEED_SNAPSHOT)
            .expect("the bundled finance seed must be valid JSON");
        let updated_at = seed["updatedAt"].as_str().unwrap_or_default();
        connection.execute(
            "INSERT INTO finance_snapshots (id, payload, updated_at) VALUES (1, ?1, ?2)",
            params![SEED_SNAPSHOT, updated_at],
        )?;
    }

    Ok(())
}

fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    initialize_database(&connection).map_err(|error| error.to_string())?;
    Ok(connection)
}

fn read_snapshot(connection: &Connection) -> Result<Value, String> {
    let payload: String = connection
        .query_row(
            "SELECT payload FROM finance_snapshots WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    serde_json::from_str(&payload).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    read_snapshot(&connection)
}

#[tauri::command]
fn refresh_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let mut connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    let mut snapshot = read_snapshot(&connection)?;
    let updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    snapshot["updatedAt"] = Value::String(updated_at.clone());
    let payload = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE finance_snapshots SET payload = ?1, updated_at = ?2 WHERE id = 1",
            params![payload, updated_at],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO sync_runs (provider, status, completed_at) VALUES ('local-seed', 'success', ?1)",
            params![updated_at],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;

    Ok(snapshot)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database_path = app.path().app_data_dir()?.join("brief.sqlite3");
            let connection = open_database(&database_path)
                .map_err(|error| format!("failed to initialize local finance data: {error}"))?;
            app.manage(AppState {
                connection: Mutex::new(connection),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_finance_snapshot,
            refresh_finance_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brief");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_reads_the_seed_snapshot() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_database(&connection).expect("schema initialization");
        let snapshot = read_snapshot(&connection).expect("seed snapshot");

        assert_eq!(snapshot["currency"], "USD");
        assert!(snapshot["holdings"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }
}
