use std::{collections::BTreeMap, time::Duration as StdDuration};

use chrono::DateTime;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    finance_contract::{MarketProjection, Snapshot},
    financial_engine, providers, AppState,
};

const DEFAULT_MARKET_UPDATE_INTERVAL_MS: u64 = 10_000;

pub(super) fn market_update_interval(milliseconds: Option<u64>) -> StdDuration {
    StdDuration::from_millis(
        milliseconds
            .unwrap_or(DEFAULT_MARKET_UPDATE_INTERVAL_MS)
            .clamp(1_000, 60_000),
    )
}

#[tauri::command]
pub(crate) fn get_saved_market(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    let storage = state.storage()?;
    if storage.ensure_writable().is_err() {
        return Ok(None);
    }
    let Some(market) = state.startup_market.load(&storage.data.snapshot) else {
        return Ok(None);
    };
    let mut response = market_payload(&storage.data.snapshot, &storage.data.settings, &market)?;
    response["cached"] = true.into();
    Ok(Some(response))
}

fn json(value: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

/// Quotes with the committed snapshot repriced at them and the chart points they make.
fn market_payload(
    snapshot: &Snapshot,
    settings: &crate::settings::FinanceSettings,
    market: &providers::MarketSnapshots,
) -> Result<Value, String> {
    let mut response = json(market)?;
    response["projection"] = json(project(snapshot, settings, &market.snapshots, false)?)?;
    response["chartSeries"] = json(market_chart_series(snapshot, settings, market)?)?;
    Ok(response)
}

/// The committed valuation at these quotes, with the user's closed and manual accounts
/// applied as the view applies them, so live totals agree with saved ones.
fn project(
    snapshot: &Snapshot,
    settings: &crate::settings::FinanceSettings,
    quotes: &BTreeMap<String, providers::MarketSnapshot>,
    historical: bool,
) -> Result<MarketProjection, String> {
    let mut projection = if historical {
        financial_engine::apply_historical_market_snapshots(snapshot, quotes)?
    } else {
        financial_engine::apply_market_snapshots(snapshot, quotes)?
    };
    financial_engine::manual::adjust_projection(&mut projection, snapshot, settings);
    Ok(projection)
}

fn market_chart_values(projection: &MarketProjection, time: i64) -> BTreeMap<String, Value> {
    let point = |value: f64| serde_json::json!({ "time": time, "value": value });
    let mut values = BTreeMap::new();
    if !projection.net_worth_incomplete && projection.net_worth.is_finite() {
        values.insert("net-worth".into(), point(projection.net_worth));
    }
    let investments_incomplete = projection.accounts.iter().any(|account| {
        matches!(account.r#type.as_str(), "brokerage" | "retirement") && account.value.is_none()
    });
    for account in &projection.brokerage_performance {
        if (account.account_id == "total" && investments_incomplete)
            || !account.current_value.is_finite()
        {
            continue;
        }
        values.insert(account.account_id.clone(), point(account.current_value));
    }
    values
}

fn market_chart_series(
    snapshot: &Snapshot,
    settings: &crate::settings::FinanceSettings,
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
        let projection = project(snapshot, settings, &closes, true)?;
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
        let projection = project(snapshot, settings, &market.snapshots, false)?;
        for (id, point) in market_chart_values(&projection, as_of.timestamp()) {
            series.entry(id).or_default().push(point);
        }
    }
    Ok(series)
}

pub(super) fn cancel_stream(
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
pub(crate) async fn start_market_stream(
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
    let response = {
        let storage = state.storage()?;
        state.startup_market.save(&storage.data.snapshot, &market);
        market_payload(&storage.data.snapshot, &storage.data.settings, &market)?
    };
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
                let storage = state.storage()?;
                let committed = &storage.data.snapshot;
                let settings = &storage.data.settings;
                let projection = project(committed, settings, &tick.market.snapshots, false)?;
                let chart_point = if let (Some(as_of), Some(chart_snapshots)) =
                    (tick.chart_as_of, tick.chart_snapshots)
                {
                    let chart_projection = project(committed, settings, &chart_snapshots, true)?;
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
                let mut payload = json(&tick.market)?;
                payload["projection"] = json(projection)?;
                if let Some(point) = chart_point {
                    payload["chartPoint"] = json(point)?;
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
pub(crate) fn stop_market_stream(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
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

pub(super) fn reuse_holding_chart(
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
pub(crate) async fn start_holding_chart(
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
            publish(
                selection
                    .borrow()
                    .tag(serde_json::json!({"kind":"error", "message":
                "Market data unavailable; retrying"})),
            );
            tokio::select! {
                _ = cancellation.changed() => break,
                _ = tokio::time::sleep(StdDuration::from_secs(30)) => {},
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_holding_chart(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
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
}
