//! A period reconciled to the cent, before anything is explained. The statement identity
//! (start + deposits − withdrawals + investment gain = end) comes from the committed daily values
//! of the combined brokerages; the waterfall books realized gains, dividends, interest, fees, and
//! taxes from the ledger, with unrealized change as the plug, so it ties by construction and a
//! test asserts it. Two returns carry plain labels: time-weighted ("how your investments did",
//! daily-linked) and money-weighted ("how your money did", the rate over daily-dated flows).
//! Nothing under a year is annualized.
//!
//! With daily closes, each holding's contribution is its start-of-day weight times its daily
//! total return, with cash and anything unpriced as one more holding so a day's parts sum to the
//! day's return, then Carino-smoothed so the period's parts sum to the period's return. Dollars
//! rank the return drivers; each security's own return ranks the top movers.
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::Serialize;

use super::returns::{
    days, money_weighted_flows, time_weighted, window_start, Day, Window, WINDOWS,
};
use crate::finance_contract::{Holding, Performance, Snapshot, Trade, Transaction};
use crate::numeric::{from_minor, minor_units, round2};
use crate::providers::closes::{CloseSeries, DailyCloses};
use crate::session::is_trading_day;
use crate::transaction_policy::Kind;

/// Sessions in a year, for annualizing daily volatility.
const SESSIONS_A_YEAR: f64 = 252.0;
/// Fewer daily returns than this say nothing about volatility or beta.
const MIN_RISK_SESSIONS: usize = 20;
/// Drivers and movers listed on each side.
const LISTED: usize = 3;
/// The key of the row that holds cash and everything not priced.
pub const CASH: &str = "cash";

/// The period's statement for the combined brokerages.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    /// The chart window this statement covers, or `None` for a summary's own start.
    pub window: Option<Window>,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub start_value: f64,
    pub end_value: f64,
    pub deposits: f64,
    /// Money taken out, as a positive amount.
    pub withdrawals: f64,
    pub net_flows: f64,
    /// End less start less net flows: what the investments earned.
    pub investment_gain: f64,
    /// How your investments did: each day's change linked, flows excluded.
    pub time_weighted_pct: f64,
    /// How your money did: the rate that grows the start and every flow to the end.
    pub money_weighted_pct: Option<f64>,
    /// Yearly rates, only for a period of a year or more.
    pub annualized: Option<Annualized>,
    /// The S&P 500's time-weighted return over the same days.
    pub benchmark_pct: Option<f64>,
    /// Time-weighted return less the benchmark's, in points.
    pub active_pct: Option<f64>,
    pub waterfall: Waterfall,
    pub risk: Risk,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annualized {
    pub time_weighted_pct: f64,
    pub money_weighted_pct: Option<f64>,
}

/// Start, flows, and what the investments earned, line by line, adding to the end to the cent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waterfall {
    pub start: f64,
    pub deposits: f64,
    pub withdrawals: f64,
    /// Estimated gains on the period's sales; `None` when a sale has no known cost.
    pub realized: Option<f64>,
    /// The price change still held; `None` when `realized` is.
    pub unrealized: Option<f64>,
    /// Realized and unrealized together: the investment gain less income, fees, and taxes.
    pub price_gain: f64,
    /// Dividends on their pay date, reinvested or not.
    pub dividends: f64,
    pub interest: f64,
    /// Fees and taxes, as positive amounts that reduce the gain.
    pub fees: f64,
    pub taxes: f64,
    pub end: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk {
    /// The deepest fall from a high in the time-weighted index, so a withdrawal never reads as
    /// a loss; zero or negative.
    pub drawdown_pct: Option<f64>,
    /// The daily time-weighted return's standard deviation, annualized.
    pub volatility_pct: Option<f64>,
    /// How far the portfolio moved for each 1% the S&P 500 moved, from the period's sessions.
    pub beta: Option<BetaFit>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BetaFit {
    pub beta: f64,
    /// The share of the portfolio's daily variance the S&P 500 explains.
    pub r_squared: f64,
    pub sessions: usize,
}

/// How spread out the holdings are now.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exposure {
    pub holdings: usize,
    /// One over the sum of squared weights: ten equal holdings are ten; one at 40% beside
    /// nine equal ones makes five.
    pub effective_holdings: f64,
    /// The five largest holdings' share of the securities held.
    pub top_five_pct: f64,
}

