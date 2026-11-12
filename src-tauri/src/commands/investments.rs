//! Holdings news, the portfolio's day and sectors, earnings, and the investment summary with
//! the market sessions it saves.
use tauri::State;

use crate::{
    providers::{self, news::PortfolioNews, portfolio},
    session::Period,
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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<PortfolioNews, String> {
    let view = state.storage()?.read_view()?;
    let news = state
        .providers
        .refresh_portfolio_news(&view.snapshot)
        .await?;
    // What was just saved is read in the background; the summary never waits on a model.
    if news.refreshed {
        crate::reading_job::nudge(&app);
    }
    Ok(news)
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
    state: State<'_, AppState>,
) -> Result<(), String> {
    Period::through_latest(&start, chrono::Utc::now())?;
    let mut held = portfolio::held_securities(&state.storage()?.read_view()?.snapshot.holdings);
    held.sort_by(|left, right| {
        right
            .value
            .total_cmp(&left.value)
            .then_with(|| left.ticker.cmp(&right.ticker))
    });
    let tickers = std::iter::once(None)
        .chain(
            held.into_iter()
                .filter_map(|security| providers::one_ticker(Some(security.ticker)).ok())
                .flatten()
                .map(Some),
        )
        .collect();
    summary_jobs::warm(&app, &start, tickers);
    Ok(())
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
