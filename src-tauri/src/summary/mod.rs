//! What happened to the portfolio, or one holding: the day in a few lines with the headlines
//! that explain it (`brief`), and over a returns window how it moved against its own normal,
//! what you did, the company events the market reacted to, and the latest reported quarter.
//!
//! Every figure is Rust's: computed from the committed snapshot, saved stories and filings, and
//! prices. Brief never infers a cause; a reason is a publisher's headline, quoted and linked.
//!
//! News volume tracks company size, not importance, so nothing here ranks by how much was
//! written. Moves are measured in each stock's own typical moves, events by how far the stock
//! moved beyond its benchmark around them, and every stock gets one event on the portfolio.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::finance_contract::{Snapshot, Trade};
use crate::insights::reconcile::{self, PeriodReport, Shares};
use crate::numeric::minor_units;
use crate::providers::closes::{self, DailyCloses};
use crate::providers::fundamentals::Fundamentals;
use crate::providers::insiders::InsiderTrade;
use crate::providers::news::{Benchmark, MARKET_FUND, RELEVANT};
pub use crate::providers::portfolio::modeled;
use crate::providers::portfolio::{held_securities, HeldSecurity};
use crate::providers::*;
use crate::session::{self, MarketState, Period, Session, SessionClock};

mod brief;
mod build;
mod contract;
mod eventlog;
mod events;
mod headlines;
mod layers;
mod ledger;
mod moves;
#[cfg(test)]
pub(crate) mod preview;
pub mod saved;
mod sessions;
#[cfg(test)]
mod tests;
mod trace;

pub use brief::Brief;
use build::Gathered;
pub use contract::*;
pub use eventlog::{Event, ListedEvent, Quiet};
use events::coverage;
use ledger::Ledger;
use moves::{holding_today, notable_moves, period_move, portfolio_today_move};
#[cfg(test)]
pub use sessions::FundRate;
pub use sessions::{calibration, CalibrationSummary, SessionRecord, METHOD};
use sessions::{HoldingPeriod, PeriodLayers};
use trace::Trace;
pub use trace::{SummaryRun, SummaryRuns};

/// The newest trades listed by name; the rest are counted.
const LISTED_TRADES: usize = 3;
/// The events listed, most consequential first.
const LISTED_EVENTS: usize = 3;
/// What's ahead looks this far past the latest session.
const AHEAD_DAYS: i64 = 31;
/// Holdings named for an unusual move of their own, whatever their size.
const NOTABLE: usize = 2;
/// A period whose own layer is this many typical periods is unusual for the stock.
const NOTABLE_Z: f64 = 2.0;
/// Closes reach this far before the period's first session, so every session in it is fitted
/// on a year of history.
const HISTORY_DAYS: i64 = 372;
/// And no further back than this from the latest session.
const MAX_HISTORY_DAYS: i64 = 4 * 365;
/// Positions listed for today, by dollar effect.
const TODAY_POSITIONS: usize = 12;
/// A sector this share of the portfolio is named for today.
const SECTOR_WEIGHT_PCT: f64 = 10.0;

/// What the finance database holds for a summary's period: its saved sessions, their events,
/// and the corporate actions committed with the latest refresh.
#[derive(Default)]
pub struct Kept {
    pub sessions: Vec<SessionRecord>,
    pub events: Vec<Event>,
    pub corporate_actions: Vec<CorporateAction>,
}

/// The closed sessions a summary computed and their events, for the caller to save.
#[derive(Default)]
pub struct Closed {
    pub sessions: Vec<SessionRecord>,
    pub events: Vec<Event>,
}

/// One holding's saved stories and filings, and its own layer by session when prices were
/// available.
#[derive(Serialize)]
pub(super) struct Evidence {
    pub ticker: String,
    pub articles: Vec<MarketNewsArticle>,
    pub filings: Vec<sec::SecFiling>,
    /// The own part of each session's move and the holding's typical day then.
    pub own: BTreeMap<NaiveDate, (f64, f64)>,
}

