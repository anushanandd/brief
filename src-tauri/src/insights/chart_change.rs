//! What each value chart says it changed over its window: from the line's value where the window
//! starts, read between the points around it, to its latest point. A line is the saved daily
//! history, each day at noon UTC unless a live point falls on that day, then the live points,
//! from the account's start date. The snapshot carries each chart's change at its saved values;
//! the market stream sends the same at live ones, so the renderer only reads them.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, NaiveTime};
use serde::Serialize;
use ts_rs::TS;

use super::calendar;
use super::changes::Window;
use crate::finance_contract::{Point, Snapshot};
use crate::numeric::round2;
use crate::settings::FinanceSettings;

/// Each window's length; all time runs from the line's first point.
pub const WINDOWS: [(Window, i64); 6] = [
    (Window::Day, 86_400),
    (Window::Week, 7 * 86_400),
    (Window::Month, 30 * 86_400),
    (Window::Quarter, 90 * 86_400),
    (Window::Year, 365 * 86_400),
    (Window::All, 0),
];

/// The chart standing for every account, without the spending cards.
pub const NET_WORTH: &str = "net-worth";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct RangeChange {
    pub change: f64,
    /// Of the size of the value where the window starts; zero when that is zero.
    pub percent: f64,
}

/// One chart's change for each window.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChartChanges {
    pub series: String,
    pub windows: BTreeMap<Window, RangeChange>,
}

/// One chart's saved history, its start, and the value it ends on.
pub struct Series {
    pub id: String,
    pub history: Vec<(NaiveDate, f64)>,
    pub start: Option<NaiveDate>,
    /// The latest value, drawn at `now` when no live point follows the history.
    pub current: f64,
}

/// A chart's line from its history and live points, as `(seconds, value)` in time order.
pub fn line(series: &Series, live: &[(i64, f64)], now: i64) -> Vec<(i64, f64)> {
    let minute = |time: i64| time.div_euclid(60) * 60;
    let live = if live.is_empty() {
        vec![(minute(now), series.current)]
    } else {
        live.to_vec()
    };
    let live_days = live
        .iter()
        .filter_map(|(time, _)| DateTime::from_timestamp(*time, 0))
        .map(|time| time.date_naive())
        .collect::<BTreeSet<_>>();
    let noon = NaiveTime::from_hms_opt(12, 0, 0).expect("noon is a time");
    let mut points = series
        .history
        .iter()
        .filter(|(date, value)| value.is_finite() && !live_days.contains(date))
        .map(|(date, value)| (date.and_time(noon).and_utc().timestamp(), *value))
        .chain(live.into_iter().filter(|(_, value)| value.is_finite()))
        .filter(|(time, _)| {
            series.start.is_none_or(|start| {
                *time
                    >= start
                        .and_hms_opt(0, 0, 0)
                        .expect("midnight")
                        .and_utc()
                        .timestamp()
            })
        })
        .collect::<Vec<_>>();
    points.sort_by_key(|(time, _)| *time);
    // One point at each time, the last given.
    points.reverse();
    points.dedup_by_key(|(time, _)| *time);
    points.reverse();
    points
}

/// The change from the line's value where a window ending `now` starts to its latest point.
pub fn range_change(points: &[(i64, f64)], window_seconds: i64, now: i64) -> RangeChange {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return RangeChange {
            change: 0.0,
            percent: 0.0,
        };
    };
    let boundary = if window_seconds > 0 {
        now - window_seconds
    } else {
        first.0
    };
    let after_index = points.iter().position(|(time, _)| *time >= boundary);
    let after = after_index.map_or(*last, |index| points[index]);
    let before = after_index
        .filter(|index| *index > 0)
        .map_or(after, |index| points[index - 1]);
    let baseline = if boundary <= first.0 {
        first.1
    } else if boundary >= last.0 {
        last.1
    } else {
        let elapsed = (after.0 - before.0) as f64;
        let progress = if elapsed > 0.0 {
            (boundary - before.0) as f64 / elapsed
        } else {
            0.0
        };
        before.1 + (after.1 - before.1) * progress
    };
    let change = last.1 - baseline;
    RangeChange {
        change: round2(change),
        percent: if baseline == 0.0 {
            0.0
        } else {
            round2(change / baseline.abs() * 100.0)
        },
    }
}

