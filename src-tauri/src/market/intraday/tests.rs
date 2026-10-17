use super::*;
use crate::{finance_contract::Point, providers::holding_market::PriceBar};

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn bar(time: &str, close: f64) -> PriceBar {
    let end = at(time).timestamp();
    PriceBar {
        time: end - 300,
        end_time: end,
        open: close,
        high: close,
        low: close,
        close,
        volume: 1.0,
        trades: 1,
        feed: "iex".into(),
    }
}

fn history(bars: Vec<PriceBar>) -> PriceHistory {
    PriceHistory {
        symbol: "TEST".into(),
        range: WEEK_SECONDS,
        resolution: 300,
        start: 0,
        end: 0,
        fetched_at: 0,
        full_verified_at: 0,
        calendar: None,
        bars,
        feeds: vec!["iex".into()],
        delay_minutes: 0,
        adjustment: "split".into(),
        cached: false,
    }
}

fn reference() -> BTreeMap<String, MarketSnapshot> {
    BTreeMap::from([(
        "TEST".into(),
        MarketSnapshot {
            symbol: "TEST".into(),
            price: 112.0,
            previous_close: 105.0,
            previous_close_as_of: Some("2026-09-10T20:00:00Z".into()),
            daily_change_pct: 0.0,
            weekly_change_pct: None,
            weekly_reference_price: None,
            weekly_reference_date: None,
            as_of: "2026-09-11T15:30:00Z".into(),
        },
    )])
}

/// Two trading days: Thursday, which has a recorded close, and Friday, today.
fn week() -> BTreeMap<String, PriceHistory> {
    BTreeMap::from([(
        "TEST".into(),
        history(vec![
            bar("2026-09-10T14:00:00Z", 100.0),
            bar("2026-09-10T19:55:00Z", 105.0),
            // After hours, past the 4 p.m. New York close.
            bar("2026-09-10T20:30:00Z", 106.0),
            bar("2026-09-11T14:00:00Z", 110.0),
            bar("2026-09-11T15:00:00Z", 112.0),
        ]),
    )])
}

fn snapshot() -> Snapshot {
    let mut snapshot: Snapshot = serde_json::from_str(include_str!(
        "../../../../src/data/fixtures/native-finance.json"
    ))
    .unwrap();
    snapshot.net_worth_history.push(Point {
        date: "2026-09-10".into(),
        value: 600.0,
    });
    snapshot
}

fn values(series: &BTreeMap<String, Vec<Value>>, id: &str) -> Vec<(i64, f64)> {
    series[id]
        .iter()
        .map(|point| {
            (
                point["time"].as_i64().unwrap(),
                point["value"].as_f64().unwrap(),
            )
        })
        .collect()
}

fn build(
    snapshot: &Snapshot,
    bars: &BTreeMap<String, PriceHistory>,
) -> BTreeMap<String, Vec<Value>> {
    intraday_series(
        snapshot,
        &FinanceSettings::default(),
        &reference(),
        bars,
        NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(),
        at("2026-09-11T15:30:00Z"),
    )
    .unwrap()
}

#[test]
fn values_today_at_each_bar_and_lands_earlier_days_on_their_recorded_close() {
    let net_worth = values(&build(&snapshot(), &week()), "net-worth");
    let value_at = |time: &str| {
        net_worth
            .iter()
            .find(|(at_time, _)| *at_time == at(time).timestamp())
            .map(|(_, value)| *value)
            .unwrap()
    };
    // Thursday's 3:55 p.m. bar is its last before the close, so it lands on the recorded 600;
    // the rest of Thursday moves with two shares from there.
    assert!((value_at("2026-09-10T19:55:00Z") - 600.0).abs() < 1e-6);
    assert!((value_at("2026-09-10T14:00:00Z") - 590.0).abs() < 1e-6);
    assert!((value_at("2026-09-10T20:30:00Z") - 602.0).abs() < 1e-6);
    // Today needs no shift: two shares rose $2 between its bars.
    assert!(
        (value_at("2026-09-11T15:00:00Z") - value_at("2026-09-11T14:00:00Z") - 4.0).abs() < 1e-6
    );
}

#[test]
fn leaves_out_days_without_a_recorded_close() {
    // The total has no recorded value for Thursday, so only today's bars remain.
    let total = values(&build(&snapshot(), &week()), "total");
    assert!(!total.is_empty());
    assert!(total
        .iter()
        .all(|(time, _)| *time >= at("2026-09-11T00:00:00Z").timestamp()));
}

#[test]
fn starts_after_the_latest_trade_that_changed_shares() {
    let mut traded = snapshot();
    traded.trades[0].date = "2026-09-10".into();
    let net_worth = values(&build(&traded, &week()), "net-worth");
    assert!(net_worth
        .iter()
        .all(|(time, _)| *time >= at("2026-09-11T00:00:00Z").timestamp()));
}

#[test]
fn draws_nothing_when_a_quoted_holding_has_no_bars() {
    assert!(build(&snapshot(), &BTreeMap::new()).is_empty());
}

#[test]
fn keeps_later_points_from_the_current_frame_after_the_history() {
    let history = BTreeMap::from([(
        "net-worth".to_string(),
        vec![serde_json::json!({ "time": 10, "value": 1.0 })],
    )]);
    let current = BTreeMap::from([(
        "net-worth".to_string(),
        vec![
            serde_json::json!({ "time": 5, "value": 0.5 }),
            serde_json::json!({ "time": 20, "value": 2.0 }),
        ],
    )]);
    let merged = with_intraday(current, history);
    let times: Vec<i64> = merged["net-worth"]
        .iter()
        .map(|point| point["time"].as_i64().unwrap())
        .collect();
    assert_eq!(times, vec![10, 20]);
}
