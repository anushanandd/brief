//! The portfolio's last week at five-minute resolution, built from each holding's price bars.
//!
//! A bar values today's holdings at that moment's prices. That is exact only while the
//! holdings are what they are now, so the series starts the day after the latest trade. Each
//! earlier day is then shifted to land on its recorded closing value, which carries that day's
//! cash and deposits; today needs no shift. A day without a recorded value is left out rather
//! than guessed.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::America::New_York;
use serde_json::Value;

use crate::{
    finance_contract::Snapshot,
    providers::{holding_market::PriceHistory, MarketSnapshot},
    settings::FinanceSettings,
};

/// How far back the portfolio's intraday history reaches, as the holding chart's week does.
pub(super) const WEEK_SECONDS: u64 = 604_800;

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

/// The recorded end-of-day value for a chart series: net worth, or an account's history.
fn recorded_value(snapshot: &Snapshot, id: &str, day: &str) -> Option<f64> {
    if id == "net-worth" {
        return snapshot
            .net_worth_history
            .iter()
            .find(|point| point.date == day)
            .map(|point| point.value);
    }
    snapshot
        .brokerage_performance
        .iter()
        .find(|account| account.account_id == id)?
        .points
        .iter()
        .find(|point| point.date == day)
        .map(|point| point.value)
}

/// Five-minute chart points for each series `market_chart_values` draws, from `since` to `now`.
/// Empty when any quoted holding lacks bars, since a partial portfolio would misstate its value.
pub(super) fn intraday_series(
    snapshot: &Snapshot,
    settings: &FinanceSettings,
    reference: &BTreeMap<String, MarketSnapshot>,
    bars: &BTreeMap<String, PriceHistory>,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<BTreeMap<String, Vec<Value>>, String> {
    let held: BTreeSet<String> = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.quote_eligible == Some(true))
        .map(|holding| holding.ticker.trim().to_ascii_uppercase())
        .collect();
    if held.is_empty()
        || held
            .iter()
            .any(|symbol| !bars.contains_key(symbol) || !reference.contains_key(symbol))
    {
        return Ok(BTreeMap::new());
    }
    // Shares are known only from the day after the latest trade that changed them.
    let latest_trade = snapshot
        .trades
        .iter()
        .filter(|trade| trade.units.is_some_and(|units| units != 0.0))
        .filter_map(|trade| NaiveDate::parse_from_str(&trade.date, "%Y-%m-%d").ok())
        .max();
    let since = now.timestamp() - WEEK_SECONDS as i64;
    let mut closes: BTreeMap<&str, Vec<(i64, f64)>> = BTreeMap::new();
    for symbol in &held {
        let mut series: Vec<(i64, f64)> = bars[symbol]
            .bars
            .iter()
            .filter(|bar| bar.close.is_finite() && bar.close > 0.0)
            .map(|bar| (bar.end_time, bar.close))
            .collect();
        series.sort_by_key(|(time, _)| *time);
        closes.insert(symbol, series);
    }
    let times: BTreeSet<i64> = closes
        .values()
        .flatten()
        .map(|(time, _)| *time)
        .filter(|time| (since..=now.timestamp()).contains(time))
        .filter(|time| {
            market_day(*time).is_some_and(|day| latest_trade.is_none_or(|trade| day > trade))
        })
        .collect();

    // Today's holdings at each moment's prices, before any day is shifted.
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
        let projection = super::project(snapshot, settings, &quotes, true)?;
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
                let (Some(at_close), Some(recorded)) = (
                    points.iter().rev().find(|(time, _)| *time <= close),
                    recorded_value(snapshot, &id, &day.format("%Y-%m-%d").to_string()),
                ) else {
                    continue;
                };
                recorded - at_close.1
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