/// One period, reconciled: the statement, how concentrated the holdings are, and the return
/// split among them when daily closes are known.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodReport {
    /// `None` when the combined brokerages' history doesn't record flows or reach the start.
    pub statement: Option<Statement>,
    pub exposure: Option<Exposure>,
    pub attribution: Option<Attribution>,
}

/// The report for the period from `start`, from the committed snapshot and daily closes.
/// `only` names one holding, whose page shows its part beside the rest of the portfolio.
pub fn report(
    snapshot: &Snapshot,
    closes: &DailyCloses,
    start: NaiveDate,
    only: Option<&str>,
) -> PeriodReport {
    let Snapshot {
        brokerage_performance: performance,
        holdings,
        trades,
        transactions,
        ..
    } = snapshot;
    PeriodReport {
        statement: statement(performance, trades, transactions, start),
        exposure: exposure(performance, holdings),
        attribution: (!closes.is_empty())
            .then(|| attribute(performance, holdings, trades, closes, start, only))
            .flatten(),
    }
}

/// What the ledger books in a period for the given accounts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Booked {
    realized: Option<i64>,
    dividends: i64,
    interest: i64,
    fees: i64,
    taxes: i64,
}

fn day_of(value: &str) -> Option<NaiveDate> {
    value
        .get(..10)
        .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
}

/// The points from the latest on or before `start` through the last.
fn window(days: &[Day], start: Option<NaiveDate>) -> Option<&[Day]> {
    let from = match start {
        Some(start) => {
            if days.first()?.date > start {
                return None;
            }
            days.iter().rposition(|day| day.date <= start)?
        }
        None => 0,
    };
    let slice = &days[from..];
    (slice.len() >= 2).then_some(slice)
}

/// Each interval's return, its flow counted at the interval's end as the time-weighted return
/// counts it; an interval that opens at zero has none.
fn daily_returns(days: &[Day], value: impl Fn(&Day) -> Option<f64>) -> Vec<Option<f64>> {
    days.windows(2)
        .map(|pair| {
            let (prior, current) = (value(&pair[0])?, value(&pair[1])?);
            let flow = pair[1].deposits - pair[0].deposits;
            (prior > 0.0).then(|| (current - flow) / prior - 1.0)
        })
        .collect()
}

/// The ledger's realized gains, income, fees, and taxes after `start` through `end`, in
/// `accounts`. A reinvested dividend reported only as a reinvestment trade counts once.
fn booked(
    trades: &[Trade],
    transactions: &[Transaction],
    accounts: &BTreeSet<String>,
    start: NaiveDate,
    end: NaiveDate,
) -> Booked {
    let inside = |date: &str| day_of(date).is_some_and(|day| day > start && day <= end);
    let mut booked = Booked {
        realized: Some(0),
        ..Booked::default()
    };
    let mut dividends = BTreeSet::new();
    for transaction in transactions {
        let date = transaction
            .posted_on
            .as_deref()
            .unwrap_or(&transaction.date);
        if transaction.pending
            || !inside(date)
            || !transaction
                .account_id
                .as_ref()
                .is_some_and(|id| accounts.contains(id))
        {
            continue;
        }
        let amount = minor_units(transaction.amount);
        match transaction.classification.as_ref().map(|c| &c.kind) {
            Some(Kind::Dividend) => {
                booked.dividends += amount;
                dividends.insert((
                    transaction.account_id.clone().unwrap_or_default(),
                    day_of(date),
                    amount.abs(),
                ));
            }
            Some(Kind::Interest) => booked.interest += amount,
            Some(Kind::Fee) => booked.fees -= amount,
            Some(Kind::Tax) => booked.taxes -= amount,
            _ => {}
        }
    }
    for trade in trades {
        if !inside(&trade.date) || !accounts.contains(&trade.account_id) {
            continue;
        }
        let kind = trade.r#type.to_ascii_uppercase();
        if kind.contains("REINVEST") {
            let amount = minor_units(trade.amount.abs());
            let key = (trade.account_id.clone(), day_of(&trade.date), amount);
            if !dividends.contains(&key) {
                booked.dividends += amount;
            }
        } else if kind.contains("SELL") {
            booked.realized = booked
                .realized
                .zip(
                    trade
                        .estimated_realized_gain
                        .filter(|gain| gain.is_finite())
                        .map(minor_units),
                )
                .map(|(total, gain)| total + gain);
        }
    }
    booked
}

