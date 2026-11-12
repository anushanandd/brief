//! The portfolio's last week at five-minute resolution, and the month before it at half an
//! hour, built from each holding's price bars.
//!
//! A bar values the day's holdings at that moment's prices. Each day's shares are today's
//! with the buys, reinvestments, and sales since undone; a day with a trade is left out, since
//! its shares changed within it, and so is every day before a share change those can't undo.
//! Each earlier day is then shifted to land on its recorded closing value, which carries that
//! day's cash and deposits; today needs no shift. A day without a recorded value is left out
//! rather than guessed.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::America::New_York;
use serde_json::Value;

use crate::{
    finance_contract::{Point, Snapshot},
    insights::totals::{group_series_id, GroupHistory},
    providers::{holding_market::PriceHistory, MarketSnapshot},
    settings::FinanceSettings,
};

/// How far back the five-minute lines reach, as the holding chart's week does.
pub(super) const WEEK_SECONDS: u64 = 604_800;
/// How far back the half-hour lines reach, as the holding chart's month does.
pub(super) const MONTH_SECONDS: u64 = 2_592_000;

/// Each holding's bars for the lines: the week's, and the month's when every holding has them.
pub(super) struct Bars {
    pub(super) week: BTreeMap<String, PriceHistory>,
    pub(super) month: Option<BTreeMap<String, PriceHistory>>,
}

/// Shares differing by less than this are the same, as the daily history rebuild counts them.
const SHARE_TOLERANCE: f64 = 0.0005;

/// A position: its account and ticker.
type Position = (String, String);

/// Shares by position.
type Shares = BTreeMap<Position, f64>;

fn position(account: &str, ticker: &str) -> Position {
    (account.to_owned(), ticker.trim().to_ascii_uppercase())
}

/// The snapshots days are valued with: the committed one while shares are today's, else a
/// copy holding a day's closing shares, one for each run of days between trades.
struct Held<'a> {
    snapshots: Vec<Cow<'a, Snapshot>>,
    days: BTreeMap<NaiveDate, usize>,
}

impl Held<'_> {
    fn on(&self, day: NaiveDate) -> Option<&Snapshot> {
        self.days.get(&day).map(|index| &*self.snapshots[*index])
    }
}

/// Each day's holdings from `start` through `today`, worked back from today's shares. A day
/// is missing when a trade changed shares on it, when it held a position no longer held, or
/// when a later share change can't be undone (a change other than a buy, reinvestment, or
/// sale, or one without a ticker).
fn held(snapshot: &Snapshot, start: NaiveDate, today: NaiveDate) -> Held<'_> {
    let mut now = Shares::new();
    for holding in &snapshot.holdings {
        *now.entry(position(&holding.account_id, &holding.ticker))
            .or_default() += holding.shares.unwrap_or_default();
    }
    // Each day's share changes; `None` when one can't be undone.
    let mut changes: BTreeMap<NaiveDate, Option<Vec<(Position, f64)>>> = BTreeMap::new();
    for trade in &snapshot.trades {
        let Some(units) = trade.units.filter(|units| *units != 0.0) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(&trade.date, "%Y-%m-%d") else {
            continue;
        };
        let kind = trade.r#type.to_ascii_uppercase();
        let signed = if kind.contains("BUY") || kind.contains("REINVEST") {
            Some(units.abs())
        } else if kind.contains("SELL") {
            Some(-units.abs())
        } else {
            None
        };
        let change = signed
            .zip(trade.ticker.as_deref())
            .map(|(signed, ticker)| (position(&trade.account_id, ticker), signed));
        let day = changes.entry(date).or_insert_with(|| Some(Vec::new()));
        match (day.as_mut(), change) {
            (Some(day), Some(change)) => day.push(change),
            (_, None) => *day = None,
            (None, Some(_)) => {}
        }
    }
    let mut held = Held {
        snapshots: Vec::new(),
        days: BTreeMap::new(),
    };
    let mut shares = now.clone();
    // The snapshot holding `shares`, once one is made.
    let mut current = None;
    let mut later = changes.into_iter().rev().peekable();
    let mut day = today;
    while day >= start {
        while let Some((_, change)) = later.next_if(|(on, _)| *on > day) {
            let Some(change) = change else {
                return held;
            };
            for (position, count) in change {
                *shares.entry(position).or_default() -= count;
            }
            current = None;
        }
        let traded = later.peek().is_some_and(|(on, _)| *on == day);
        let known = shares.iter().all(|(position, count)| {
            *count >= -SHARE_TOLERANCE
                && (count.abs() <= SHARE_TOLERANCE || now.contains_key(position))
        });
        if !traded && known {
            let index = *current.get_or_insert_with(|| {
                held.snapshots.push(if shares == now {
                    Cow::Borrowed(snapshot)
                } else {
                    Cow::Owned(with_shares(snapshot, &now, &shares))
                });
                held.snapshots.len() - 1
            });
            held.days.insert(day, index);
        }
        let Some(previous) = day.pred_opt() else {
            break;
        };
        day = previous;
    }
    held
}

