mod authentication;
mod database;
mod finance_contract;
mod financial_engine;
mod foundation_model;
mod health;
mod providers;
mod startup_market;
mod storage;
mod transaction_policy;
mod workspace;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Duration, Utc};
use providers::{
    IntegrationStatus, LinkSession, LinkStatus, PlaidData, PlaidRefreshProgress, ProviderSync,
    Providers,
};
use serde_json::Value;
use storage::{Annotation, Storage};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(target_os = "macos")]
use objc2_app_kit::{NSWindow, NSWindowButton};
#[cfg(target_os = "macos")]
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    Runtime,
};

const HISTORY_CACHE_HOURS: i64 = 12;
const DEFAULT_MARKET_UPDATE_INTERVAL_MS: u64 = 10_000;

#[cfg(target_os = "macos")]
fn hide_window_buttons(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let native = window.ns_window()? as usize;
    window.run_on_main_thread(move || {
        // SAFETY: Tauri owns this NSWindow for the life of the main app window.
        let native = unsafe { &*(native as *const NSWindow) };
        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = native.standardWindowButton(kind) {
                button.setHidden(true);
            }
        }
    })
}

fn market_update_interval(milliseconds: Option<u64>) -> StdDuration {
    StdDuration::from_millis(
        milliseconds
            .unwrap_or(DEFAULT_MARKET_UPDATE_INTERVAL_MS)
            .clamp(1_000, 60_000),
    )
}

fn history_cache_is_fresh(refreshed_at: Option<&str>, now: DateTime<Utc>) -> bool {
    refreshed_at
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| now.signed_duration_since(value.with_timezone(&Utc)))
        .is_some_and(|age| age >= Duration::zero() && age < Duration::hours(HISTORY_CACHE_HOURS))
}

struct AppState {
    startup_market: startup_market::StartupMarket,
    storage: Mutex<Storage>,
    providers: Providers,
    refresh: tokio::sync::Mutex<()>,
    market_owner: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>,
    holding_owner: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>,
    holding_selection:
        Mutex<Option<tokio::sync::watch::Sender<providers::holding_market::ChartSelection>>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceRefreshTask {
    id: String,
    label: String,
    status: String,
    state: &'static str,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceRefreshProgress {
    title: &'static str,
    detail: &'static str,
    task: Option<FinanceRefreshTask>,
}

fn report_refresh(app: &AppHandle, title: &'static str, detail: &'static str) {
    let _ = app.emit(
        "finance-refresh-progress",
        FinanceRefreshProgress {
            title,
            detail,
            task: None,
        },
    );
}

fn report_refresh_task(
    app: &AppHandle,
    id: impl Into<String>,
    label: impl Into<String>,
    status: impl Into<String>,
    task_state: &'static str,
) {
    let _ = app.emit(
        "finance-refresh-progress",
        FinanceRefreshProgress {
            title: "Refreshing financial data…",
            detail: "Connected sources update independently, so completed checks appear as they finish.",
            task: Some(FinanceRefreshTask {
                id: id.into(),
                label: label.into(),
                status: status.into(),
                state: task_state,
            }),
        },
    );
}

fn timed_status(status: &str, started: Instant) -> String {
    format!("{status} · {:.1}s", started.elapsed().as_secs_f64())
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
fn export_finance_backup(path: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .export_backup(&PathBuf::from(path))
}

#[tauri::command]
fn inspect_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<storage::BackupInfo, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .inspect_backup(&PathBuf::from(path))
}

#[tauri::command]
async fn restore_finance_backup(path: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let workspace = storage.import_backup(&PathBuf::from(path))?;
    Ok(serde_json::json!({
        "snapshot": storage.snapshot()?,
        "workspace": workspace,
    }))
}

#[tauri::command]
fn get_data_health(state: State<'_, AppState>) -> Result<health::HealthReport, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let snapshot = serde_json::from_value(storage.snapshot()?)
        .map_err(|error| format!("Invalid committed snapshot: {error}"))?;
    Ok(health::report(&snapshot, &storage.sync_runs()?, Utc::now()))
}

#[tauri::command]
fn transaction_annotations(
    changes: BTreeMap<String, Annotation>,
    importing: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let annotations = storage.save_annotations(changes, importing)?;
    Ok(serde_json::json!({ "annotations": annotations, "snapshot": storage.snapshot()? }))
}

#[tauri::command]
async fn export_finance_csv(contents: String, filename: String) -> Result<bool, String> {
    if contents.len() > 50 * 1024 * 1024 {
        return Err("Export exceeds 50 MB".into());
    }
    let path =
        tauri::async_runtime::spawn_blocking(move || workspace::choose_csv_destination(&filename))
            .await
            .map_err(|_| "File picker interrupted")??;
    let Some(path) = path else { return Ok(false) };
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&path)
        .map_err(|_| "Choose a new filename; existing files are never overwritten")?;
    if file
        .write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = std::fs::remove_file(path);
        return Err("Export could not be saved".into());
    }
    Ok(true)
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
async fn get_plaid_recurring_report(
    state: State<'_, AppState>,
) -> Result<providers::PlaidRecurringReport, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh to finish")?;
    state.providers.plaid_recurring_report().await
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
        "alphavantage" => status.alpha_vantage,
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
async fn generate_foundation_explanation(
    evidence: String,
    request_id: String,
    started: tauri::ipc::Channel<()>,
) -> Result<String, String> {
    foundation_model::generate(evidence, request_id, started).await
}

