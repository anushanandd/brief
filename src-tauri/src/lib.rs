mod authentication;
mod database;
mod financial_engine;
mod foundation_model;
mod providers;
mod storage;

use std::{collections::BTreeMap, sync::Mutex};

use chrono::{DateTime, Duration, Utc};
use providers::{
    IntegrationStatus, LinkSession, LinkStatus, MarketNewsArticle, PlaidData, ProviderSync,
    Providers,
};
use serde_json::Value;
use storage::{Annotation, Storage};
use tauri::{AppHandle, Manager, State};

#[cfg(target_os = "macos")]
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    Emitter, Runtime,
};

const HISTORY_CACHE_HOURS: i64 = 12;

fn history_cache_is_fresh(refreshed_at: Option<&str>, now: DateTime<Utc>) -> bool {
    refreshed_at
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| now.signed_duration_since(value.with_timezone(&Utc)))
        .is_some_and(|age| age >= Duration::zero() && age < Duration::hours(HISTORY_CACHE_HOURS))
}

struct AppState {
    storage: Mutex<Storage>,
    providers: Providers,
    refresh: tokio::sync::Mutex<()>,
}

fn update_or_warn<T>(
    cached: &mut Option<T>,
    result: Result<T, String>,
    provider: &str,
    warnings: &mut Vec<String>,
) -> bool {
    match result {
        Ok(value) => {
            *cached = Some(value);
            true
        }
        Err(error) => {
            warnings.push(format!("{provider}: {error}"));
            false
        }
    }
}

#[tauri::command]
fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    if storage.upgrade_projection().is_err() {
        let mut snapshot = storage.snapshot()?;
        let warnings = snapshot["syncWarnings"].as_array_mut();
        if let Some(warnings) = warnings {
            warnings.push("Cached calculations need a refresh to update.".into());
        } else {
            snapshot["syncWarnings"] =
                serde_json::json!(["Cached calculations need a refresh to update."]);
        }
        return Ok(snapshot);
    }
    storage.snapshot()
}

#[tauri::command]
fn get_sync_runs(state: State<'_, AppState>) -> Result<Vec<database::SyncRun>, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?
        .sync_runs()
}

#[tauri::command]
async fn recover_finance_state(restore: bool, state: State<'_, AppState>) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .recover(restore)
}

#[tauri::command]
fn transaction_annotations(
    changes: BTreeMap<String, Annotation>,
    importing: bool,
    state: State<'_, AppState>,
) -> Result<BTreeMap<String, Annotation>, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .save_annotations(changes, importing)
}

#[tauri::command]
fn get_integration_status(state: State<'_, AppState>) -> Result<IntegrationStatus, String> {
    state.providers.integration_status()
}

#[tauri::command]
fn get_provider_connections(
    state: State<'_, AppState>,
) -> Result<Vec<providers::ProviderConnection>, String> {
    let cache = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .data
        .plaid_cache
        .clone();
    state.providers.connections(&cache)
}

#[tauri::command]
async fn forget_provider_connection(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _refresh = state.refresh.lock().await;
    state.providers.forget_connection(&item_id)?;
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .cancel_pending_refresh();
    Ok(())
}