fn risk(days: &[Day]) -> Risk {
    let portfolio = daily_returns(days, |day| Some(day.value));
    let benchmark = daily_returns(days, |day| day.benchmark);
    let mut index = 1.0_f64;
    let mut peak = 1.0_f64;
    let mut drawdown = 0.0_f64;
    for r in portfolio.iter().flatten() {
        index *= 1.0 + r;
        peak = peak.max(index);
        drawdown = drawdown.min(index / peak - 1.0);
    }
    // Sessions only: a weekend's flat day isn't a day of risk.
    let sessions: Vec<(f64, Option<f64>)> = days[1..]
        .iter()
        .zip(portfolio.iter().zip(&benchmark))
        .filter(|(day, _)| is_trading_day(day.date))
        .filter_map(|(_, (r, b))| Some(((*r)?, *b)))
        .collect();
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len().max(1) as f64;
    let own: Vec<f64> = sessions.iter().map(|(r, _)| *r).collect();
    let volatility = (own.len() >= MIN_RISK_SESSIONS).then(|| {
        let m = mean(&own);
        let variance =
            own.iter().map(|r| (r - m).powi(2)).sum::<f64>() / (own.len() - 1).max(1) as f64;
        round2(variance.sqrt() * SESSIONS_A_YEAR.sqrt() * 100.0)
    });
    let paired: Vec<(f64, f64)> = sessions
        .iter()
        .filter_map(|(r, b)| Some((*r, (*b)?)))
        .collect();
    let beta = (paired.len() >= MIN_RISK_SESSIONS)
        .then(|| {
            let x: Vec<f64> = paired.iter().map(|(_, b)| *b).collect();
            let y: Vec<f64> = paired.iter().map(|(r, _)| *r).collect();
            let (mx, my) = (mean(&x), mean(&y));
            let sxx: f64 = x.iter().map(|x| (x - mx).powi(2)).sum();
            let syy: f64 = y.iter().map(|y| (y - my).powi(2)).sum();
            let sxy: f64 = x.iter().zip(&y).map(|(x, y)| (x - mx) * (y - my)).sum();
            (sxx > 0.0 && syy > 0.0).then(|| BetaFit {
                beta: round2(sxy / sxx),
                r_squared: round2(sxy * sxy / (sxx * syy)),
                sessions: paired.len(),
            })
        })
        .flatten();
    Risk {
        drawdown_pct: (!portfolio.is_empty()).then(|| round2(drawdown * 100.0)),
        volatility_pct: volatility,
        beta,
    }
}