/// The snapshot holding these shares, each position's difference from today's on its first row.
fn with_shares(snapshot: &Snapshot, now: &Shares, shares: &Shares) -> Snapshot {
    let mut copy = snapshot.clone();
    let mut applied = BTreeSet::new();
    for holding in &mut copy.holdings {
        let key = position(&holding.account_id, &holding.ticker);
        if let (Some(then), Some(held), Some(count)) =
            (shares.get(&key), now.get(&key), holding.shares.as_mut())
        {
            if applied.insert(key) {
                *count += then - held;
            }
        }
    }
    copy
}

/// The New York trading day a bar belongs to.
fn market_day(time: i64) -> Option<NaiveDate> {
    Utc.timestamp_opt(time, 0)
        .single()
        .map(|time| time.with_timezone(&New_York).date_naive())
}

/// The regular-session close on a trading day, where recorded daily values are taken.
fn regular_close(day: NaiveDate) -> Option<i64> {
    New_York
        .from_local_datetime(&day.and_hms_opt(16, 0, 0)?)
        .single()
        .map(|close| close.timestamp())
}

/// Each chart line's recorded end-of-day values, by line id and then date.
pub(super) type RecordedCloses = BTreeMap<String, BTreeMap<String, f64>>;

/// The recorded closes of every series `market_chart_values` draws: net worth, the accounts'
/// total without the spending cards (the line Home and All accounts draw), each brokerage's
/// performance, and each account group's history. `snapshot` is the view's, with closed and
/// manual accounts applied, so earlier days land where the saved chart has them.
pub(super) fn recorded_closes(
    snapshot: &Snapshot,
    accounts: &[Point],
    groups: &[GroupHistory],
) -> RecordedCloses {
    let by_date = |points: &[Point]| {
        points
            .iter()
            .map(|point| (point.date.clone(), point.value))
            .collect::<BTreeMap<_, _>>()
    };
    let mut closes = snapshot
        .brokerage_performance
        .iter()
        .map(|account| {
            let points = account
                .points
                .iter()
                .map(|point| (point.date.clone(), point.value))
                .collect();
            (account.account_id.clone(), points)
        })
        .collect::<RecordedCloses>();
    for group in groups {
        closes.insert(group_series_id(&group.account_type), by_date(&group.points));
    }
    closes.insert("net-worth".into(), by_date(&snapshot.net_worth_history));
    closes.insert("accounts".into(), by_date(accounts));
    closes
}

