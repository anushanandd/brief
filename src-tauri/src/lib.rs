mod authentication;
mod providers;

use std::{collections::BTreeMap, fs, path::Path, sync::Mutex};

use chrono::{SecondsFormat, Utc};
use providers::{IntegrationStatus, LinkSession, LinkStatus, PlaidCache, ProviderSync, Providers};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use tauri::{Manager, State};

const EMPTY_SNAPSHOT: &str = include_str!("../../src/data/empty.json");

struct AppState {
    connection: Mutex<Connection>,
    providers: Providers,
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

        CREATE TABLE IF NOT EXISTS provider_cache (
          key TEXT PRIMARY KEY,
          payload TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS integration_configuration (
          provider TEXT PRIMARY KEY,
          configured INTEGER NOT NULL DEFAULT 0 CHECK (configured IN (0, 1))
        );

        INSERT OR IGNORE INTO integration_configuration (provider, configured)
        VALUES ('plaid', 0), ('snaptrade', 0);

        DROP TABLE IF EXISTS sync_configuration;
        ",
    )?;

    let existing: Option<i64> = connection
        .query_row("SELECT id FROM finance_snapshots WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?;

    if existing.is_none() {
        let empty: Value = serde_json::from_str(EMPTY_SNAPSHOT)
            .expect("the bundled empty finance snapshot must be valid JSON");
        let updated_at = empty["updatedAt"].as_str().unwrap_or_default();
        connection.execute(
            "INSERT INTO finance_snapshots (id, payload, updated_at) VALUES (1, ?1, ?2)",
            params![EMPTY_SNAPSHOT, updated_at],
        )?;
    } else {
        let payload: String = connection.query_row(
            "SELECT payload FROM finance_snapshots WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        if payload.contains("\"id\": \"fidelity\"") || payload.contains("\"id\":\"fidelity\"") {
            connection.execute(
                "UPDATE finance_snapshots SET payload = ?1, updated_at = '1970-01-01T00:00:00Z' WHERE id = 1",
                params![EMPTY_SNAPSHOT],
            )?;
        }
    }

    let snapshot = read_snapshot(connection).map_err(|_| rusqlite::Error::InvalidQuery)?;
    migrate_integration_configuration(connection, &snapshot)?;

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

fn migrate_integration_configuration(
    connection: &Connection,
    snapshot: &Value,
) -> Result<(), rusqlite::Error> {
    let Some(providers) = snapshot.get("providers").and_then(Value::as_array) else {
        return Ok(());
    };
    for provider in providers {
        let Some(id @ ("plaid" | "snaptrade")) = provider.get("id").and_then(Value::as_str) else {
            continue;
        };
        if provider.get("status").and_then(Value::as_str) == Some("ready") {
            connection.execute(
                "UPDATE integration_configuration SET configured = 1 WHERE provider = ?1",
                params![id],
            )?;
        }
    }
    Ok(())
}

fn read_integration_status(connection: &Connection) -> Result<IntegrationStatus, String> {
    let mut status = IntegrationStatus {
        plaid: false,
        snaptrade: false,
    };
    let mut statement = connection
        .prepare("SELECT provider, configured FROM integration_configuration")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (provider, configured) = row.map_err(|error| error.to_string())?;
        match provider.as_str() {
            "plaid" => status.plaid = configured,
            "snaptrade" => status.snaptrade = configured,
            _ => {}
        }
    }
    Ok(status)
}

fn mark_integration_configured(connection: &Connection, provider: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO integration_configuration (provider, configured) VALUES (?1, 1)
             ON CONFLICT(provider) DO UPDATE SET configured = 1",
            params![provider],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn read_plaid_cache(connection: &Connection) -> Result<PlaidCache, String> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM provider_cache WHERE key = 'plaid'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    payload
        .map(|value| serde_json::from_str(&value).map_err(|error| error.to_string()))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn write_plaid_cache(connection: &Connection, cache: &PlaidCache) -> Result<(), String> {
    let payload = serde_json::to_string(cache).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO provider_cache (key, payload) VALUES ('plaid', ?1)
             ON CONFLICT(key) DO UPDATE SET payload = excluded.payload",
            params![payload],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn write_snapshot_payload(connection: &Connection, snapshot: &Value) -> Result<(), String> {
    let payload = serde_json::to_string(snapshot).map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE finance_snapshots SET payload = ?1 WHERE id = 1",
            params![payload],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Default)]
struct TransactionBrand {
    logo_url: Option<String>,
    website: Option<String>,
    logo_name: Option<String>,
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn transaction_brand(transaction: &Value) -> TransactionBrand {
    let counterparty = transaction
        .get("counterparties")
        .and_then(Value::as_array)
        .and_then(|counterparties| {
            counterparties.iter().find(|counterparty| {
                counterparty.get("type").and_then(Value::as_str) == Some("merchant")
            })
        });
    TransactionBrand {
        logo_url: optional_string(transaction.get("logo_url"))
            .or_else(|| optional_string(counterparty.and_then(|value| value.get("logo_url")))),
        website: optional_string(transaction.get("website"))
            .or_else(|| optional_string(counterparty.and_then(|value| value.get("website")))),
        logo_name: optional_string(transaction.get("merchant_name"))
            .or_else(|| optional_string(counterparty.and_then(|value| value.get("name")))),
    }
}

fn enrich_snapshot_transaction_logos(snapshot: &mut Value, cache: &PlaidCache) -> bool {
    let brands: BTreeMap<String, TransactionBrand> = cache
        .transactions()
        .filter_map(|transaction| {
            optional_string(transaction.get("transaction_id"))
                .map(|id| (id, transaction_brand(transaction)))
        })
        .collect();
    let Some(transactions) = snapshot
        .get_mut("transactions")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for transaction in transactions {
        let Some(id) = optional_string(transaction.get("id")) else {
            continue;
        };
        let Some(brand) = brands.get(&id) else {
            continue;
        };
        let Some(object) = transaction.as_object_mut() else {
            continue;
        };
        for (key, value) in [
            ("logoUrl", brand.logo_url.as_ref()),
            ("website", brand.website.as_ref()),
            ("logoName", brand.logo_name.as_ref()),
        ] {
            if !object.contains_key(key) {
                if let Some(value) = value {
                    object.insert(key.into(), Value::String(value.clone()));
                    changed = true;
                }
            }
        }
    }
    changed
}

fn persist_snapshot(connection: &mut Connection, snapshot: &Value) -> Result<(), String> {
    let updated_at = snapshot["updatedAt"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    let payload = serde_json::to_string(snapshot).map_err(|error| error.to_string())?;
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
            "INSERT INTO sync_runs (provider, status, completed_at) VALUES ('local', 'success', ?1)",
            params![updated_at],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    let mut snapshot = read_snapshot(&connection)?;
    let cache = read_plaid_cache(&connection)?;
    if enrich_snapshot_transaction_logos(&mut snapshot, &cache) {
        write_snapshot_payload(&connection, &snapshot)?;
    }
    Ok(snapshot)
}

#[tauri::command]
fn get_integration_status(state: State<'_, AppState>) -> Result<IntegrationStatus, String> {
    let connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    read_integration_status(&connection)
}

#[tauri::command]
async fn save_integration_credentials(
    provider: String,
    client_id: String,
    secret: Option<String>,
    consumer_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let requires_authentication = {
        let connection = state
            .connection
            .lock()
            .map_err(|_| "the local database lock is unavailable".to_string())?;
        let status = read_integration_status(&connection)?;
        match provider.as_str() {
            "plaid" => status.plaid,
            "snaptrade" => status.snaptrade,
            _ => return Err("Unknown provider".into()),
        }
    };
    if requires_authentication && !state.providers.take_credential_edit_authorization()? {
        return Err("Authenticate before replacing saved credentials".into());
    }

    let save_result = state
        .providers
        .save_credentials(&provider, client_id, secret, consumer_key)
        .await;
    if let Err(error) = save_result {
        // Keep a recent authorization valid while the user corrects a rejected client ID or key.
        if requires_authentication {
            state.providers.authorize_credential_edit()?;
        }
        return Err(error);
    }
    let connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    mark_integration_configured(&connection, &provider)?;
    read_integration_status(&connection)
}

#[tauri::command]
async fn authenticate_sensitive_action(state: State<'_, AppState>) -> Result<(), String> {
    state.providers.lock_credentials()?;
    #[cfg(target_os = "macos")]
    if !state.providers.unlock_credentials()? {
        authentication::authenticate_sensitive_action().await?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        state.providers.unlock_credentials()?;
        authentication::authenticate_sensitive_action().await?;
    }
    state.providers.authorize_credential_edit()
}

#[tauri::command]
async fn begin_provider_link(
    provider: String,
    state: State<'_, AppState>,
) -> Result<LinkSession, String> {
    state.providers.begin_link(&provider).await
}

#[tauri::command]
async fn poll_provider_link(
    provider: String,
    session_id: String,
    state: State<'_, AppState>,
) -> Result<LinkStatus, String> {
    state.providers.poll_link(&provider, &session_id).await
}

#[tauri::command]
async fn refresh_provider_data(state: State<'_, AppState>) -> Result<ProviderSync, String> {
    state.providers.lock_credentials()?;
    state.providers.unlock_credentials()?;

    let (mut plaid_cache, history, history_estimated, previous_benchmark) = {
        let connection = state
            .connection
            .lock()
            .map_err(|_| "the local database lock is unavailable".to_string())?;
        let snapshot = read_snapshot(&connection)?;
        (
            read_plaid_cache(&connection)?,
            snapshot
                .get("netWorthHistory")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
            snapshot
                .get("netWorthHistoryEstimated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            snapshot
                .get("benchmarkHistory")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
        )
    };

    let benchmark_sync = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        state.providers.sync_sp500(),
    );
    let (plaid, snaptrade, benchmark) = tokio::join!(
        state.providers.sync_plaid(&mut plaid_cache),
        state.providers.sync_snaptrade(),
        benchmark_sync,
    );
    let plaid = plaid?;
    let snaptrade = snaptrade?;
    let benchmark = benchmark
        .ok()
        .and_then(Result::ok)
        .filter(|history| history.as_array().is_some_and(|points| !points.is_empty()))
        .unwrap_or(previous_benchmark);

    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| "the local database lock is unavailable".to_string())?;
        write_plaid_cache(&connection, &plaid_cache)?;
    }

    Ok(ProviderSync {
        plaid,
        snaptrade,
        net_worth_history: history,
        net_worth_history_estimated: history_estimated,
        benchmark_history: benchmark,
    })
}

#[tauri::command]
fn save_finance_snapshot(snapshot: Value, state: State<'_, AppState>) -> Result<(), String> {
    let mut connection = state
        .connection
        .lock()
        .map_err(|_| "the local database lock is unavailable".to_string())?;
    persist_snapshot(&mut connection, &snapshot)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let database_path = app.path().app_data_dir()?.join("brief.sqlite3");
            app.manage(AppState {
                connection: Mutex::new(open_database(&database_path).map_err(|error| {
                    format!("failed to initialize local finance data: {error}")
                })?),
                providers: Providers::new()
                    .map_err(|error| format!("failed to initialize providers: {error}"))?,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_finance_snapshot,
            get_integration_status,
            authenticate_sensitive_action,
            save_integration_credentials,
            begin_provider_link,
            poll_provider_link,
            refresh_provider_data,
            save_finance_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brief");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_reads_the_empty_snapshot() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_database(&connection).expect("schema initialization");
        let snapshot = read_snapshot(&connection).expect("seed snapshot");
        assert_eq!(snapshot["currency"], "USD");
        assert!(snapshot["holdings"].as_array().is_some_and(Vec::is_empty));
    }

    #[test]
    fn persists_provider_cache() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_database(&connection).expect("schema initialization");
        let cache = PlaidCache::default();
        write_plaid_cache(&connection, &cache).expect("cache write");
        read_plaid_cache(&connection).expect("cache read");
    }

    #[test]
    fn backfills_transaction_branding_from_the_plaid_cache() {
        let cache: PlaidCache = serde_json::from_value(serde_json::json!({
            "items": {
                "item-1": {
                    "cursor": null,
                    "transactions": [{
                        "transaction_id": "transaction-1",
                        "merchant_name": "Coffee Shop",
                        "logo_url": "https://plaid-merchant-logos.plaid.com/coffee.png",
                        "website": "coffee.example"
                    }]
                }
            }
        }))
        .expect("Plaid cache");
        let mut snapshot = serde_json::json!({
            "transactions": [{ "id": "transaction-1", "merchant": "COFFEE SHOP 123" }]
        });

        assert!(enrich_snapshot_transaction_logos(&mut snapshot, &cache));
        assert_eq!(snapshot["transactions"][0]["logoName"], "Coffee Shop");
        assert_eq!(snapshot["transactions"][0]["website"], "coffee.example");
        assert_eq!(
            snapshot["transactions"][0]["logoUrl"],
            "https://plaid-merchant-logos.plaid.com/coffee.png"
        );
        assert!(!enrich_snapshot_transaction_logos(&mut snapshot, &cache));
    }

    #[test]
    fn reads_integration_status_without_the_keychain() {
        let connection = Connection::open_in_memory().expect("in-memory database");
        initialize_database(&connection).expect("schema initialization");
        let status = read_integration_status(&connection).expect("integration status");
        assert!(!status.plaid);
        assert!(!status.snaptrade);

        mark_integration_configured(&connection, "plaid").expect("mark Plaid configured");
        let status = read_integration_status(&connection).expect("integration status");
        assert!(status.plaid);
        assert!(!status.snaptrade);
    }

    #[test]
    fn credential_edit_authorization_is_single_use() {
        let providers = Providers::new().expect("provider state");

        assert!(!providers
            .take_credential_edit_authorization()
            .expect("authorization state"));
        providers
            .authorize_credential_edit()
            .expect("authorize credential edit");
        assert!(providers
            .take_credential_edit_authorization()
            .expect("authorization state"));
        assert!(!providers
            .take_credential_edit_authorization()
            .expect("authorization state"));
    }
}