/// The statement for `days`, labeled with its window.
fn statement_of(
    window: Option<Window>,
    days: &[Day],
    trades: &[Trade],
    transactions: &[Transaction],
    accounts: &BTreeSet<String>,
) -> Option<Statement> {
    let (first, last) = (days.first()?, days.last()?);
    let (mut deposits, mut withdrawals) = (0_i64, 0_i64);
    for pair in days.windows(2) {
        let flow = minor_units(pair[1].deposits) - minor_units(pair[0].deposits);
        if flow > 0 {
            deposits += flow;
        } else {
            withdrawals -= flow;
        }
    }
    let (start, end) = (minor_units(first.value), minor_units(last.value));
    let gain = end - start - deposits + withdrawals;
    let booked = booked(trades, transactions, accounts, first.date, last.date);
    let price_gain = gain - booked.dividends - booked.interest + booked.fees + booked.taxes;
    let values: Vec<(f64, f64)> = days.iter().map(|day| (day.value, day.deposits)).collect();
    let time_weighted_pct = time_weighted(&values)?;
    let benchmark = days
        .iter()
        .map(|day| day.benchmark.map(|value| (value, day.deposits)))
        .collect::<Option<Vec<_>>>()
        .and_then(|values| time_weighted(&values));
    let mut flows = vec![(0_i64, first.value)];
    flows.extend(days.windows(2).filter_map(|pair| {
        let flow = pair[1].deposits - pair[0].deposits;
        (flow != 0.0).then(|| ((pair[1].date - first.date).num_days(), flow))
    }));
    let span = (last.date - first.date).num_days();
    let money_weighted_pct = money_weighted_flows(&flows, span, last.value);
    let yearly = |pct: f64| ((1.0 + pct / 100.0).powf(365.0 / span as f64) - 1.0) * 100.0;
    Some(Statement {
        window,
        start: first.date,
        end: last.date,
        start_value: from_minor(start),
        end_value: from_minor(end),
        deposits: from_minor(deposits),
        withdrawals: from_minor(withdrawals),
        net_flows: from_minor(deposits - withdrawals),
        investment_gain: from_minor(gain),
        time_weighted_pct: round2(time_weighted_pct),
        money_weighted_pct: money_weighted_pct.map(round2),
        annualized: (span >= 365).then(|| Annualized {
            time_weighted_pct: round2(yearly(time_weighted_pct)),
            money_weighted_pct: money_weighted_pct.map(|pct| round2(yearly(pct))),
        }),
        benchmark_pct: benchmark.map(round2),
        active_pct: benchmark.map(|benchmark| round2(time_weighted_pct - benchmark)),
        waterfall: Waterfall {
            start: from_minor(start),
            deposits: from_minor(deposits),
            withdrawals: from_minor(withdrawals),
            realized: booked.realized.map(from_minor),
            unrealized: booked
                .realized
                .map(|realized| from_minor(price_gain - realized)),
            price_gain: from_minor(price_gain),
            dividends: from_minor(booked.dividends),
            interest: from_minor(booked.interest),
            fees: from_minor(booked.fees),
            taxes: from_minor(booked.taxes),
            end: from_minor(end),
        },
        risk: risk(days),
    })
}

/// The combined brokerages' history, and the accounts it combines; `None` without flows.
fn combined(performance: &[Performance]) -> Option<(&Performance, BTreeSet<String>)> {
    let total = performance.iter().find(|account| {
        account.account_id == "total" && account.performance_method.as_deref() != Some("value-only")
    })?;
    let accounts = performance
        .iter()
        .filter(|account| account.account_id != "total")
        .map(|account| account.account_id.clone())
        .collect();
    Some((total, accounts))
}

/// The brokerage accounts the combined history covers.
pub fn investment_accounts(performance: &[Performance]) -> BTreeSet<String> {
    performance
        .iter()
        .filter(|account| account.account_id != "total")
        .map(|account| account.account_id.clone())
        .collect()
}

/// The combined brokerages' typical day in dollars: a robust spread (1.4826 times the median
/// absolute deviation) of the latest 250 days' investment gains; `None` without flows.
pub fn typical_day(performance: &[Performance]) -> Option<f64> {
    let (total, _) = combined(performance)?;
    let days = days(&total.points)?;
    let gains: Vec<f64> = days
        .windows(2)
        .filter(|pair| is_trading_day(pair[1].date))
        .map(|pair| pair[1].value - pair[0].value - (pair[1].deposits - pair[0].deposits))
        .collect();
    let mut recent = gains[gains.len().saturating_sub(250)..].to_vec();
    if recent.len() < 20 {
        return None;
    }
    let center = crate::numeric::median(&mut recent)?;
    let mut deviations: Vec<f64> = recent.iter().map(|gain| (gain - center).abs()).collect();
    crate::numeric::median(&mut deviations).map(|deviation| round2(1.4826 * deviation))
}

