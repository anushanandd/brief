//! Holdings news, the portfolio's day and sectors, earnings, and the investment summary with
//! the market sessions it saves.
use tauri::State;

use crate::{
    providers::{self, portfolio},
    summary, summary_jobs, AppState,
};

#[tauri::command]
pub(crate) async fn get_market_news(
    symbols: Vec<String>,
    refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<providers::news_cache::NewsResult, String> {
    state
        .providers
        .market_news(symbols, refresh.unwrap_or(false))
        .await
}

fn held_securities(state: &AppState) -> Result<Vec<portfolio::HeldSecurity>, String> {
    Ok(portfolio::held_securities(
        &state.storage()?.read_view()?.snapshot.holdings,
    ))
}

#[tauri::command]
pub(crate) async fn get_portfolio_sectors(
    state: State<'_, AppState>,
) -> Result<portfolio::SectorExposure, String> {
    let held = held_securities(&state)?;
    state.providers.portfolio_sectors(held).await
}

#[tauri::command]
pub(crate) async fn get_earnings_calendar(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<providers::EarningsEvent>, String> {
    state.providers.saved_earnings_calendar(symbols).await
}

/// The portfolio's, or one holding's, day and period: the brief and its headlines, moves,
/// activity, and events (an `InvestmentSummary`). The summary saved from the same inputs comes
/// back without a network call; one saved from older inputs comes back at once, marked
/// `refreshing`, while the new one is made in the background; only with none saved does the
/// request wait for market data.
#[tauri::command]
pub(crate) async fn get_investment_summary(
    start: String,
    ticker: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let request = summary_jobs::request(&state, &start, ticker).await?;
    let (ticker, period_start, inputs, now) = (
        request.ticker.clone(),
        request.period.start,
        request.inputs.clone(),
        request.now,
    );
    let served = summary::saved::serve(
        &state.storage,
        ticker.as_deref(),
        period_start,
        &inputs,
        now,
        async || request.make(&state).await,
    )
    .await?;
    if matches!(served, summary::saved::Served::Refreshing(_)) {
        summary_jobs::refresh(&app, start, ticker);
    }
    Ok(served.into_value())
}

/// Makes the portfolio's summary from `start`, then each holding's, largest first, in the
/// background and one at a time, so Investments and each holding's page open on a saved one.
#[tauri::command]
pub(crate) fn warm_investment_summaries(
    start: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    summary_jobs::warm(&app, &start)
}

/// How often the layered model's group and own-move thresholds fired over saved sessions.
#[tauri::command(async)]
pub(crate) fn get_calibration(
    state: State<'_, AppState>,
) -> Result<summary::CalibrationSummary, String> {
    state.storage()?.calibration()
}

/// The latest summary runs, stage by stage, for the Logs page.
#[tauri::command]
pub(crate) fn get_summary_runs(state: State<'_, AppState>) -> Vec<summary::SummaryRun> {
    state.summary_runs.list()
}
