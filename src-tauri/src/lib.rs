mod authentication;
mod providers;

use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};

use providers::{
    IntegrationStatus, LinkSession, LinkStatus, MarketSnapshot, PlaidCache, ProviderSync, Providers,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tauri::{Manager, State};

#[cfg(target_os = "macos")]
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    AppHandle, Emitter, Runtime,
};

const EMPTY_SNAPSHOT: &str = include_str!("../../src/data/empty.json");

struct Storage {
    snapshot: PathBuf,
    plaid_cache: PathBuf,
}

impl Storage {
    fn new(data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        let storage = Self {
            snapshot: data_dir.join("finance-snapshot.json"),
            plaid_cache: data_dir.join("plaid-cache.json"),
        };
        if !storage.snapshot.exists() {
            let empty =
                serde_json::from_str::<Value>(EMPTY_SNAPSHOT).map_err(|error| error.to_string())?;
            storage.write(&storage.snapshot, &empty)?;
        }
        Ok(storage)
    }

    fn read<T: DeserializeOwned>(&self, path: &PathBuf) -> Result<T, String> {
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
    }

    fn read_or_default<T: Default + DeserializeOwned>(&self, path: &PathBuf) -> Result<T, String> {
        if path.exists() {
            self.read(path)
        } else {
            Ok(T::default())
        }
    }

    fn write<T: Serialize>(&self, path: &PathBuf, value: &T) -> Result<(), String> {
        let temporary = path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec(value).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::rename(temporary, path).map_err(|error| error.to_string())
    }
}

struct AppState {
    storage: Mutex<Storage>,
    providers: Providers,
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

#[tauri::command]
fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    let mut snapshot = storage.read(&storage.snapshot)?;
    let cache = storage.read_or_default(&storage.plaid_cache)?;
    if enrich_snapshot_transaction_logos(&mut snapshot, &cache) {
        storage.write(&storage.snapshot, &snapshot)?;
    }
    Ok(snapshot)
}

#[tauri::command]
fn get_integration_status(state: State<'_, AppState>) -> Result<IntegrationStatus, String> {
    state.providers.integration_status()
}

#[tauri::command]
async fn save_integration_credentials(
    provider: String,
    client_id: String,
    secret: Option<String>,
    consumer_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let status = state.providers.integration_status()?;
    let requires_authentication = match provider.as_str() {
        "plaid" => status.plaid,
        "snaptrade" => status.snaptrade,
        "alpaca" => status.alpaca,
        _ => return Err("Unknown provider".into()),
    };
    if requires_authentication && !state.providers.take_credential_edit_authorization()? {
        return Err("Authenticate before replacing saved credentials".into());
    }

    if let Err(error) = state
        .providers
        .save_credentials(&provider, client_id, secret, consumer_key)
        .await
    {
        if requires_authentication {
            state.providers.authorize_credential_edit()?;
        }
        return Err(error);
    }
    state.providers.integration_status()
}