/// A statement for each chart window the history covers.
pub fn statements(
    performance: &[Performance],
    trades: &[Trade],
    transactions: &[Transaction],
) -> Vec<Statement> {
    let Some((total, accounts)) = combined(performance) else {
        return Vec::new();
    };
    let Some(days) = days(&total.points) else {
        return Vec::new();
    };
    let Some(end) = days.last().map(|day| day.date) else {
        return Vec::new();
    };
    WINDOWS
        .iter()
        .filter_map(|kind| {
            let slice = window(&days, window_start(*kind, end))?;
            statement_of(Some(*kind), slice, trades, transactions, &accounts)
        })
        .collect()
}

/// The statement from `start` through the latest point.
pub fn statement(
    performance: &[Performance],
    trades: &[Trade],
    transactions: &[Transaction],
    start: NaiveDate,
) -> Option<Statement> {
    let (total, accounts) = combined(performance)?;
    let days = days(&total.points)?;
    statement_of(
        None,
        window(&days, Some(start))?,
        trades,
        transactions,
        &accounts,
    )
}

/// The securities held in the combined brokerages, by ticker: shares and value.
fn held(holdings: &[Holding], accounts: &BTreeSet<String>) -> BTreeMap<String, (f64, f64)> {
    let mut held: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for holding in holdings.iter().filter(|holding| {
        accounts.contains(&holding.account_id)
            && holding.currency.as_deref().is_none_or(|code| code == "USD")
    }) {
        let (Some(shares), Some(value)) = (holding.shares, holding.value) else {
            continue;
        };
        if shares.is_finite() && value.is_finite() && shares > 0.0 && value > 0.0 {
            let entry = held.entry(holding.ticker.clone()).or_default();
            entry.0 += shares;
            entry.1 += value;
        }
    }
    held
}

/// How concentrated the combined brokerages' securities are now.
pub fn exposure(performance: &[Performance], holdings: &[Holding]) -> Option<Exposure> {
    let (_, accounts) = combined(performance)?;
    let mut values: Vec<f64> = held(holdings, &accounts)
        .into_values()
        .map(|(_, value)| value)
        .collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return None;
    }
    values.sort_by(|a, b| b.total_cmp(a));
    let squares: f64 = values.iter().map(|value| (value / total).powi(2)).sum();
    Some(Exposure {
        holdings: values.len(),
        effective_holdings: (10.0 / squares).round() / 10.0,
        top_five_pct: (values.iter().take(5).sum::<f64>() / total * 1_000.0).round() / 10.0,
    })
}

/// One holding's part in the period, or the cash and unpriced row.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contribution {
    /// A ticker, or `cash` for cash and every holding without prices.
    pub key: String,
    /// What it earned, in dollars; with the cash row these sum to the investment gain.
    pub dollars: f64,
    /// Its part of the period's time-weighted return, in points, Carino-smoothed so the parts
    /// sum to that return.
    pub contribution_pct: f64,
    /// The security's own total return over the period; `None` for cash.
    pub return_pct: Option<f64>,
    /// Its share of the portfolio at the start and the end.
    pub start_weight_pct: f64,
    pub end_weight_pct: f64,
    /// Bought or sold during the period.
    pub traded: bool,
    /// How its weight changed because prices moved (the start's shares at the end's prices)
    /// and because of trades, in points; `None` for cash.
    pub drift_pct: Option<f64>,
    pub traded_pct: Option<f64>,
    /// What the shares held at the start would have earned held unchanged.
    pub buy_and_hold: Option<f64>,
}