/// The session whose prices could first react to a story or filing published at `at`; a
/// posted date is that day's session, or the next one.
pub(super) fn reacting_session(at: &str) -> Option<NaiveDate> {
    match session::instant(at) {
        Some(instant) => Some(Session::reacting_to(instant).date),
        None => session::date_of(at).map(|day| Session::on_or_after(day).date),
    }
}

/// The New York date a story, filing, or trade is listed under.
pub(super) fn listed_date(at: &str) -> String {
    session::date_of(at).map_or_else(|| at.chars().take(10).collect(), |day| day.to_string())
}

/// The summary, the closed sessions it computed, and the holdings it was built for.
struct Prepared {
    summary: InvestmentSummary,
    closed: Closed,
    held: Vec<HeldSecurity>,
}

/// Each stock's sector fund and sector name, when its SIC code maps to one.
type Sectors = BTreeMap<String, (&'static str, &'static str)>;

/// What the news cache holds for the modeled stocks.
#[derive(Serialize)]
struct Saved {
    evidence: Vec<Evidence>,
    sectors: Sectors,
    /// The market's stories since the period began, as long as they are kept.
    market_news: Vec<MarketNewsArticle>,
    /// The cause read from each "why it moved" story's text, by link.
    causes: BTreeMap<String, reading::Cause>,
    /// Each stock's latest earnings report as saved.
    reports: Vec<(String, earnings::SavedReport)>,
    /// What analysts and the options market expect of each stock.
    expectations: Vec<Expectation>,
}

/// Latest prices for the modeled stocks, their benchmarks, and the rest of the portfolio.
struct Quotes {
    snapshots: BTreeMap<String, MarketSnapshot>,
    /// The priced holdings the latest session is judged on: the holding, or the portfolio's
    /// stocks.
    held: Vec<HeldSecurity>,
}

/// The modeled stocks with the S&P 500 and their sector funds: one price request covers them.
fn benchmarked(stocks: &[String], sectors: &Sectors) -> Vec<String> {
    let mut symbols = stocks.to_vec();
    symbols.push(MARKET_FUND.0.to_owned());
    symbols.extend(sectors.values().map(|(fund, _)| (*fund).to_owned()));
    symbols
}

/// What the portfolio's layers are measured with: every priced holding (funds included) and
/// the group funds.
fn layer_symbols(held: &[HeldSecurity]) -> Vec<String> {
    held.iter()
        .map(|security| security.ticker.clone())
        .chain(
            layers::GROUP_FUNDS
                .iter()
                .map(|(symbol, _)| (*symbol).to_owned()),
        )
        .collect()
}

/// Each symbol's latest session from its daily closes: its close, or the live price during the
/// session, against the close before. A symbol without that session's close keeps its live
/// snapshot only while the session is open; outside regular hours a pre-market or after-hours
/// price never stands in for a close.
fn session_snapshots(
    snapshots: &BTreeMap<String, MarketSnapshot>,
    closes: &DailyCloses,
    day: NaiveDate,
) -> BTreeMap<String, MarketSnapshot> {
    let session = Session { date: day };
    let in_session = |snapshot: &MarketSnapshot| {
        session::instant(&snapshot.as_of)
            .is_some_and(|at| at >= session.open() && at < session.close())
    };
    snapshots
        .iter()
        .filter_map(|(symbol, snapshot)| {
            let closed = closes.get(symbol).and_then(|series| {
                let [.., (before, previous), (last, close)] = series.as_slice() else {
                    return None;
                };
                (*last == day).then_some((*before, *previous, *close))
            });
            let snapshot = match closed {
                Some((before, previous, close)) => MarketSnapshot {
                    price: close,
                    previous_close: previous,
                    previous_close_as_of: Some(Session { date: before }.close().to_rfc3339()),
                    daily_change_pct: (close / previous - 1.0) * 100.0,
                    as_of: if in_session(snapshot) {
                        snapshot.as_of.clone()
                    } else {
                        session.close().to_rfc3339()
                    },
                    ..snapshot.clone()
                },
                None if in_session(snapshot) => snapshot.clone(),
                None => return None,
            };
            Some((symbol.clone(), snapshot))
        })
        .collect()
}