#[tauri::command]
async fn authenticate_sensitive_action(state: State<'_, AppState>) -> Result<(), String> {
    state.providers.lock_credentials()?;
    authentication::authenticate_sensitive_action().await?;
    state.providers.unlock_credentials()?;
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
async fn get_market_snapshots(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BTreeMap<String, MarketSnapshot>, String> {
    state.providers.market_snapshots(symbols).await
}

#[tauri::command]
async fn refresh_provider_data(state: State<'_, AppState>) -> Result<ProviderSync, String> {
    state.providers.lock_credentials()?;
    state.providers.unlock_credentials()?;

    let (mut plaid_cache, history, history_estimated, previous_benchmark) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())?;
        let snapshot: Value = storage.read(&storage.snapshot)?;
        (
            storage.read_or_default(&storage.plaid_cache)?,
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

    let storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    storage.write(&storage.plaid_cache, &plaid_cache)?;

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
    let storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    storage.write(&storage.snapshot, &snapshot)
}

#[cfg(target_os = "macos")]
fn shortcut_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    let settings = MenuItem::with_id(app, "open-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    if let Some(app_menu) = menu.items()?.first().and_then(|item| item.as_submenu()) {
        app_menu.insert(&settings, 1)?;
    }

    for item in menu.items()? {
        let Some(submenu) = item.as_submenu() else {
            continue;
        };
        let title = submenu.text()?;

        if title == "File" {
            menu.remove(&item)?;
            continue;
        }

        if title == "Edit" || title == "Window" {
            for child in submenu.items()? {
                let Some(predefined) = child.as_predefined_menuitem() else {
                    continue;
                };
                let label = predefined.text()?.replace('&', "");
                if (title == "Edit" && label == "Select All")
                    || (title == "Window" && label != "Zoom")
                {
                    submenu.remove(&child)?;
                }
            }
        }
    }

    let previous = MenuItem::with_id(
        app,
        "graph-previous",
        "Previous Graph",
        true,
        Some("CmdOrCtrl+ArrowLeft"),
    )?;
    let next = MenuItem::with_id(
        app,
        "graph-next",
        "Next Graph",
        true,
        Some("CmdOrCtrl+ArrowRight"),
    )?;
    let week = MenuItem::with_id(app, "graph-week", "1 Week", true, Some("CmdOrCtrl+W"))?;
    let month = MenuItem::with_id(app, "graph-month", "1 Month", true, Some("CmdOrCtrl+M"))?;
    let three_months = MenuItem::with_id(
        app,
        "graph-three-months",
        "3 Months",
        true,
        Some("CmdOrCtrl+3"),
    )?;
    let all = MenuItem::with_id(app, "graph-all", "All Time", true, Some("CmdOrCtrl+A"))?;
    let graph = Submenu::with_items(
        app,
        "Graph",
        true,
        &[&previous, &next, &week, &month, &three_months, &all],
    )?;
    menu.append(&graph)?;

    Ok(menu)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(target_os = "macos")]
    let builder = builder.menu(shortcut_menu).on_menu_event(|app, event| {
        let shortcut = event.id().as_ref();
        if shortcut == "open-settings" {
            let _ = app.emit("open-settings", ());
        } else if shortcut.starts_with("graph-") {
            let _ = app.emit("graph-shortcut", shortcut.to_string());
        }
    });

    builder
        .setup(|app| {
            app.manage(AppState {
                storage: Mutex::new(Storage::new(app.path().app_data_dir()?)?),
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
            get_market_snapshots,
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
    fn persists_snapshot_and_provider_cache() {
        let directory = std::env::temp_dir().join(format!("brief-test-{}", uuid::Uuid::new_v4()));
        let storage = Storage::new(directory.clone()).expect("storage");
        let snapshot: Value = storage.read(&storage.snapshot).expect("snapshot");
        assert!(snapshot["holdings"].as_array().is_some_and(Vec::is_empty));
        storage
            .write(&storage.plaid_cache, &PlaidCache::default())
            .expect("cache write");
        storage
            .read::<PlaidCache>(&storage.plaid_cache)
            .expect("cache read");
        fs::remove_dir_all(directory).expect("test cleanup");
    }

    #[test]
    fn backfills_transaction_branding_from_the_plaid_cache() {
        let cache: PlaidCache = serde_json::from_value(serde_json::json!({
            "items": { "item-1": { "cursor": null, "transactions": [{
                "transaction_id": "transaction-1",
                "merchant_name": "Coffee Shop",
                "logo_url": "https://plaid-merchant-logos.plaid.com/coffee.png",
                "website": "coffee.example"
            }]}}
        }))
        .expect("Plaid cache");
        let mut snapshot = serde_json::json!({
            "transactions": [{ "id": "transaction-1", "merchant": "COFFEE SHOP 123" }]
        });

        assert!(enrich_snapshot_transaction_logos(&mut snapshot, &cache));
        assert_eq!(snapshot["transactions"][0]["logoName"], "Coffee Shop");
        assert!(!enrich_snapshot_transaction_logos(&mut snapshot, &cache));
    }

    #[test]
    fn credential_edit_authorization_is_single_use() {
        let providers = Providers::new().expect("provider state");
        assert!(!providers.take_credential_edit_authorization().unwrap());
        providers.authorize_credential_edit().unwrap();
        assert!(providers.take_credential_edit_authorization().unwrap());
        assert!(!providers.take_credential_edit_authorization().unwrap());
    }
}