/// The period's return split among the holdings.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    /// Every priced holding and the cash row, largest dollars either way first.
    pub contributions: Vec<Contribution>,
    /// The return drivers: the largest gains and losses in dollars, at most three each, and
    /// the rest of the portfolio together.
    pub gains: Vec<String>,
    pub losses: Vec<String>,
    pub rest: f64,
    /// The top movers: the holdings whose own return rose and fell most, at most three each.
    pub risers: Vec<String>,
    pub fallers: Vec<String>,
    /// How much of the investment gain the three largest drivers explain.
    pub top_three_share_pct: Option<f64>,
    /// What the start's holdings would have earned held unchanged, and what trading added.
    pub buy_and_hold: Option<f64>,
    pub trading: Option<f64>,
    /// Holdings whose shares the saved trades don't explain, or without a close at the start;
    /// their part stays in the cash row.
    pub unpriced: Vec<String>,
}

/// The close on or before `day`.
pub fn close_on(series: &[(NaiveDate, f64)], day: NaiveDate) -> Option<f64> {
    let index = series.partition_point(|(date, _)| *date <= day);
    index.checked_sub(1).map(|index| series[index].1)
}

/// Shares held at the end of each day, rebuilt back from today's from the saved trades; `None`
/// when a trade has no share count or the history goes below zero.
pub struct Shares {
    now: f64,
    /// Trades after the period's start: day and signed shares.
    trades: Vec<(NaiveDate, f64)>,
}

impl Shares {
    pub fn rebuild(ticker: &str, now: f64, trades: &[Trade], start: NaiveDate) -> Option<Self> {
        let mut changes = Vec::new();
        for trade in trades
            .iter()
            .filter(|trade| trade.ticker.as_deref() == Some(ticker))
        {
            let Some(day) = day_of(&trade.date).filter(|day| *day > start) else {
                continue;
            };
            let kind = trade.r#type.to_ascii_uppercase();
            let sign = if kind.contains("BUY") || kind.contains("REINVEST") {
                1.0
            } else if kind.contains("SELL") {
                -1.0
            } else {
                continue;
            };
            let units = trade
                .units
                .filter(|units| units.is_finite() && *units > 0.0)?;
            changes.push((day, sign * units));
        }
        let shares = Self {
            now,
            trades: changes,
        };
        (shares.at(start) >= -1e-6).then_some(shares)
    }

    pub fn at(&self, day: NaiveDate) -> f64 {
        let later: f64 = self
            .trades
            .iter()
            .filter(|(date, _)| *date > day)
            .map(|(_, units)| units)
            .sum();
        (self.now - later).max(0.0)
    }

    fn traded(&self) -> bool {
        !self.trades.is_empty()
    }
}