#[tauri::command]
fn cancel_foundation_explanation(request_id: String) {
    foundation_model::cancel(request_id);
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
    browser_completed: Option<bool>,
    state: State<'_, AppState>,
) -> Result<LinkStatus, String> {
    state
        .providers
        .poll_link(&provider, &session_id, browser_completed.unwrap_or(false))
        .await
}

#[tauri::command]
fn cancel_provider_link(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.providers.finish_link(&session_id)
}

#[tauri::command]
fn reveal_main_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Window unavailable")?;
    window.show().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_saved_market(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "Local store unavailable")?;
    if storage.ensure_writable().is_err() {
        return Ok(None);
    }
    let Some(market) = state.startup_market.load(&storage.data.snapshot) else {
        return Ok(None);
    };
    let mut response = serde_json::to_value(&market).map_err(|error| error.to_string())?;
    response["projection"] =
        financial_engine::apply_market_snapshots(&storage.data.snapshot, &market.snapshots)?;
    response["chartSeries"] =
        serde_json::to_value(market_chart_series(&storage.data.snapshot, &market)?)
            .map_err(|error| error.to_string())?;
    response["cached"] = true.into();
    Ok(Some(response))
}

fn market_chart_values(projection: &Value, time: i64) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    if projection["netWorthIncomplete"].as_bool() != Some(true) {
        if let Some(value) = projection["netWorth"]
            .as_f64()
            .filter(|value| value.is_finite())
        {
            values.insert(
                "net-worth".into(),
                serde_json::json!({ "time": time, "value": value }),
            );
        }
    }
    let investments_incomplete =
        projection["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|account| {
                matches!(account["type"].as_str(), Some("brokerage" | "retirement"))
                    && account["value"].is_null()
            });
    for account in projection["brokeragePerformance"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(id) = account["accountId"].as_str() else {
            continue;
        };
        if id == "total" && investments_incomplete {
            continue;
        }
        let Some(value) = account["currentValue"]
            .as_f64()
            .filter(|value| value.is_finite())
        else {
            continue;
        };
        values.insert(
            id.into(),
            serde_json::json!({ "time": time, "value": value }),
        );
    }
    values
}

fn market_chart_series(
    snapshot: &Value,
    market: &providers::MarketSnapshots,
) -> Result<BTreeMap<String, Vec<Value>>, String> {
    let close_time = market
        .snapshots
        .values()
        .filter_map(|snapshot| snapshot.previous_close_as_of.as_deref())
        .filter_map(|value| DateTime::parse_from_rfc3339(value).ok())
        .max();
    let mut series = BTreeMap::<String, Vec<Value>>::new();

    if let Some(as_of) = close_time {
        let closes = market
            .snapshots
            .iter()
            .map(|(symbol, current)| {
                let mut previous = current.clone();
                previous.price = current.previous_close;
                previous.daily_change_pct = 0.0;
                previous.weekly_change_pct = current
                    .weekly_reference_price
                    .map(|reference| (previous.price - reference) / reference * 100.0);
                previous.as_of = as_of.to_rfc3339();
                (symbol.clone(), previous)
            })
            .collect::<BTreeMap<_, _>>();
        let projection = financial_engine::apply_historical_market_snapshots(snapshot, &closes)?;
        for (id, point) in market_chart_values(&projection, as_of.timestamp()) {
            series.entry(id).or_default().push(point);
        }
    }

    if let Some(as_of) = market
        .as_of
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .filter(|as_of| close_time.is_none_or(|close| *as_of > close))
    {
        let projection = financial_engine::apply_market_snapshots(snapshot, &market.snapshots)?;
        for (id, point) in market_chart_values(&projection, as_of.timestamp()) {
            series.entry(id).or_default().push(point);
        }
    }
    Ok(series)
}