#[tauri::command]
async fn save_integration_credentials(
    provider: String,
    client_id: String,
    secret: Option<String>,
    consumer_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh before changing credentials")?;
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
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .cancel_pending_refresh();
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
fn get_foundation_model_status() -> foundation_model::FoundationModelStatus {
    foundation_model::status()
}

#[tauri::command]
async fn generate_foundation_explanation(evidence: String) -> Result<String, String> {
    foundation_model::generate(evidence).await
}

#[tauri::command]
async fn begin_provider_link(
    provider: String,
    item_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<LinkSession, String> {
    state
        .providers
        .begin_link(&provider, item_id.as_deref())
        .await
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
fn cancel_provider_link(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.providers.finish_link(&session_id)
}

#[tauri::command]
async fn get_market_snapshots(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let market = state.providers.market_snapshots(symbols).await?;
    let snapshot = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .snapshot()?;
    let projection = financial_engine::apply_market_snapshots(&snapshot, &market.snapshots)?;
    let mut response = serde_json::to_value(market).map_err(|error| error.to_string())?;
    response["financeSnapshot"] = projection;
    Ok(response)
}

#[tauri::command]
async fn get_market_news(
    symbol: String,
    state: State<'_, AppState>,
) -> Result<Vec<MarketNewsArticle>, String> {
    state.providers.market_news(symbol).await
}

#[tauri::command]
async fn refresh_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "A refresh is already running")?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    let sync =
        match tokio::time::timeout(std::time::Duration::from_secs(120), prepare_refresh(&state))
            .await
        {
            Ok(Ok(sync)) => sync,
            result => {
                let error_code = if result.is_err() {
                    "refresh_timeout"
                } else {
                    "provider_refresh_failed"
                };
                if let Ok(mut storage) = state.storage.lock() {
                    storage.record_failed_sync(&run_id, &started_at, error_code);
                }
                return match result {
                    Err(_) => Err("Account refresh timed out; your saved data is unchanged".into()),
                    Ok(Err(error)) => Err(error),
                    Ok(Ok(_)) => unreachable!(),
                };
            }
        };
    let account_links = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .data
        .account_links
        .clone();
    let snapshot = match financial_engine::project(&sync, &account_links, Utc::now()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            if let Ok(mut storage) = state.storage.lock() {
                storage.fail_pending(&sync.sync_id, "projection_failed");
            }
            return Err(error);
        }
    };
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?
        .commit(&sync.sync_id, snapshot)
}

#[tauri::command]
fn save_account_link(
    plaid_account_id: String,
    snaptrade_account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .save_account_link(plaid_account_id, snaptrade_account_id)
}