/// Scheduled events in the month after `end`: announced corporate actions for the holdings
/// (confirmed) and market-wide events from the calendar and published rules (scheduled).
fn upcoming(end: NaiveDate, sources: &eventlog::Sources) -> Vec<Upcoming> {
    let horizon = end + Duration::days(AHEAD_DAYS);
    let mut ahead: Vec<Upcoming> = sources
        .corporate_actions
        .iter()
        .filter(|action| {
            sources.held.contains(action.symbol())
                && action.ex_date() > end
                && action.ex_date() <= horizon
        })
        .map(|action| {
            let symbol = action.symbol();
            Upcoming {
                date: action.ex_date(),
                kind: eventlog::action_kind(action).into(),
                ticker: Some(symbol.into()),
                title: match action {
                    CorporateAction::Split { .. } => format!("{symbol} split"),
                    CorporateAction::Dividend { .. } => format!("{symbol} goes ex-dividend"),
                },
                status: eventlog::Status::Confirmed,
                amount: match action {
                    CorporateAction::Split { .. } => None,
                    CorporateAction::Dividend { .. } => {
                        eventlog::action_dollars(action, sources.shares, end)
                    }
                },
                url: None,
            }
        })
        .collect();
    ahead.extend(
        eventlog::scheduled_macro(end, horizon, sources)
            .into_iter()
            .map(|event| Upcoming {
                date: event
                    .at
                    .as_deref()
                    .and_then(session::date_of)
                    .unwrap_or(event.session),
                kind: event.kind.clone(),
                ticker: None,
                title: event
                    .source
                    .as_ref()
                    .map_or_else(String::new, |source| source.title.clone()),
                status: event.status,
                amount: None,
                url: event.source.as_ref().and_then(|source| source.url.clone()),
            }),
    );
    ahead.sort_by(|a, b| a.date.cmp(&b.date).then(a.kind.cmp(&b.kind)));
    ahead
}

/// The holding's total return over the period beside its benchmarks', all from the same daily
/// closes (adjusted for splits and dividends), through the latest session.
fn holding_period_move(
    summary: &mut InvestmentSummary,
    ticker: &str,
    sectors: &Sectors,
    records: &[&SessionRecord],
    closes: &DailyCloses,
    period: Period,
) {
    let sector = sectors.get(ticker).copied();
    let mut start = BTreeMap::new();
    let mut latest = BTreeMap::new();
    for (symbol, series) in closes {
        if let (Some(from), Some((_, to))) =
            (reconcile::close_on(series, period.start), series.last())
        {
            start.insert(symbol.clone(), from);
            latest.insert(symbol.clone(), *to);
        }
    }
    match period_move(
        ticker,
        sector,
        &start,
        &latest,
        sessions::holding_period(records, ticker),
    ) {
        Some(moved) => summary.period_move = Some(moved),
        None => summary.missing("prices"),
    }
}

/// A year of daily closes and the returns they give, for every symbol the summary measures.
#[derive(Default)]
struct Prices {
    closes: DailyCloses,
    returns: BTreeMap<String, layers::Returns>,
    volumes: BTreeMap<String, BTreeMap<NaiveDate, f64>>,
}