/// Splits the period from `start` among the holdings, from each security's daily closes
/// (adjusted for splits and dividends, so a close-to-close return is the total return).
/// `only` limits the holdings named to one ticker; the rest join the cash row.
pub fn attribute(
    performance: &[Performance],
    holdings: &[Holding],
    trades: &[Trade],
    closes: &DailyCloses,
    start: NaiveDate,
    only: Option<&str>,
) -> Option<Attribution> {
    let (total, accounts) = combined(performance)?;
    let days = days(&total.points)?;
    let days = window(&days, Some(start))?;
    let (first, last) = (days.first()?, days.last()?);
    let mut unpriced = Vec::new();
    let mut priced: Vec<Priced> = Vec::new();
    for (ticker, (shares, _)) in held(holdings, &accounts) {
        if only.is_some_and(|only| only != ticker) {
            continue;
        }
        let series = closes.get(&ticker).map(Vec::as_slice);
        let rebuilt = Shares::rebuild(&ticker, shares, trades, first.date);
        match (series, rebuilt) {
            (Some(series), Some(rebuilt))
                if rebuilt.at(first.date) <= 0.0 || close_on(series, first.date).is_some() =>
            {
                priced.push((ticker, rebuilt, series));
            }
            _ => unpriced.push(ticker),
        }
    }
    let Daily {
        portfolio,
        parts,
        dollars,
    } = daily(days, &priced);
    let smoothed = carino(&portfolio, &parts);
    let period_return = portfolio.iter().fold(1.0, |growth, r| growth * (1.0 + r)) - 1.0;
    let gain = minor_units(last.value)
        - minor_units(first.value)
        - (minor_units(last.deposits) - minor_units(first.deposits));
    let weight = |value: f64, of: f64| {
        if of > 0.0 {
            round2(value / of * 100.0)
        } else {
            0.0
        }
    };
    let mut contributions: Vec<Contribution> = priced
        .iter()
        .enumerate()
        .map(|(index, (ticker, shares, series))| {
            let at_start = shares.at(first.date);
            let (from, to) = (close_on(series, first.date), close_on(series, last.date));
            let return_pct = from
                .zip(to)
                .filter(|(from, _)| *from > 0.0)
                .map(|(from, to)| round2((to / from - 1.0) * 100.0));
            let start_weight = weight(at_start * from.unwrap_or(0.0), first.value);
            let end_weight = weight(shares.now * to.unwrap_or(0.0), last.value);
            // The start's shares at the end's prices: what prices alone would have made it.
            let held_weight = to.map(|to| weight(at_start * to, last.value));
            Contribution {
                key: ticker.clone(),
                dollars: from_minor(minor_units(dollars[index])),
                contribution_pct: round2(smoothed[index] * 100.0),
                return_pct,
                start_weight_pct: start_weight,
                end_weight_pct: end_weight,
                traded: shares.traded(),
                drift_pct: held_weight.map(|held| round2(held - start_weight)),
                traded_pct: held_weight.map(|held| round2(end_weight - held)),
                buy_and_hold: from
                    .zip(to)
                    .map(|(from, to)| from_minor(minor_units(at_start * (to - from)))),
            }
        })
        .collect();
    // The cash row takes what the holdings don't explain, rounding included, so the dollars
    // sum to the gain and the points to the return exactly as shown.
    let named_dollars: i64 = contributions
        .iter()
        .map(|item| minor_units(item.dollars))
        .sum();
    let named_points: f64 = contributions.iter().map(|item| item.contribution_pct).sum();
    let named_start: f64 = contributions.iter().map(|item| item.start_weight_pct).sum();
    let named_end: f64 = contributions.iter().map(|item| item.end_weight_pct).sum();
    contributions.push(Contribution {
        key: CASH.into(),
        dollars: from_minor(gain - named_dollars),
        contribution_pct: round2(round2(period_return * 100.0) - named_points),
        return_pct: None,
        start_weight_pct: round2(100.0 - named_start),
        end_weight_pct: round2(100.0 - named_end),
        traded: false,
        drift_pct: None,
        traded_pct: None,
        buy_and_hold: None,
    });
    contributions.sort_by(|a, b| {
        b.dollars
            .abs()
            .total_cmp(&a.dollars.abs())
            .then(a.key.cmp(&b.key))
    });
    let holdings_only = || contributions.iter().filter(|item| item.key != CASH);
    let Drivers {
        gains,
        losses,
        rest,
        risers,
        fallers,
    } = drivers(&contributions);
    let top_three: i64 = holdings_only()
        .take(LISTED)
        .map(|item| minor_units(item.dollars))
        .sum();
    let buy_and_hold = holdings_only()
        .map(|item| item.buy_and_hold.map(minor_units))
        .sum::<Option<i64>>();
    Some(Attribution {
        top_three_share_pct: (gain.abs() >= 100)
            .then(|| (top_three as f64 / gain as f64 * 1_000.0).round() / 10.0),
        trading: buy_and_hold.map(|held| from_minor(named_dollars - held)),
        buy_and_hold: buy_and_hold.map(from_minor),
        contributions,
        gains,
        losses,
        rest,
        risers,
        fallers,
        unpriced,
    })
}

/// A priced holding: its ticker, its shares over time, and its closes.
type Priced<'a> = (String, Shares, &'a CloseSeries);

/// Each day's portfolio return and each holding's part of it, its weight times its return, with
/// each holding's dollars over the period.
struct Daily {
    portfolio: Vec<f64>,
    parts: Vec<Vec<f64>>,
    dollars: Vec<f64>,
}