fn cancel_stream(
    stream: &mut Option<(String, tokio::sync::watch::Sender<bool>)>,
    request_id: Option<&str>,
) {
    if request_id.is_none_or(|id| stream.as_ref().is_some_and(|(active, _)| active == id)) {
        if let Some((_, cancel)) = stream.take() {
            let _ = cancel.send(true);
        }
    }
}

#[tauri::command]
async fn start_market_stream(
    symbols: Vec<String>,
    update_interval_ms: Option<u64>,
    request_id: String,
    updates: tauri::ipc::Channel<Value>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err("Invalid market request".into());
    }
    let symbols = providers::normalize_stock_symbols(symbols);
    let (cancel, mut receiver) = tokio::sync::watch::channel(false);
    {
        let mut stream = state
            .market_owner
            .lock()
            .map_err(|_| "Market stream state unavailable")?;
        cancel_stream(&mut stream, None);
        *stream = Some((request_id, cancel));
        state.providers.market_streams.start(symbols.clone());
    }
    let market = tokio::select! {
        _ = receiver.changed() => return Ok(()),
        result = state.providers.market_snapshots(symbols.clone()) => result?,
    };
    if *receiver.borrow() {
        return Ok(());
    }
    let mut response = serde_json::to_value(&market).map_err(|error| error.to_string())?;
    {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable")?;
        state.startup_market.save(&storage.data.snapshot, &market);
        response["projection"] =
            financial_engine::apply_market_snapshots(&storage.data.snapshot, &market.snapshots)?;
        response["chartSeries"] =
            serde_json::to_value(market_chart_series(&storage.data.snapshot, &market)?)
                .map_err(|error| error.to_string())?;
    }
    updates
        .send(serde_json::json!({"kind": "update", "market": response}))
        .map_err(|error| error.to_string())?;
    if symbols.is_empty() {
        return Ok(());
    }

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut cancellation = receiver.clone();

        let stream = state.providers.stream_market_updates(
            symbols,
            market,
            market_update_interval(update_interval_ms),
            receiver.clone(),
            |tick| {
                if *receiver.borrow() {
                    return Ok(());
                }
                let storage = state
                    .storage
                    .lock()
                    .map_err(|_| "The local data store is unavailable")?;
                let committed = &storage.data.snapshot;
                let projection =
                    financial_engine::apply_market_snapshots(committed, &tick.market.snapshots)?;
                let chart_point = if let (Some(as_of), Some(chart_snapshots)) =
                    (tick.chart_as_of, tick.chart_snapshots)
                {
                    let chart_projection = financial_engine::apply_historical_market_snapshots(
                        committed,
                        &chart_snapshots,
                    )?;
                    DateTime::parse_from_rfc3339(&as_of)
                        .ok()
                        .map(|time| market_chart_values(&chart_projection, time.timestamp()))
                } else {
                    tick.market
                        .as_of
                        .as_deref()
                        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
                        .map(|time| market_chart_values(&projection, time.timestamp()))
                };
                state.startup_market.save(committed, &tick.market);
                drop(storage);
                let mut payload =
                    serde_json::to_value(&tick.market).map_err(|error| error.to_string())?;
                payload["projection"] = projection;
                if let Some(point) = chart_point {
                    payload["chartPoint"] =
                        serde_json::to_value(point).map_err(|error| error.to_string())?;
                }
                updates
                    .send(serde_json::json!({"kind": "update", "market": payload}))
                    .map_err(|error| error.to_string())
            },
        );
        let result = tokio::select! {
            _ = cancellation.changed() => return,
            result = stream => result,
        };
        if let Err(error) = result {
            if !*receiver.borrow() {
                let _ = updates.send(serde_json::json!({"kind": "error", "message": error}));
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_market_stream(request_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut owner = state
        .market_owner
        .lock()
        .map_err(|_| "Market state unavailable")?;
    if owner.as_ref().is_some_and(|(id, _)| id == &request_id) {
        cancel_stream(&mut owner, Some(&request_id));
        state.providers.market_streams.stop();
    }
    Ok(())
}

fn reuse_holding_chart(
    owner: &mut Option<(String, tokio::sync::watch::Sender<bool>)>,
    chart: Option<&tokio::sync::watch::Sender<providers::holding_market::ChartSelection>>,
    selected: &providers::holding_market::ChartSelection,
) -> bool {
    let Some(chart) = chart.filter(|chart| !chart.is_closed()) else {
        return false;
    };
    let Some((id, cancel)) = owner.as_mut() else {
        return false;
    };
    if *id != chart.borrow().request_id || *cancel.borrow() {
        return false;
    }
    chart.send_replace(selected.clone());
    *id = selected.request_id.clone();
    true
}

#[tauri::command]
async fn start_holding_chart(
    symbol: String,
    range: u64,
    request_id: String,
    warm_symbols: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    providers::holding_market::resolution(range)?;
    let symbol = providers::normalize_stock_symbols(vec![symbol])
        .into_iter()
        .next()
        .ok_or("Invalid security symbol")?;
    if request_id.is_empty() || request_id.len() > 100 {
        return Err("Invalid chart request".into());
    }
    let cache = app
        .path()
        .app_data_dir()
        .map_err(|_| "Price cache directory unavailable")?
        .join("market-prices.sqlite3");
    let selected = providers::holding_market::ChartSelection {
        symbol,
        range,
        request_id: request_id.clone(),
        warm_symbols: warm_symbols
            .unwrap_or_default()
            .into_iter()
            .take(160)
            .filter(|s| !providers::normalize_stock_symbols(vec![s.clone()]).is_empty())
            .collect(),
    };
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let selection = {
        let mut owner = state
            .holding_owner
            .lock()
            .map_err(|_| "Market owner unavailable")?;
        let mut chart = state
            .holding_selection
            .lock()
            .map_err(|_| "Chart selection unavailable")?;
        if reuse_holding_chart(&mut owner, chart.as_ref(), &selected) {
            return Ok(());
        }
        cancel_stream(&mut owner, None);
        *owner = Some((request_id, cancel));
        let (sender, selection) = tokio::sync::watch::channel(selected);
        *chart = Some(sender);
        selection
    };
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let publish = |payload: Value| {
            if *receiver.borrow() {
                return;
            }
            let _ = app.emit("holding-chart-update", payload);
        };
        let initial = selection.borrow().clone();
        if let Some(history) =
            providers::holding_market::cached_history(&cache, &initial.symbol, initial.range)
        {
            publish(
                initial
                    .tag(serde_json::json!({"kind":"history", "history":history, "startedAt":0})),
            );
        }
        let mut cancellation = receiver.clone();
        loop {
            if *cancellation.borrow() {
                break;
            }
            let result = tokio::select! {
                _ = cancellation.changed() => break,
                result = state.providers.run_holding_chart(selection.clone(), &cache,
                    receiver.clone(), &publish) => result,
            };
            if result.is_ok() {
                break;
            }
            publish(selection.borrow().tag(serde_json::json!({"kind":"error", "message":
                "Market data unavailable; retrying. Check Alpaca access in Settings. Saved history is retained."})));
            tokio::select! {
                _ = cancellation.changed() => break,
                _ = tokio::time::sleep(StdDuration::from_secs(30)) => {},
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_holding_chart(request_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut chart = state
        .holding_owner
        .lock()
        .map_err(|_| "Security chart state unavailable")?;
    if chart.as_ref().is_some_and(|(id, _)| id == &request_id) {
        if let Some((_, cancel)) = chart.take() {
            let _ = cancel.send(true);
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_market_news(
    symbols: Vec<String>,
    refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<providers::news_cache::NewsResult, String> {
    state
        .providers
        .market_news(symbols, refresh.unwrap_or(false))
        .await
}

#[tauri::command]
async fn get_earnings_calendar(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<providers::EarningsEvent>, String> {
    state.providers.earnings_calendar(symbols).await
}

#[tauri::command]
async fn refresh_finance_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "A refresh is already running")?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    report_refresh(
        &app,
        "Preparing refresh…",
        "Unlocking credentials and reading the last committed snapshot.",
    );
    let sync = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        prepare_refresh(&app, &state, &run_id, &started_at),
    )
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
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let snapshot = match financial_engine::project(&sync, &account_links, Utc::now()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if let Ok(mut storage) = state.storage.lock() {
                    storage.fail_pending(&sync.sync_id, "projection_failed");
                }
                return Err(error);
            }
        };
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable")?;
        let result = storage.commit(&sync.sync_id, snapshot);
        if result.is_err() {
            storage.fail_pending(&sync.sync_id, "commit_failed");
        }
        result
    })
    .await
    .map_err(|_| "Local refresh worker failed".to_string())?
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

async fn prepare_refresh(
    app: &AppHandle,
    state: &AppState,
    run_id: &str,
    started_at: &str,
) -> Result<ProviderSync, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .ensure_writable()?;
    state.providers.unlock_credentials()?;
    let integrations = state.providers.integration_status()?;

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
    if integrations.plaid {
        report_refresh_task(app, "plaid", "Plaid", "Checking", "active");
    }
    if integrations.snaptrade {
        report_refresh_task(app, "snaptrade", "SnapTrade", "Checking", "active");
    }
    let plaid_progress = |progress: PlaidRefreshProgress| {
        report_refresh_task(
            app,
            progress.id,
            progress.label,
            progress.status,
            progress.state,
        );
    };
    let plaid_sync = async {
        let started = Instant::now();
        let result = state
            .providers
            .sync_plaid(
                &mut next_plaid_cache,
                provider_data.plaid.as_ref(),
                &plaid_progress,
            )
            .await;
        if integrations.plaid {
            match &result {
                Ok(data) if data.warnings.is_empty() => {
                    report_refresh_task(
                        app,
                        "plaid",
                        "Plaid",
                        timed_status("Complete", started),
                        "complete",
                    );
                }
                Ok(_) => report_refresh_task(
                    app,
                    "plaid",
                    "Plaid",
                    timed_status("Completed with notices", started),
                    "warning",
                ),
                Err(_) => {
                    report_refresh_task(
                        app,
                        "plaid",
                        "Plaid",
                        timed_status("Could not update", started),
                        "warning",
                    );
                }
            }
        }
        result
    };
    let snaptrade_sync = async {
        let started = Instant::now();
        let result = state
            .providers
            .sync_snaptrade(
                provider_data.snaptrade.as_ref(),
                refresh_snaptrade_history,
                backfill_snaptrade_activity,
            )
            .await;
        if integrations.snaptrade {
            match &result {
                Ok(data) if data.warnings.is_empty() => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Complete", started),
                    "complete",
                ),
                Ok(_) => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Completed with notices", started),
                    "warning",
                ),
                Err(_) => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Could not update", started),
                    "warning",
                ),
            }
        }
        result
    };
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
    let (plaid, snaptrade, benchmark) = tokio::join!(plaid_sync, snaptrade_sync, benchmark_sync);
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
        run_id,
        started_at,
        revision,
        next_plaid_cache,
        provider_data.clone(),
        warnings.clone(),
    )?;

    Ok(ProviderSync {
        sync_id,
        balances_fresh: (!integrations.plaid || plaid_complete)
            && (!integrations.snaptrade || snaptrade_updated),
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
        for child in app_menu.items()? {
            if let Some(predefined) = child.as_predefined_menuitem() {
                let label = predefined.text()?.replace('&', "");
                if label.starts_with("Hide ") && label != "Hide Others" {
                    app_menu.remove(&child)?;
                }
            }
        }
        let hide_values = MenuItem::with_id(
            app,
            "toggle-sensitive-values",
            "Hide or Show Values",
            true,
            Some("CmdOrCtrl+H"),
        )?;
        app_menu.insert(&hide_values, 2)?;
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
    let year = MenuItem::with_id(app, "graph-year", "1 Year", true, None::<&str>)?;
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
            &year,
            &all,
        ],
    )?;
    menu.append(&graph)?;

    Ok(menu)
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(target_os = "macos")]
    let builder = builder.menu(shortcut_menu).on_menu_event(|app, event| {
        let shortcut = event.id().as_ref();
        if shortcut == "open-settings" {
            let _ = app.emit("open-settings", ());
        } else if shortcut == "toggle-sensitive-values" {
            let _ = app.emit("toggle-sensitive-values", ());
        } else if shortcut.starts_with("graph-") {
            let _ = app.emit("graph-shortcut", shortcut.to_string());
        }
    });

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            hide_window_buttons(
                &app.get_webview_window("main")
                    .ok_or("Main window unavailable")?,
            )?;
            app.manage(AppState {
                startup_market: startup_market::StartupMarket::new(
                    app.path().app_data_dir()?.join("startup-market.sqlite3"),
                ),
                storage: Mutex::new(Storage::new(app.path().app_data_dir()?)?),
                providers: Providers::new()
                    .and_then(|providers| {
                        providers.with_news_cache(
                            &app.path()
                                .app_data_dir()
                                .map_err(|_| "App data directory unavailable")?
                                .join("news.sqlite3"),
                        )
                    })
                    .map_err(|error| format!("failed to initialize providers: {error}"))?,
                refresh: tokio::sync::Mutex::new(()),
                market_owner: Mutex::new(None),
                holding_owner: Mutex::new(None),
                holding_selection: Mutex::new(None),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(StdDuration::from_secs(3)).await;
                let _ = reveal_main_window(handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            reveal_main_window,
            get_saved_market,
            get_finance_snapshot,
            get_sync_runs,
            recover_finance_state,
            export_finance_backup,
            inspect_finance_backup,
            restore_finance_backup,
            get_data_health,
            transaction_annotations,
            export_finance_csv,
            get_integration_status,
            get_provider_connections,
            get_plaid_recurring_report,
            forget_provider_connection,
            get_foundation_model_status,
            generate_foundation_explanation,
            cancel_foundation_explanation,
            authenticate_sensitive_action,
            save_integration_credentials,
            begin_provider_link,
            poll_provider_link,
            cancel_provider_link,
            start_market_stream,
            stop_market_stream,
            start_holding_chart,
            stop_holding_chart,
            get_market_news,
            get_earnings_calendar,
            refresh_finance_snapshot,
            save_account_link,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brief");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cleanup_cannot_cancel_a_new_market_request() {
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let mut stream = Some(("new".into(), sender));
        cancel_stream(&mut stream, Some("old"));
        assert!(!*receiver.borrow());
        assert!(stream.is_some());
        cancel_stream(&mut stream, Some("new"));
        assert!(*receiver.borrow());
        assert!(stream.is_none());
    }

    #[test]
    fn changing_chart_selection_retains_socket_owner_and_old_cleanup_cannot_stop_it() {
        use providers::holding_market::ChartSelection;
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        let mut owner = Some(("first".into(), cancel));
        let (chart, selection) = tokio::sync::watch::channel(ChartSelection {
            symbol: "TEST".into(),
            range: 86400,
            request_id: "first".into(),
            warm_symbols: Vec::new(),
        });
        let next = ChartSelection {
            symbol: "OTHER".into(),
            range: 604800,
            request_id: "second".into(),
            warm_symbols: Vec::new(),
        };
        assert!(reuse_holding_chart(&mut owner, Some(&chart), &next));
        assert_eq!(selection.borrow().symbol, "OTHER");
        assert_eq!(selection.borrow().range, 604800);
        cancel_stream(&mut owner, Some("first"));
        assert!(!*cancelled.borrow());
        assert_eq!(owner.as_ref().unwrap().0, "second");
        cancel_stream(&mut owner, Some("second"));
        assert!(*cancelled.borrow());
        assert!(!reuse_holding_chart(&mut owner, Some(&chart), &next));

        let (cancel, _) = tokio::sync::watch::channel(false);
        owner = Some(("portfolio".into(), cancel));
        assert!(!reuse_holding_chart(&mut owner, Some(&chart), &next));
        assert_eq!(owner.unwrap().0, "portfolio");
    }

    #[tokio::test]
    async fn socket_handoff_waits_for_release_and_cancelled_waiters_never_acquire() {
        let socket = tokio::sync::Mutex::new(());
        let active = socket.lock().await;
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let next = providers::market_stream::socket_lease(&socket, receiver.clone());
        tokio::pin!(next);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(next.as_mut(), &mut context).is_pending());
        cancel.send(true).unwrap();
        assert!(next.await.is_none());
        assert!(providers::market_stream::socket_lease(&socket, receiver)
            .await
            .is_none());
        drop(active);
        let (_cancel, receiver) = tokio::sync::watch::channel(false);
        assert!(providers::market_stream::socket_lease(&socket, receiver)
            .await
            .is_some());
    }

    #[test]
    fn market_update_interval_defaults_and_stays_bounded() {
        assert_eq!(market_update_interval(None), StdDuration::from_secs(10));
        assert_eq!(market_update_interval(Some(500)), StdDuration::from_secs(1));
        assert_eq!(
            market_update_interval(Some(30_000)),
            StdDuration::from_secs(30)
        );
        assert_eq!(
            market_update_interval(Some(90_000)),
            StdDuration::from_secs(60)
        );
    }

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