/// The summary of `period`: its inputs gathered, then built, then a holding's company
/// detail added.
async fn prepare_summary(
    providers: &Providers,
    snapshot: &Snapshot,
    period: Period,
    ticker: Option<String>,
    clock: SessionClock,
    kept: Kept,
    trace: &mut Trace,
) -> Result<Prepared, String> {
    let ledger = Ledger::from_snapshot(snapshot);
    if let Some(ticker) = &ticker {
        if !ledger
            .held
            .iter()
            .any(|security| &security.ticker == ticker)
        {
            return Err(format!("{ticker} isn’t held"));
        }
    }
    let mut summary = InvestmentSummary::new(period, ticker.clone(), clock);
    summary.activity = ledger.activity(period, ticker.as_deref());
    let stocks = modeled(&ledger.held, ticker.as_deref());
    trace.stage(
        "ledger",
        "ok",
        Some(format!(
            "{} held, {} modeled, {} trades",
            ledger.held.len(),
            stocks.len(),
            summary.activity.buys + summary.activity.sells
        )),
    );
    let Saved {
        evidence,
        sectors,
        market_news,
        causes,
        reports,
        expectations,
    } = saved_evidence(providers, &stocks, period).await?;
    summary.expectations = expectations;
    trace.stage(
        "evidence",
        "ok",
        Some(format!(
            "{} stories, {} filings",
            evidence
                .iter()
                .map(|holding| holding.articles.len())
                .sum::<usize>(),
            evidence
                .iter()
                .map(|holding| holding.filings.len())
                .sum::<usize>()
        )),
    );
    let quotes = quotes(
        providers,
        &stocks,
        &sectors,
        &ledger.held,
        ticker.as_deref(),
    )
    .await;
    trace.stage(
        "quotes",
        if quotes.snapshots.is_empty() {
            "failed"
        } else {
            "ok"
        },
        Some(format!("{} symbols", quotes.snapshots.len())),
    );
    // Every holding is measured against the market and every group fund, a fund holding
    // included, so one model serves the portfolio, each holding, and the period.
    let mut measured = benchmarked(&stocks, &sectors);
    measured.extend(layer_symbols(&quotes.held));
    let prices = match history(providers, measured, &quotes.snapshots, period).await {
        Ok((prices, truncated)) => {
            trace.stage(
                "history",
                if truncated.is_empty() {
                    "ok"
                } else {
                    "partial"
                },
                Some(if truncated.is_empty() {
                    format!("{} symbols", prices.closes.len())
                } else {
                    format!("{} cut short: {}", truncated.len(), truncated.join(", "))
                }),
            );
            if !truncated.is_empty() {
                summary.missing_for("history", truncated);
            }
            prices
        }
        Err(error) => {
            trace.stage("history", "failed", Some(error));
            summary.missing("prices");
            Prices::default()
        }
    };
    let gathered = Gathered {
        summary,
        ledger,
        evidence,
        sectors,
        market_news,
        causes,
        reports,
        quotes,
        prices,
    };
    let mut prepared = build::build(gathered, snapshot, period, &ticker, kept, trace);
    if let Some(ticker) = &ticker {
        company_detail(
            providers,
            &mut prepared.summary,
            ticker,
            &prepared.held,
            period.start,
        )
        .await;
    }
    Ok(prepared)
}

/// Saved stories, filings, and sectors for `stocks`, and the market's stories.
async fn saved_evidence(
    providers: &Providers,
    stocks: &[String],
    period: Period,
) -> Result<Saved, String> {
    let cache = providers.news_cache().await?;
    let mut saved = Saved {
        evidence: Vec::new(),
        sectors: BTreeMap::new(),
        market_news: Vec::new(),
        causes: BTreeMap::new(),
        reports: Vec::new(),
        expectations: Vec::new(),
    };
    for symbol in stocks {
        let company = cache.company(symbol)?.0;
        if let Some(sector) = company
            .as_ref()
            .and_then(|company| company.sic)
            .and_then(sec::sector_for_sic)
            .filter(|(fund, _)| fund != symbol)
        {
            saved.sectors.insert(symbol.clone(), sector);
        }
        saved
            .reports
            .push((symbol.clone(), earnings::saved_report(&cache, symbol)));
        if let Some((estimates, next)) =
            expectations::saved_expectation(&cache, symbol, period.through)
        {
            saved.expectations.push(Expectation {
                ticker: symbol.clone(),
                estimates,
                next_report: next.as_ref().map(|(day, _)| day.clone()),
                expected_move_pct: next.map(|(_, pct)| pct),
            });
        }
        let articles = cache.articles(symbol)?;
        for article in articles
            .iter()
            .filter(|article| reading::explainer(&article.headline))
        {
            if let Some(cause) = reading::saved_cause(&cache, &article.url) {
                saved.causes.insert(article.url.clone(), cause);
            }
        }
        saved.evidence.push(Evidence {
            ticker: symbol.clone(),
            articles,
            filings: company.map(|company| company.filings).unwrap_or_default(),
            own: BTreeMap::new(),
        });
    }
    saved.market_news = cache
        .market_news(&Session { date: period.start }.close().to_rfc3339())
        .unwrap_or_default();
    Ok(saved)
}

