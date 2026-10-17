//! Investment returns per brokerage account: time-weighted (what the holdings did) and
//! money-weighted (what the money earned, given when it went in), each beside the same
//! measure for the benchmark. Both come from the committed daily values and net deposits;
//! an account without known flows has no return, never an estimated one.
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use crate::finance_contract::{Performance, PerformancePoint};
use crate::numeric::round2;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Window {
    Day,
    Week,
    Month,
    Quarter,
    YearToDate,
    Year,
    All,
}

pub(super) const WINDOWS: [Window; 7] = [
    Window::Day,
    Window::Week,
    Window::Month,
    Window::Quarter,
    Window::YearToDate,
    Window::Year,
    Window::All,
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowReturn {
    pub window: Window,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Percent over the window, compounding each day's market change.
    pub time_weighted_pct: f64,
    /// Percent over the window that, applied to every deposit from its date, reaches the
    /// ending value. Unavailable when no single rate explains the flows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub money_weighted_pct: Option<f64>,
    /// The benchmark's time-weighted percent over the same days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_pct: Option<f64>,
    /// Deposits less withdrawals during the window.
    pub net_deposits: f64,
    /// Ending value less starting value and net deposits.
    pub market_gain: f64,
    /// `ahead`, `behind`, or `inline` (within 0.05 points) of the benchmark.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_benchmark: Option<&'static str>,
}

/// How one return compares with another: within 0.05 points reads as in line.
pub fn compare(you: f64, benchmark: f64) -> &'static str {
    if (you - benchmark).abs() < 0.05 {
        "inline"
    } else if you > benchmark {
        "ahead"
    } else {
        "behind"
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountReturns {
    pub account_id: String,
    pub windows: Vec<WindowReturn>,
}

/// One day of a performance history with its running deposits, when they are known.
pub(super) struct Day {
    pub(super) date: NaiveDate,
    pub(super) value: f64,
    pub(super) deposits: f64,
    pub(super) benchmark: Option<f64>,
}

/// A history's days, or `None` when any point lacks a date or its deposits.
pub(super) fn days(points: &[PerformancePoint]) -> Option<Vec<Day>> {
    points
        .iter()
        .map(|point| {
            Some(Day {
                date: NaiveDate::parse_from_str(point.date.get(..10)?, "%Y-%m-%d").ok()?,
                value: point.value,
                deposits: point.net_deposits?,
                benchmark: point.sp500,
            })
        })
        .collect()
}

/// Compounds each interval's change with its flow counted at the end of the interval, the
/// convention the committed `marketChange` uses. A day that opens at zero is funded first.
pub(super) fn time_weighted(values: &[(f64, f64)]) -> Option<f64> {
    let mut growth = 1.0;
    for pair in values.windows(2) {
        let (prior, prior_deposits) = pair[0];
        let (value, deposits) = pair[1];
        let flow = deposits - prior_deposits;
        let base = if prior > 0.0 { prior } else { flow };
        if base <= 0.0 {
            // Nothing was invested across this interval.
            continue;
        }
        let ending = if prior > 0.0 { value - flow } else { value };
        growth *= ending / base;
        if !growth.is_finite() || growth < 0.0 {
            return None;
        }
    }
    Some((growth - 1.0) * 100.0)
}

/// The daily rate at which the opening value and every later flow grow to the ending value,
/// found by bisection and returned as a percent over the whole window.
fn money_weighted(days: &[Day]) -> Option<f64> {
    let first = days.first()?;
    let last = days.last()?;
    let mut flows = vec![(0_i64, first.value)];
    flows.extend(days.windows(2).filter_map(|pair| {
        let flow = pair[1].deposits - pair[0].deposits;
        (flow != 0.0).then(|| ((pair[1].date - first.date).num_days(), flow))
    }));
    money_weighted_flows(&flows, (last.date - first.date).num_days(), last.value)
}

/// The percent over `span` days at which money put in (positive) and taken out (negative) on
/// the given days grows to `ending`. Unavailable when no single rate explains the flows.
pub fn money_weighted_flows(flows: &[(i64, f64)], span: i64, ending: f64) -> Option<f64> {
    if span <= 0 || flows.iter().all(|(_, flow)| *flow <= 0.0) {
        return None;
    }
    // Future value of every flow at the window's end, less the actual ending value.
    let excess = |rate: f64| {
        flows
            .iter()
            .map(|(day, flow)| flow * (1.0 + rate).powi((span - day) as i32))
            .sum::<f64>()
            - ending
    };
    // Daily rates are small, so the narrowest bracket that holds the answer is used: wide
    // ones overflow over a long history.
    let brackets = [0.001, 0.01, 0.1, 0.999]
        .into_iter()
        .flat_map(|bound| [(0.0, bound), (-bound, 0.0)]);
    let (mut low, mut high, at_low) = brackets
        .filter_map(|(low, high)| {
            let (at_low, at_high) = (excess(low), excess(high));
            (at_low.is_finite() && at_high.is_finite() && at_low.signum() != at_high.signum())
                .then_some((low, high, at_low))
        })
        .next()?;
    for _ in 0..100 {
        let middle = (low + high) / 2.0;
        if excess(middle).signum() == at_low.signum() {
            low = middle;
        } else {
            high = middle;
        }
    }
    let total = ((1.0 + (low + high) / 2.0).powi(span as i32) - 1.0) * 100.0;
    total.is_finite().then_some(total)
}

pub(super) fn window_start(window: Window, end: NaiveDate) -> Option<NaiveDate> {
    Some(match window {
        // The latest close before the last day: the day's move.
        Window::Day => end - Duration::days(1),
        Window::Week => end - Duration::days(7),
        Window::Month => end - Duration::days(30),
        Window::Quarter => end - Duration::days(91),
        Window::YearToDate => NaiveDate::from_ymd_opt(end.year(), 1, 1)? - Duration::days(1),
        Window::Year => end - Duration::days(365),
        Window::All => return None,
    })
}

fn window_return(window: Window, days: &[Day]) -> Option<WindowReturn> {
    let end = days.last()?.date;
    // The latest observation on or before the window's start is its opening value.
    let from = match window_start(window, end) {
        Some(start) => {
            if days.first()?.date > start {
                // History begins inside the window, so it cannot cover the whole of it.
                return None;
            }
            days.iter().rposition(|day| day.date <= start)?
        }
        None => 0,
    };
    let slice = &days[from..];
    if slice.len() < 2 {
        return None;
    }
    let first = slice.first()?;
    let last = slice.last()?;
    let values = slice
        .iter()
        .map(|day| (day.value, day.deposits))
        .collect::<Vec<_>>();
    let benchmark = slice
        .iter()
        .map(|day| day.benchmark.map(|value| (value, day.deposits)))
        .collect::<Option<Vec<_>>>()
        .and_then(|values| time_weighted(&values));
    let net_deposits = last.deposits - first.deposits;
    let time_weighted_pct = round2(time_weighted(&values)?);
    Some(WindowReturn {
        window,
        start: first.date,
        end: last.date,
        time_weighted_pct,
        vs_benchmark: benchmark.map(|benchmark| compare(time_weighted_pct, round2(benchmark))),
        money_weighted_pct: money_weighted(slice).map(round2),
        benchmark_pct: benchmark.map(round2),
        net_deposits: round2(net_deposits),
        market_gain: round2(last.value - first.value - net_deposits),
    })
}

/// Returns for every brokerage account whose history records deposits and withdrawals.
pub fn returns(performance: &[Performance]) -> Vec<AccountReturns> {
    performance
        .iter()
        .filter(|account| account.performance_method.as_deref() != Some("value-only"))
        .filter_map(|account| {
            let days = days(&account.points)?;
            let windows = WINDOWS
                .iter()
                .filter_map(|window| window_return(*window, &days))
                .collect::<Vec<_>>();
            (!windows.is_empty()).then(|| AccountReturns {
                account_id: account.account_id.clone(),
                windows,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(points: &[(&str, f64, Option<f64>, Option<f64>)]) -> Performance {
        serde_json::from_value(serde_json::json!({
            "accountId": "snaptrade:one", "name": "One", "institution": "Broker",
            "currentValue": points.last().unwrap().1,
            "points": points.iter().map(|(date, value, deposits, benchmark)| serde_json::json!({
                "date": date, "value": value, "netDeposits": deposits, "sp500": benchmark,
            })).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn all(performance: &Performance) -> WindowReturn {
        returns(std::slice::from_ref(performance))[0]
            .windows
            .iter()
            .find(|window| window.window == Window::All)
            .unwrap()
            .clone()
    }

    #[test]
    fn a_deposit_is_not_counted_as_a_gain() {
        // 1,000 grows 10%, then 1,000 more arrives and the whole grows another 10%.
        let result = all(&account(&[
            ("2026-01-01", 1_000.0, Some(1_000.0), Some(1_000.0)),
            ("2026-01-02", 2_100.0, Some(2_000.0), Some(2_050.0)),
            ("2026-01-03", 2_310.0, Some(2_000.0), Some(2_152.5)),
        ]));
        assert_eq!(result.time_weighted_pct, 21.0);
        assert_eq!(result.benchmark_pct, Some(10.25));
        assert_eq!(result.net_deposits, 1_000.0);
        assert_eq!(result.market_gain, 310.0);
    }

    #[test]
    fn timing_separates_money_weighted_from_time_weighted() {
        // The account doubles while small, then falls by half after a large deposit: the
        // holdings broke even, but most of the money only saw the fall.
        let result = all(&account(&[
            ("2026-01-01", 100.0, Some(100.0), None),
            ("2026-01-02", 1_200.0, Some(1_100.0), None),
            ("2026-01-03", 600.0, Some(1_100.0), None),
        ]));
        assert_eq!(result.time_weighted_pct, 0.0);
        assert!(result.money_weighted_pct.unwrap() < -40.0);
        assert_eq!(result.benchmark_pct, None);
    }

    #[test]
    fn a_steady_rate_gives_the_same_answer_both_ways() {
        let result = all(&account(&[
            ("2026-01-01", 1_000.0, Some(1_000.0), None),
            ("2026-01-02", 1_010.0, Some(1_000.0), None),
            ("2026-01-03", 1_020.1, Some(1_000.0), None),
        ]));
        assert_eq!(result.time_weighted_pct, 2.01);
        assert_eq!(result.money_weighted_pct, Some(2.01));
    }

    #[test]
    fn accounts_without_known_flows_or_enough_history_have_no_return() {
        assert!(returns(&[account(&[
            ("2026-01-01", 1_000.0, None, None),
            ("2026-01-02", 1_100.0, None, None),
        ])])
        .is_empty());
        let short = returns(&[account(&[
            ("2026-01-01", 1_000.0, Some(1_000.0), None),
            ("2026-01-10", 1_100.0, Some(1_000.0), None),
        ])]);
        // Nine days of history state a day's and a week's return, but not a month's or a year's.
        assert_eq!(
            short[0]
                .windows
                .iter()
                .map(|window| window.window)
                .collect::<Vec<_>>(),
            vec![Window::Day, Window::Week, Window::All]
        );
        let mut value_only = account(&[
            ("2026-01-01", 1_000.0, Some(1_000.0), None),
            ("2026-01-02", 1_100.0, Some(1_000.0), None),
        ]);
        value_only.performance_method = Some("value-only".into());
        assert!(returns(&[value_only]).is_empty());
    }

    #[test]
    fn an_account_funded_after_its_first_observation_starts_at_the_deposit() {
        let result = all(&account(&[
            ("2026-01-01", 0.0, Some(0.0), None),
            ("2026-01-02", 1_050.0, Some(1_000.0), None),
            ("2026-01-03", 1_155.0, Some(1_000.0), None),
        ]));
        assert_eq!(result.time_weighted_pct, 15.5);
    }
}