/// Chart points for each series `market_chart_values` draws: the last week at five minutes, and
/// before it, when every holding has the month's bars, the rest of the month at half an hour.
/// Empty when any quoted holding lacks the week's bars, since a partial portfolio would
/// misstate its value. A line's earlier days need its `recorded` close; non-market balances
/// (cash, manual accounts) are what that close carries, so only the holdings move within a day.
pub(super) fn intraday_series(
    snapshot: &Snapshot,
    settings: &FinanceSettings,
    reference: &BTreeMap<String, MarketSnapshot>,
    bars: &Bars,
    recorded: &RecordedCloses,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<BTreeMap<String, Vec<Value>>, String> {
    let quoted: BTreeSet<String> = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.quote_eligible == Some(true))
        .map(|holding| holding.ticker.trim().to_ascii_uppercase())
        .collect();
    if quoted.is_empty()
        || quoted
            .iter()
            .any(|symbol| !bars.week.contains_key(symbol) || !reference.contains_key(symbol))
    {
        return Ok(BTreeMap::new());
    }
    let month = bars
        .month
        .as_ref()
        .filter(|month| quoted.iter().all(|symbol| month.contains_key(symbol)));
    let week_start = now.timestamp() - WEEK_SECONDS as i64;
    let since = now.timestamp() - month.map_or(WEEK_SECONDS, |_| MONTH_SECONDS) as i64;
    let closes_of = |history: &PriceHistory| {
        history
            .bars
            .iter()
            .filter(|bar| bar.close.is_finite() && bar.close > 0.0)
            .map(|bar| (bar.end_time, bar.close))
            .collect::<Vec<_>>()
    };
    // Each moment drawn: the week's bars within it, and the month's before it.
    let mut times = BTreeSet::new();
    let mut closes: BTreeMap<&str, Vec<(i64, f64)>> = BTreeMap::new();
    for symbol in &quoted {
        let week = closes_of(&bars.week[symbol]);
        let earlier = month
            .map(|month| closes_of(&month[symbol]))
            .unwrap_or_default();
        times.extend(
            week.iter()
                .map(|(time, _)| *time)
                .filter(|time| (week_start..=now.timestamp()).contains(time))
                .chain(
                    earlier
                        .iter()
                        .map(|(time, _)| *time)
                        .filter(|time| (since..week_start).contains(time)),
                ),
        );
        // Prices in time order, from the finer bars where both have one.
        let series = earlier.into_iter().chain(week).collect::<BTreeMap<_, _>>();
        closes.insert(symbol, series.into_iter().collect());
    }
    let Some(first) = market_day(since) else {
        return Ok(BTreeMap::new());
    };
    let held = held(snapshot, first, today);
    times.retain(|time| market_day(*time).is_some_and(|day| held.on(day).is_some()));

    // Each day's holdings at each moment's prices, before any day is shifted.
    let mut raw: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
    let mut cursor: BTreeMap<&str, usize> = BTreeMap::new();
    for &time in &times {
        let mut quotes = BTreeMap::new();
        let mut priced = true;
        for (symbol, series) in &closes {
            let index = cursor.entry(symbol).or_default();
            while *index < series.len() && series[*index].0 <= time {
                *index += 1;
            }
            // The latest close at or before this moment; none yet means it can't be priced.
            let Some(&(_, price)) = index.checked_sub(1).and_then(|last| series.get(last)) else {
                priced = false;
                break;
            };
            let mut quote = reference[*symbol].clone();
            quote.price = price;
            quote.as_of = Utc
                .timestamp_opt(time, 0)
                .single()
                .ok_or("Invalid bar time")?
                .to_rfc3339();
            quotes.insert((*symbol).to_owned(), quote);
        }
        if !priced {
            continue;
        }
        let Some(holdings) = market_day(time).and_then(|day| held.on(day)) else {
            continue;
        };
        let projection = super::project(holdings, settings, &quotes, true, today)?;
        for (id, point) in super::market_chart_values(&projection, settings, time) {
            if let Some(value) = point["value"].as_f64().filter(|value| value.is_finite()) {
                raw.entry(id).or_default().push((time, value));
            }
        }
    }

    // Each earlier day lands on its recorded close; today stays as valued.
    let mut series = BTreeMap::new();
    for (id, points) in raw {
        let mut days: BTreeMap<NaiveDate, Vec<(i64, f64)>> = BTreeMap::new();
        for point in points {
            if let Some(day) = market_day(point.0) {
                days.entry(day).or_default().push(point);
            }
        }
        let mut shifted = Vec::new();
        for (day, points) in days {
            let offset = if day >= today {
                0.0
            } else {
                let Some(close) = regular_close(day) else {
                    continue;
                };
                let (Some(at_close), Some(close_value)) = (
                    points.iter().rev().find(|(time, _)| *time <= close),
                    recorded
                        .get(&id)
                        .and_then(|days| days.get(&day.format("%Y-%m-%d").to_string())),
                ) else {
                    continue;
                };
                close_value - at_close.1
            };
            shifted.extend(
                points.into_iter().map(
                    |(time, value)| serde_json::json!({ "time": time, "value": value + offset }),
                ),
            );
        }
        if !shifted.is_empty() {
            series.insert(id, shifted);
        }
    }
    Ok(series)
}

/// The intraday history, then any later points the current frame adds, such as now.
pub(super) fn with_intraday(
    mut current: BTreeMap<String, Vec<Value>>,
    intraday: BTreeMap<String, Vec<Value>>,
) -> BTreeMap<String, Vec<Value>> {
    for (id, mut history) in intraday {
        let last = history
            .last()
            .and_then(|point| point["time"].as_i64())
            .unwrap_or(i64::MIN);
        let later = current
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .filter(|point| point["time"].as_i64().is_some_and(|time| time > last));
        history.extend(later);
        current.insert(id, history);
    }
    current
}

#[cfg(test)]
mod tests;