/// Latest prices, so today counts in the model as it does in the headline change. The
/// portfolio's other priced holdings join the modeled stocks and benchmarks for today's move.
async fn quotes(
    providers: &Providers,
    stocks: &[String],
    sectors: &Sectors,
    held: &[HeldSecurity],
    ticker: Option<&str>,
) -> Quotes {
    let held: Vec<HeldSecurity> = held
        .iter()
        .filter(|security| {
            ticker.is_none_or(|ticker| security.ticker == ticker)
                && !matches!(
                    security.kind.as_deref().map(str::to_lowercase).as_deref(),
                    Some("crypto" | "cryptocurrency")
                )
                && alpaca::stock_symbol(&security.ticker).is_ok()
        })
        .cloned()
        .collect();
    let mut quoted = benchmarked(stocks, sectors);
    quoted.extend(held.iter().map(|security| security.ticker.clone()));
    if ticker.is_none() {
        quoted.extend(layer_symbols(&held));
    }
    let snapshots = providers
        .market_snapshots(normalize_stock_symbols(quoted))
        .await
        .map(|snapshots| snapshots.snapshots)
        .unwrap_or_default();
    Quotes { snapshots, held }
}

/// Daily closes for every symbol measured, from a year before the period's first session
/// (no further back than `MAX_HISTORY_DAYS`), with the live price standing in for the
/// latest session's close while it trades. Also returns the symbols the request cut short.
async fn history(
    providers: &Providers,
    symbols: Vec<String>,
    snapshots: &BTreeMap<String, MarketSnapshot>,
    period: Period,
) -> Result<(Prices, Vec<String>), String> {
    let Period { start, end, .. } = period;
    let from = (start - Duration::days(HISTORY_DAYS))
        .min(end - Duration::days(HISTORY_DAYS))
        .max(end - Duration::days(MAX_HISTORY_DAYS));
    let closes::Closes {
        series: mut closes,
        volumes,
        truncated,
    } = providers
        .daily_closes(normalize_stock_symbols(symbols), from, end)
        .await?;
    for (symbol, series) in &mut closes {
        if let Some(snapshot) = snapshots.get(symbol) {
            closes::overlay_latest(series, snapshot.price, &snapshot.as_of);
        }
    }
    let returns = closes
        .iter()
        .map(|(symbol, series)| (symbol.clone(), closes::returns(series)))
        .collect();
    Ok((
        Prices {
            closes,
            returns,
            volumes,
        },
        truncated,
    ))
}

/// A holding's insider trades and latest reported quarter. EDGAR not configured or
/// unreachable leaves insiders out; funds and other non-company holdings have no quarters.
async fn company_detail(
    providers: &Providers,
    summary: &mut InvestmentSummary,
    ticker: &str,
    held: &[HeldSecurity],
    start: NaiveDate,
) {
    summary.insiders = providers
        .insider_trades(ticker, start)
        .await
        .unwrap_or_default();
    let fund = held.iter().any(|security| {
        security.ticker == ticker && portfolio::fund_or_other(security.kind.as_deref())
    });
    if fund {
        return;
    }
    match providers.fundamentals(ticker).await {
        Ok(fundamentals) => summary.fundamentals = fundamentals,
        Err(_) => summary.missing("fundamentals"),
    }
}

/// Runs one traced summary operation and logs it in `runs`, with the provider failures it
/// saw.
async fn traced<T>(
    runs: &SummaryRuns,
    kind: &'static str,
    period: Period,
    ticker: Option<&str>,
    work: impl AsyncFnOnce(&mut Trace) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let mut trace = Trace::start(kind, period, ticker);
    let (result, providers) = http::observed(work(&mut trace)).await;
    let (error, partial) = match &result {
        Ok((_, partial)) => (None, *partial),
        Err(error) => (Some(error.clone()), false),
    };
    runs.record(trace.finish(error, partial, providers));
    result.map(|(value, _)| value)
}