/// Each chart's change for each window ending `now`, with each chart's live points.
pub fn changes(
    series: &[Series],
    live: &BTreeMap<String, Vec<(i64, f64)>>,
    now: i64,
) -> Vec<ChartChanges> {
    series
        .iter()
        .map(|series| {
            let points = line(
                series,
                live.get(&series.id).map_or(&[][..], Vec::as_slice),
                now,
            );
            ChartChanges {
                series: series.id.clone(),
                windows: WINDOWS
                    .iter()
                    .map(|(window, seconds)| (*window, range_change(&points, *seconds, now)))
                    .collect(),
            }
        })
        .collect()
}

/// The first day a chart counts from: the start the user saved, else the first activity, for
/// the account or the earliest of the accounts it stands for.
fn start_date(snapshot: &Snapshot, settings: &FinanceSettings, id: &str) -> Option<NaiveDate> {
    let ids: Vec<&str> = match id {
        NET_WORTH => snapshot
            .accounts
            .iter()
            .filter(|account| account.id != "all")
            .map(|account| account.id.as_str())
            .collect(),
        "total" => snapshot
            .accounts
            .iter()
            .filter(|account| matches!(account.r#type.as_str(), "brokerage" | "retirement"))
            .map(|account| account.id.as_str())
            .collect(),
        id => vec![id],
    };
    ids.into_iter()
        .filter_map(|account| {
            settings
                .account_start_dates
                .get(account)
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .or_else(|| {
                    // The account's first activity, or a linked feed account's.
                    let related = |other: &str| {
                        other == account
                            || snapshot
                                .account_links
                                .get(other)
                                .is_some_and(|linked| linked == account)
                    };
                    snapshot
                        .transactions
                        .iter()
                        .filter(|transaction| {
                            transaction.account_id.as_deref().is_some_and(related)
                        })
                        .filter_map(|transaction| {
                            calendar::transaction_date(transaction.posted_date())
                        })
                        .chain(
                            snapshot
                                .trades
                                .iter()
                                .filter(|trade| related(&trade.account_id))
                                .filter_map(|trade| calendar::transaction_date(&trade.date)),
                        )
                        .min()
                })
        })
        .min()
}

fn daily(points: impl IntoIterator<Item = (String, f64)>) -> Vec<(NaiveDate, f64)> {
    points
        .into_iter()
        .filter_map(|(date, value)| {
            Some((
                NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?,
                value,
            ))
        })
        .collect()
}

/// The value charts: net worth without the spending cards (from `net_worth_history`, its
/// card-free history, and `net_worth`, its value), each brokerage and their total, and each
/// account with a balance history.
pub fn series(
    snapshot: &Snapshot,
    settings: &FinanceSettings,
    net_worth_history: &[Point],
    net_worth: f64,
) -> Vec<Series> {
    let mut series = vec![Series {
        id: NET_WORTH.into(),
        history: daily(
            net_worth_history
                .iter()
                .map(|point| (point.date.clone(), point.value)),
        ),
        start: start_date(snapshot, settings, NET_WORTH),
        current: net_worth,
    }];
    for performance in &snapshot.brokerage_performance {
        series.push(Series {
            id: performance.account_id.clone(),
            history: daily(
                performance
                    .points
                    .iter()
                    .map(|point| (point.date.clone(), point.value)),
            ),
            start: start_date(snapshot, settings, &performance.account_id),
            current: performance.current_value,
        });
    }
    for history in &snapshot.account_balance_history {
        if series
            .iter()
            .any(|existing| existing.id == history.account_id)
        {
            continue;
        }
        let Some(current) = snapshot
            .accounts
            .iter()
            .find(|account| account.id == history.account_id)
            .and_then(|account| account.value)
        else {
            continue;
        };
        series.push(Series {
            id: history.account_id.clone(),
            history: daily(
                history
                    .points
                    .iter()
                    .map(|point| (point.date.clone(), point.value)),
            ),
            start: start_date(snapshot, settings, &history.account_id),
            current,
        });
    }
    series
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn noon(value: &str) -> i64 {
        day(value)
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
    }

    #[test]
    fn a_window_starts_between_the_points_around_it() {
        // 100 at Sep 1 noon, 200 at Sep 3 noon: a day before Sep 3 noon reads 150.
        let points = [(noon("2026-09-01"), 100.0), (noon("2026-09-03"), 200.0)];
        assert_eq!(
            range_change(&points, 86_400, noon("2026-09-03")),
            RangeChange {
                change: 50.0,
                percent: 33.33,
            }
        );
        // All time runs from the first point; a window before the line starts there too.
        let all = range_change(&points, 0, noon("2026-09-03"));
        assert_eq!((all.change, all.percent), (100.0, 100.0));
        assert_eq!(
            range_change(&points, 365 * 86_400, noon("2026-09-03")).change,
            100.0
        );
        // Nothing drawn, nothing changed; a zero start has no percent.
        assert_eq!(range_change(&[], 86_400, 0).change, 0.0);
        assert_eq!(
            range_change(&[(0, 0.0), (10, 5.0)], 0, 10),
            RangeChange {
                change: 5.0,
                percent: 0.0,
            }
        );
    }

    #[test]
    fn each_window_reads_the_line_an_account_chart_draws() {
        let series = |start: Option<&str>| Series {
            id: "cash".into(),
            history: vec![
                (day("2026-08-09"), 150.0),
                (day("2026-09-01"), 100.0),
                (day("2026-09-08"), 120.0),
            ],
            start: start.map(day),
            current: 120.0,
        };
        let now = noon("2026-09-08");
        let percent = |series: &Series, live: &[(i64, f64)], window: i64| {
            range_change(&line(series, live, now), window, now).percent
        };
        assert_eq!(percent(&series(None), &[], 7 * 86_400), 20.0);
        assert_eq!(percent(&series(None), &[], 30 * 86_400), -20.0);
        assert_eq!(percent(&series(None), &[], 0), -20.0);
        // From the account's start date, all time begins there.
        assert_eq!(percent(&series(Some("2026-09-01")), &[], 0), 20.0);
        // A live point replaces its day.
        assert_eq!(percent(&series(None), &[(now, 110.0)], 7 * 86_400), 10.0);
    }

    #[test]
    fn a_line_is_saved_days_then_live_points_from_the_start_date() {
        let series = Series {
            id: "broker".into(),
            history: vec![
                (day("2026-08-30"), 90.0),
                (day("2026-09-01"), 100.0),
                (day("2026-09-02"), 110.0),
            ],
            start: Some(day("2026-09-01")),
            current: 130.0,
        };
        let live_time = noon("2026-09-02") + 3_600;
        // A live point replaces its day's saved value; the day before the start is left out.
        assert_eq!(
            line(&series, &[(live_time, 125.0)], live_time),
            [(noon("2026-09-01"), 100.0), (live_time, 125.0)]
        );
        // Without live points, the current value is drawn at the minute of `now`.
        let now = noon("2026-09-03") + 30;
        assert_eq!(
            line(&series, &[], now).last(),
            Some(&(noon("2026-09-03"), 130.0))
        );
        let windows = &changes(&[series], &BTreeMap::new(), now)[0].windows;
        assert_eq!(windows[&Window::All].change, 30.0);
    }
}