async fn prepare_refresh(state: &AppState) -> Result<ProviderSync, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .ensure_writable()?;
    state.providers.unlock_credentials()?;

    let (
        plaid_cache,
        mut provider_data,
        previous_benchmark,
        had_plaid,
        had_snaptrade,
        revision,
        previous_snapshot,
        market_cache_compatible,
    ) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())?;
        let snapshot = &storage.data.snapshot;
        let benchmark_compatible = snapshot
            .pointer("/provenance/benchmark/provider")
            .and_then(Value::as_str)
            == Some("Alpaca")
            && snapshot
                .pointer("/provenance/benchmark/adjustment")
                .and_then(Value::as_str)
                == Some("all");
        let security_history_compatible = storage
            .data
            .provider_data
            .security_history_adjustment
            .as_deref()
            == Some("split");
        (
            storage.data.plaid_cache.clone(),
            storage.data.provider_data.clone(),
            if benchmark_compatible {
                snapshot
                    .get("benchmarkHistory")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new()))
            } else {
                Value::Array(Vec::new())
            },
            snapshot
                .get("accounts")
                .and_then(Value::as_array)
                .is_some_and(|accounts| {
                    accounts.iter().any(|account| {
                        account
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| id.starts_with("plaid:"))
                    })
                }),
            snapshot
                .get("accounts")
                .and_then(Value::as_array)
                .is_some_and(|accounts| {
                    accounts.iter().any(|account| {
                        account
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| id.starts_with("snaptrade:"))
                    })
                }),
            storage.data.revision,
            snapshot.clone(),
            benchmark_compatible && security_history_compatible,
        )
    };

    let now = Utc::now();
    let refresh_snaptrade_history =
        !history_cache_is_fresh(provider_data.snaptrade_history_refreshed_at.as_deref(), now);
    let backfill_snaptrade_activity = !history_cache_is_fresh(
        provider_data.snaptrade_activity_backfilled_at.as_deref(),
        now,
    );
    if provider_data.security_history_adjustment.as_deref() != Some("split") {
        provider_data.security_history.clear();
        provider_data.security_history_adjustment = None;
        provider_data.market_history_refreshed_at = None;
    }
    let refresh_market_history = !market_cache_compatible
        || !history_cache_is_fresh(provider_data.market_history_refreshed_at.as_deref(), now);
    let mut next_plaid_cache = plaid_cache.clone();
    let benchmark_sync = async {
        if refresh_market_history {
            Some(
                tokio::time::timeout(
                    std::time::Duration::from_secs(12),
                    state.providers.sync_benchmark(),
                )
                .await,
            )
        } else {
            None
        }
    };
    let (plaid, snaptrade, benchmark) = tokio::join!(
        state
            .providers
            .sync_plaid(&mut next_plaid_cache, provider_data.plaid.as_ref()),
        state.providers.sync_snaptrade(
            provider_data.snaptrade.as_ref(),
            refresh_snaptrade_history,
            backfill_snaptrade_activity
        ),
        benchmark_sync,
    );
    let mut warnings = Vec::new();
    for (name, error) in [
        ("plaid", plaid.as_ref().err()),
        ("snaptrade", snaptrade.as_ref().err()),
    ] {
        let status = provider_data.sync_status.entry(name.into()).or_default();
        status.error = error.cloned();
        if error.is_none() {
            status.updated_at = Some(now.to_rfc3339());
        }
    }
    let plaid_updated = update_or_warn(&mut provider_data.plaid, plaid, "Plaid", &mut warnings);
    if !plaid_updated {
        next_plaid_cache = plaid_cache;
    }
    let mut refreshed_account_ids = if plaid_updated {
        provider_data
            .plaid
            .as_ref()
            .map(|data| data.refreshed_account_ids.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let plaid_complete = plaid_updated
        && provider_data
            .plaid
            .as_ref()
            .is_none_or(|data| data.warnings.is_empty());
    if let Some(data) = &provider_data.plaid {
        warnings.extend(data.warnings.clone());
        if !data.warnings.is_empty() {
            provider_data
                .sync_status
                .entry("plaid".into())
                .or_default()
                .error = Some(data.warnings.join(" · "));
        }
    }
    let snaptrade_updated = update_or_warn(
        &mut provider_data.snaptrade,
        snaptrade,
        "SnapTrade",
        &mut warnings,
    );
    if let Some(data) = &provider_data.snaptrade {
        warnings.extend(data.warnings.clone());
        if snaptrade_updated {
            refreshed_account_ids.extend(
                data.accounts
                    .iter()
                    .filter_map(|account| account.get("id").and_then(Value::as_str))
                    .map(|id| format!("snaptrade:{id}")),
            );
        }
    }
    if refresh_snaptrade_history
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.history_complete)
    {
        provider_data.snaptrade_history_refreshed_at = Some(now.to_rfc3339());
    }
    if backfill_snaptrade_activity
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.activity_complete)
    {
        provider_data.snaptrade_activity_backfilled_at = Some(now.to_rfc3339());
    }
    if (!plaid_updated && provider_data.plaid.is_none() && had_plaid)
        || (!snaptrade_updated && provider_data.snaptrade.is_none() && had_snaptrade)
    {
        return Err(format!(
            "Could not refresh safely without a provider cache: {}",
            warnings.join(" · ")
        ));
    }
    let mut benchmark_refreshed = false;
    if let Some(result) = &benchmark {
        match result {
            Ok(Err(error)) => warnings.push(format!("Benchmark history: {error}")),
            Err(_) => warnings.push("Benchmark history: request timed out".into()),
            _ => {}
        }
    }
    let benchmark = benchmark
        .and_then(Result::ok)
        .and_then(Result::ok)
        .filter(|history| history.as_array().is_some_and(|points| !points.is_empty()))
        .inspect(|_| benchmark_refreshed = true)
        .unwrap_or(previous_benchmark);
    let mut security_history_refreshed = false;
    if refresh_market_history {
        let empty_plaid = PlaidData::default();
        match tokio::time::timeout(
            std::time::Duration::from_secs(12),
            state
                .providers
                .sync_security_history(provider_data.plaid.as_ref().unwrap_or(&empty_plaid)),
        )
        .await
        {
            Ok(Ok(history)) => {
                provider_data.security_history = history;
                provider_data.security_history_adjustment = Some("split".into());
                security_history_refreshed = true;
            }
            Ok(Err(error)) => warnings.push(format!("Market history: {error}")),
            Err(_) => warnings.push("Market history: request timed out".into()),
        }
    }
    if benchmark_refreshed && security_history_refreshed {
        provider_data.market_history_refreshed_at = Some(now.to_rfc3339());
    }

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    let sync_id = storage.stage(
        revision,
        next_plaid_cache,
        provider_data.clone(),
        warnings.clone(),
    )?;

    Ok(ProviderSync {
        sync_id,
        balances_fresh: plaid_complete && snaptrade_updated,
        refreshed_account_ids,
        previous_snapshot,
        plaid: provider_data.plaid.unwrap_or_default(),
        snaptrade: provider_data.snaptrade.unwrap_or_default(),
        benchmark_history: benchmark,
        security_history: provider_data.security_history,
    })
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

    let previous = MenuItem::with_id(app, "graph-previous", "Previous Graph", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "graph-next", "Next Graph", true, None::<&str>)?;
    let previous_window = MenuItem::with_id(
        app,
        "graph-window-previous",
        "Previous Date View",
        true,
        Some("Cmd+Ctrl+ArrowLeft"),
    )?;
    let next_window = MenuItem::with_id(
        app,
        "graph-window-next",
        "Next Date View",
        true,
        Some("Cmd+Ctrl+ArrowRight"),
    )?;
    let week = MenuItem::with_id(app, "graph-week", "1 Week", true, None::<&str>)?;
    let month = MenuItem::with_id(app, "graph-month", "1 Month", true, None::<&str>)?;
    let quarter = MenuItem::with_id(app, "graph-quarter", "3 Months", true, None::<&str>)?;
    let all = MenuItem::with_id(app, "graph-all", "All Time", true, None::<&str>)?;
    let graph = Submenu::with_items(
        app,
        "Graph",
        true,
        &[
            &previous,
            &next,
            &previous_window,
            &next_window,
            &week,
            &month,
            &quarter,
            &all,
        ],
    )?;
    menu.append(&graph)?;

    Ok(menu)
}

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
                refresh: tokio::sync::Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_finance_snapshot,
            get_sync_runs,
            recover_finance_state,
            transaction_annotations,
            get_integration_status,
            get_provider_connections,
            forget_provider_connection,
            get_foundation_model_status,
            generate_foundation_explanation,
            authenticate_sensitive_action,
            save_integration_credentials,
            begin_provider_link,
            poll_provider_link,
            cancel_provider_link,
            get_market_snapshots,
            get_market_news,
            refresh_finance_snapshot,
            save_account_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brief");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_edit_authorization_is_single_use() {
        let providers = Providers::new().expect("provider state");
        assert!(!providers.take_credential_edit_authorization().unwrap());
        providers.authorize_credential_edit().unwrap();
        assert!(providers.take_credential_edit_authorization().unwrap());
        assert!(!providers.take_credential_edit_authorization().unwrap());
    }

    #[test]
    fn provider_failures_keep_last_good_data_and_add_a_warning() {
        let mut cached = Some(vec!["saved"]);
        let mut warnings = Vec::new();

        assert!(!update_or_warn(
            &mut cached,
            Err("offline".into()),
            "Plaid",
            &mut warnings,
        ));
        assert_eq!(cached, Some(vec!["saved"]));
        assert_eq!(warnings, vec!["Plaid: offline"]);

        assert!(update_or_warn(
            &mut cached,
            Ok(vec!["fresh"]),
            "Plaid",
            &mut warnings,
        ));
        assert_eq!(cached, Some(vec!["fresh"]));
    }

    #[test]
    fn historical_data_expires_after_twelve_hours() {
        let now = "2026-09-04T16:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(history_cache_is_fresh(Some("2026-09-04T05:00:01Z"), now));
        assert!(!history_cache_is_fresh(Some("2026-09-04T04:00:00Z"), now));
        assert!(!history_cache_is_fresh(Some("invalid"), now));
    }
}