/// What Data health checks about the summary: whether news is expected, how fresh it is,
/// the latest run in `runs`, and the calibration rates when they could be read.
pub async fn summary_health(
    providers: &Providers,
    runs: &SummaryRuns,
    snapshot: &Snapshot,
    calibration: Option<CalibrationSummary>,
) -> crate::health::SummaryHealth {
    let stocks = !modeled(&held_securities(&snapshot.holdings), None).is_empty();
    crate::health::SummaryHealth {
        news_expected: stocks
            && providers
                .integration_status()
                .is_ok_and(|status| status.alpaca),
        newest_story: providers.newest_story().await,
        latest_run: runs.list().into_iter().next(),
        calibration,
    }
}

/// The summary for `period`, from the saved session records it can use, with the closed
/// sessions it computed for the caller to save. The run is logged in `runs`.
pub async fn investment_summary(
    providers: &Providers,
    runs: &SummaryRuns,
    snapshot: &Snapshot,
    period: Period,
    ticker: Option<String>,
    clock: SessionClock,
    kept: Kept,
) -> Result<(InvestmentSummary, Closed), String> {
    let scope = ticker.clone();
    traced(runs, "summary", period, scope.as_deref(), async |trace| {
        let prepared =
            prepare_summary(providers, snapshot, period, ticker, clock, kept, trace).await?;
        let partial = !prepared.summary.unavailable.is_empty();
        Ok(((prepared.summary, prepared.closed), partial))
    })
    .await
}

/// How long after a session's close its prices still move: the close job waits as long.
const SETTLE_MINUTES: i64 = 30;
/// While prices move, a saved summary stands for a quarter hour.
const MOVING_STEP_SECONDS: i64 = 15 * 60;

/// A hash of what a summary of `period` would be made from, so a saved summary made from the
/// same inputs is shown without asking for market data again. It covers everything the
/// summary reads that is saved on this Mac: the finances (`finances`, the storage's own
/// stamp of them), the saved sessions and events, and the news cache's stories, filings,
/// readings, reports, and expectations for the stocks it models. Prices come over the network
/// and can't be hashed before they're fetched, so the key also holds where the market stands:
/// while the latest session trades, and for half an hour after its close, it changes every
/// quarter hour; after that only with the session or the day. `version` changes with what a
/// summary is or how it's made, so a summary saved by an older build is never shown.
#[allow(clippy::too_many_arguments)]
pub async fn inputs_key(
    providers: &Providers,
    snapshot: &Snapshot,
    finances: &str,
    period: Period,
    ticker: Option<&str>,
    clock: &SessionClock,
    now: DateTime<Utc>,
    kept: &Kept,
    version: u32,
) -> Result<String, String> {
    let ledger = Ledger::from_snapshot(snapshot);
    if let Some(ticker) = ticker {
        if !ledger.held.iter().any(|security| security.ticker == ticker) {
            return Err(format!("{ticker} isn’t held"));
        }
    }
    let saved = saved_evidence(providers, &modeled(&ledger.held, ticker), period).await?;
    let settled = now >= Session { date: clock.date }.close() + Duration::minutes(SETTLE_MINUTES);
    let market = if settled {
        format!("settled {}", clock.date)
    } else {
        format!("moving {}", now.timestamp().div_euclid(MOVING_STEP_SECONDS))
    };
    let inputs = json!({
        "version": version,
        "ticker": ticker,
        "period": [period.start, period.end, period.through],
        "today": clock.today,
        "market": market,
        "finances": finances,
        "saved": saved,
        "sessions": kept.sessions,
        "events": kept.events,
        "actions": kept.corporate_actions,
    });
    Ok(crate::database::sha256(
        &serde_json::to_vec(&inputs).map_err(|error| error.to_string())?,
    ))
}

