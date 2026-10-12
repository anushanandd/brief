//! Holdings news, the portfolio's day and sectors, earnings, and the investment summary with
//! the market sessions it saves.
use tauri::State;

use crate::{
    providers::{self, news::PortfolioNews, portfolio},
    session::{Period, SessionClock},
    summary, AppState,
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
pub(crate) async fn get_portfolio_today(
    state: State<'_, AppState>,
) -> Result<portfolio::PortfolioToday, String> {
    let held = held_securities(&state)?;
    state.providers.portfolio_today(held).await
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
    state.providers.earnings_calendar(symbols).await
}

/// Saves the past week's news for every held stock and checks EDGAR for the stalest holdings,
/// at most every ten minutes.
#[tauri::command]
pub(crate) async fn refresh_portfolio_news(
    state: State<'_, AppState>,
) -> Result<PortfolioNews, String> {
    let view = state.storage()?.read_view()?;
    state.providers.refresh_portfolio_news(&view.snapshot).await
}

/// The portfolio's, or one holding's, day and period: the brief and its headlines, moves,
/// activity, and events.
#[tauri::command]
pub(crate) async fn get_investment_summary(
    start: String,
    ticker: Option<String>,
    state: State<'_, AppState>,
) -> Result<summary::InvestmentSummary, String> {
    let now = chrono::Utc::now();
    let period = Period::through_latest(&start, now)?;
    let ticker = providers::one_ticker(ticker)?;
    let (view, kept) = {
        let storage = state.storage()?;
        (storage.read_view()?, storage.summary_kept(period)?)
    };
    let (summary, closed) = state
        .providers
        .investment_summary(&view.snapshot, period, ticker, SessionClock::at(now), kept)
        .await?;
    state.save_market_sessions(&closed);
    Ok(summary)
}

/// How often the layered model's group and own-move thresholds fired over saved sessions.
#[tauri::command]
pub(crate) fn get_calibration(
    state: State<'_, AppState>,
) -> Result<summary::CalibrationSummary, String> {
    state.storage()?.calibration()
}

/// The latest summary runs, stage by stage, for the Logs page.
#[tauri::command]
pub(crate) fn get_summary_runs(state: State<'_, AppState>) -> Vec<summary::SummaryRun> {
    state.providers.summary_runs.list()
}