fn daily(days: &[Day], priced: &[Priced<'_>]) -> Daily {
    let mut portfolio = Vec::new();
    let mut parts: Vec<Vec<f64>> = Vec::new();
    let mut dollars = vec![0.0_f64; priced.len()];
    for pair in days.windows(2) {
        let (prior, current) = (&pair[0], &pair[1]);
        let flow = current.deposits - prior.deposits;
        let r = if prior.value > 0.0 {
            (current.value - flow) / prior.value - 1.0
        } else {
            0.0
        };
        let mut day_parts = Vec::with_capacity(priced.len());
        for (index, (_, shares, series)) in priced.iter().enumerate() {
            let held = shares.at(prior.date);
            let part = match (close_on(series, prior.date), close_on(series, current.date)) {
                (Some(before), Some(after)) if held > 0.0 && before > 0.0 && prior.value > 0.0 => {
                    let ret = after / before - 1.0;
                    dollars[index] += held * before * ret;
                    held * before / prior.value * ret
                }
                _ => 0.0,
            };
            day_parts.push(part);
        }
        portfolio.push(r);
        parts.push(day_parts);
    }
    Daily {
        portfolio,
        parts,
        dollars,
    }
}

/// The holdings named as return drivers and top movers, with the rest of the gain together.
struct Drivers {
    gains: Vec<String>,
    losses: Vec<String>,
    rest: f64,
    risers: Vec<String>,
    fallers: Vec<String>,
}

/// From contributions sorted largest dollars first.
fn drivers(contributions: &[Contribution]) -> Drivers {
    let holdings_only = || contributions.iter().filter(|item| item.key != CASH);
    let pick = |gains: bool| -> Vec<String> {
        holdings_only()
            .filter(|item| {
                if gains {
                    item.dollars > 0.0
                } else {
                    item.dollars < 0.0
                }
            })
            .take(LISTED)
            .map(|item| item.key.clone())
            .collect()
    };
    let (gains, losses) = (pick(true), pick(false));
    let listed: BTreeSet<&String> = gains.iter().chain(&losses).collect();
    let rest = contributions
        .iter()
        .filter(|item| !listed.contains(&item.key))
        .map(|item| minor_units(item.dollars))
        .sum::<i64>();
    let mut by_return: Vec<&Contribution> = holdings_only()
        .filter(|item| item.return_pct.is_some())
        .collect();
    by_return.sort_by(|a, b| {
        b.return_pct
            .unwrap_or(0.0)
            .total_cmp(&a.return_pct.unwrap_or(0.0))
            .then(a.key.cmp(&b.key))
    });
    let risers = by_return
        .iter()
        .filter(|item| item.return_pct.is_some_and(|pct| pct > 0.0))
        .take(LISTED)
        .map(|item| item.key.clone())
        .collect();
    let fallers = by_return
        .iter()
        .rev()
        .filter(|item| item.return_pct.is_some_and(|pct| pct < 0.0))
        .take(LISTED)
        .map(|item| item.key.clone())
        .collect();
    Drivers {
        gains,
        losses,
        rest: from_minor(rest),
        risers,
        fallers,
    }
}

/// Carino's smoothing: each day's parts scaled by ln(1 + r_t) / r_t over ln(1 + R) / R, so
/// the parts sum to the period's return `R` rather than to the sum of daily returns. Returns
/// each holding's smoothed part; the cash row is the remainder.
fn carino(portfolio: &[f64], parts: &[Vec<f64>]) -> Vec<f64> {
    let coefficient = |r: f64| {
        if r.abs() < 1e-12 {
            1.0
        } else {
            (1.0 + r).ln() / r
        }
    };
    let period = portfolio.iter().fold(1.0, |growth, r| growth * (1.0 + r)) - 1.0;
    let k = coefficient(period);
    let holdings = parts.first().map_or(0, Vec::len);
    let mut smoothed = vec![0.0; holdings];
    for (r, day) in portfolio.iter().zip(parts) {
        let scale = coefficient(*r) / k;
        for (total, part) in smoothed.iter_mut().zip(day) {
            *total += scale * part;
        }
    }
    smoothed
}

#[cfg(test)]
mod tests;