/// Records the session that ends `period` and any missing before it, for the close job:
/// the portfolio's summary without the model, logged in `runs` as a close.
pub async fn close_session(
    providers: &Providers,
    runs: &SummaryRuns,
    snapshot: &Snapshot,
    period: Period,
    clock: SessionClock,
    kept: Kept,
) -> Result<Closed, String> {
    traced(runs, "close", period, None, async |trace| {
        let prepared =
            prepare_summary(providers, snapshot, period, None, clock, kept, trace).await?;
        let partial = !prepared.summary.unavailable.is_empty();
        Ok((prepared.closed, partial))
    })
    .await
}

/// How far back a holding's page looks for its latest report.
const HOLDING_REPORT_DAYS: i64 = 120;

/// Each saved earnings report in reach, newest first: the period's for the portfolio, the
/// latest for a holding. Its reaction is the stock's own move (its `own` layer) over the first
/// session that could react and the next.
fn earnings_digests(
    reports: &[(String, earnings::SavedReport)],
    evidence: &[Evidence],
    period: Period,
    ticker: Option<&str>,
) -> Vec<EarningsDigest> {
    let earliest = match ticker {
        Some(_) => period.end - Duration::days(HOLDING_REPORT_DAYS),
        None => period.start,
    };
    let mut digests: Vec<EarningsDigest> = reports
        .iter()
        .filter(|(symbol, _)| ticker.is_none_or(|ticker| ticker == symbol))
        .filter_map(|(symbol, report)| {
            let quarter = report.quarter.as_ref();
            let quarter_day = quarter.and_then(|quarter| {
                NaiveDate::parse_from_str(&quarter.reported_date, "%Y-%m-%d").ok()
            });
            let reported_on = report
                .filing
                .as_ref()
                .and_then(|filing| session::date_of(&filing.filed_at))
                .or(quarter_day)?;
            if reported_on < earliest || reported_on > period.through {
                return None;
            }
            let reacting = match (
                &report.filing,
                quarter.and_then(|q| q.report_time.as_deref()),
            ) {
                (Some(filing), _) => reacting_session(&filing.filed_at)?,
                (None, Some("post-market")) => Session::on_or_after(reported_on).next().date,
                (None, _) => Session::on_or_after(reported_on).date,
            };
            let own: Vec<(f64, f64)> = evidence
                .iter()
                .find(|holding| &holding.ticker == symbol)
                .map(|holding| {
                    holding
                        .own
                        .range(reacting..)
                        .take(2)
                        .map(|(_, parts)| *parts)
                        .collect()
                })
                .unwrap_or_default();
            let reaction = (!own.is_empty()).then(|| {
                (
                    own.iter().map(|(pct, _)| pct).sum::<f64>(),
                    own.iter()
                        .map(|(_, sigma)| sigma * sigma)
                        .sum::<f64>()
                        .sqrt(),
                )
            });
            let eps = quarter.and_then(|quarter| quarter.reported_eps);
            let estimate = quarter.and_then(|quarter| quarter.estimated_eps);
            Some(EarningsDigest {
                ticker: symbol.clone(),
                reported_on,
                fiscal_date_ending: quarter.map(|quarter| quarter.fiscal_date_ending.clone()),
                report_time: quarter.and_then(|quarter| quarter.report_time.clone()),
                eps,
                eps_estimate: estimate,
                surprise_pct: quarter.and_then(|quarter| quarter.surprise_pct),
                revenue: report
                    .release
                    .as_ref()
                    .and_then(|release| release.revenue.clone()),
                guidance: report
                    .release
                    .as_ref()
                    .map(|release| release.guidance.clone())
                    .unwrap_or_default(),
                guidance_by: report
                    .release
                    .as_ref()
                    .map(|release| release.guidance_by.clone()),
                release_read: report.release.is_some(),
                release_url: report.release.as_ref().map(|release| release.url.clone()),
                filing_url: report.filing.as_ref().map(|filing| filing.url.clone()),
                reaction_pct: reaction.map(|(pct, _)| (pct * 100.0).round() / 100.0),
                reaction_typical_pct: reaction
                    .map(|(_, typical)| (typical * 100.0).round() / 100.0),
                expected_move_pct: report.expected_move_pct,
                call_guidance: report
                    .call_guidance
                    .as_ref()
                    .map(|(sentences, _)| sentences.clone())
                    .unwrap_or_default(),
                call_guidance_by: report.call_guidance.as_ref().map(|(_, by)| by.clone()),
                line: brief::earnings_line(symbol, eps, estimate, reaction),
            })
        })
        .collect();
    digests.sort_by(|a, b| {
        b.reported_on
            .cmp(&a.reported_on)
            .then(a.ticker.cmp(&b.ticker))
    });
    digests
}

/// Whether a holding's day, by its largest layer, was its own or its group's, so a reason
/// for it says something the market's doesn't. A holding without layers may have one.
fn moved_on_its_own(kind: Option<&str>) -> bool {
    !matches!(kind, Some("withMarket" | "quiet"))
}

/// Events worth a line under "Changed": what happened at a company or in the market, not
/// your own trades or Brief's statistical flags, and not a story already quoted.
fn changed(events: &[ListedEvent], quoted: &[headlines::Headline]) -> Vec<ListedEvent> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event.family,
                eventlog::Family::Disclosure
                    | eventlog::Family::CorporateAction
                    | eventlog::Family::News
                    | eventlog::Family::Macro
            )
        })
        .filter(|event| {
            event.url.as_deref().is_none_or(|url| {
                !quoted.iter().any(|headline| headline.url == url)
                    && !event
                        .coverage
                        .iter()
                        .any(|story| quoted.iter().any(|headline| headline.url == story.url))
            })
        })
        .take(LISTED_EVENTS)
        .cloned()
        .collect()
}

/// The day in lines and the headlines that explain it. A quiet day gets its figures and what
/// changed, without reasons.
#[allow(clippy::too_many_arguments)]
fn day_brief(
    today: &TodayMove,
    ticker: Option<&str>,
    held: &[HeldSecurity],
    stories: &[MarketNewsArticle],
    causes: &BTreeMap<String, reading::Cause>,
    events: &[ListedEvent],
    session: &SessionClock,
    day: NaiveDate,
) -> Brief {
    // A holding's dollars: what its position moved at the day's change.
    let position_change = ticker.and_then(|ticker| {
        let value: f64 = held
            .iter()
            .filter(|security| security.ticker == ticker)
            .map(|security| security.value)
            .sum();
        let pct = today.change_pct?;
        (value > 0.0).then(|| value - value / (1.0 + pct / 100.0))
    });
    let lines = brief::lines(today, ticker, position_change, session);
    if brief::quiet(today) {
        return Brief {
            market: None,
            lines,
            headlines: Vec::new(),
            changed: changed(events, &[]),
            earnings: Vec::new(),
        };
    }
    // A holding that moved with the market or stood still gets no reason of its own: the
    // market's wrap is its reason.
    let movers: Vec<(String, f64)> = match (ticker, today.change_pct) {
        (Some(ticker), Some(change_pct)) if moved_on_its_own(today.kind) => {
            vec![(ticker.to_owned(), change_pct)]
        }
        (Some(_), _) => Vec::new(),
        _ => brief::leaders(today)
            .into_iter()
            .filter(|position| moved_on_its_own(position.kind))
            .map(|position| (position.ticker.clone(), position.change_pct))
            .collect(),
    };
    let names = crate::providers::feeds::names(
        &held
            .iter()
            .map(|security| (security.ticker.clone(), security.name.clone()))
            .collect::<Vec<_>>(),
    );
    // Stories from the previous session's close until the next session opens.
    let session_day = Session { date: day };
    let picked = headlines::pick(
        stories,
        &names,
        &movers,
        today.market.as_ref().map(|market| market.change_pct),
        causes,
        session_day.previous().close(),
        session_day.next().open(),
    );
    let changed = changed(events, &picked.movers);
    Brief {
        market: picked.market,
        lines,
        headlines: picked.movers,
        changed,
        earnings: Vec::new(),
    }
}
