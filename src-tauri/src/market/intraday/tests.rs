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

/// A half-hour bar, as the month has.
fn half_hour(time: &str, close: f64) -> PriceBar {
    let end = at(time).timestamp();
    PriceBar {
        time: end - 1800,
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
fn week() -> Bars {
    Bars {
        week: BTreeMap::from([(
            "TEST".into(),
            history(vec![
                bar("2026-09-10T14:00:00Z", 100.0),
                bar("2026-09-10T19:55:00Z", 105.0),
                // After hours, past the 4 p.m. New York close.
                bar("2026-09-10T20:30:00Z", 106.0),
                bar("2026-09-11T14:00:00Z", 110.0),
                bar("2026-09-11T15:00:00Z", 112.0),
            ]),
        )]),
        month: None,
    }
}

/// The week, with the month's half hours before it: Wednesday, September 2, between the
/// fixture's buy of three shares on the 1st and its sale of one on the 8th.
fn month() -> Bars {
    let mut bars = week();
    bars.month = Some(BTreeMap::from([(
        "TEST".into(),
        history(vec![
            half_hour("2026-09-02T14:00:00Z", 90.0),
            half_hour("2026-09-02T20:00:00Z", 94.0),
            // Within the week: the five-minute bars stand there.
            half_hour("2026-09-10T14:00:00Z", 99.0),
        ]),
    )]));
    bars
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

fn build_groups(
    snapshot: &Snapshot,
    groups: &[GroupHistory],
    bars: &Bars,
) -> BTreeMap<String, Vec<Value>> {
    intraday_series(
        snapshot,
        &FinanceSettings::default(),
        &reference(),
        bars,
        &recorded_closes(snapshot, &snapshot.net_worth_history, groups),
        NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(),
        at("2026-09-11T15:30:00Z"),
    )
    .unwrap()
}

fn build_with(
    snapshot: &Snapshot,
    settings: &FinanceSettings,
    accounts: &[Point],
    bars: &Bars,
) -> BTreeMap<String, Vec<Value>> {
    intraday_series(
        snapshot,
        settings,
        &reference(),
        bars,
        &recorded_closes(snapshot, accounts, &[]),
        NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(),
        at("2026-09-11T15:30:00Z"),
    )
    .unwrap()
}

fn build(snapshot: &Snapshot, bars: &Bars) -> BTreeMap<String, Vec<Value>> {
    build_with(
        snapshot,
        &FinanceSettings::default(),
        &snapshot.net_worth_history,
        bars,
    )
}

#[test]
fn the_accounts_line_gets_the_week_with_non_market_balances_held() {
    // The card is set apart, so the accounts' line is net worth without its -$300, and its
    // recorded Thursday close is $900 (net worth's $600 less the card's then balance).
    let snapshot = snapshot();
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:card".into()),
        ..FinanceSettings::default()
    };
    let accounts = [Point {
        date: "2026-09-10".into(),
        value: 900.0,
    }];
    let series = build_with(&snapshot, &settings, &accounts, &week());
    let line = values(&series, "accounts");
    let value_at = |line: &[(i64, f64)], time: &str| {
        line.iter()
            .find(|(at_time, _)| *at_time == at(time).timestamp())
            .map(|(_, value)| *value)
    };
    // Earlier days stay in, landing on the line's own recorded close, with only the holdings
    // moving within the day: two shares, $5 lower in the morning.
    assert!((value_at(&line, "2026-09-10T19:55:00Z").unwrap() - 900.0).abs() < 1e-6);
    assert!((value_at(&line, "2026-09-10T14:00:00Z").unwrap() - 890.0).abs() < 1e-6);
    // Today the card is held at its known balance: the line is net worth less it at every bar.
    let net_worth = values(&series, "net-worth");
    for time in ["2026-09-11T14:00:00Z", "2026-09-11T15:00:00Z"] {
        let difference = value_at(&line, time).unwrap() - value_at(&net_worth, time).unwrap();
        assert!((difference - 300.0).abs() < 1e-6);
    }
    // The same five-minute moments as the brokerage's own line.
    let brokerage = values(&series, "snaptrade:brokerage");
    let times = |line: &[(i64, f64)]| line.iter().map(|(time, _)| *time).collect::<Vec<_>>();
    assert_eq!(
        times(&line)
            .into_iter()
            .filter(|time| times(&brokerage).contains(time))
            .count(),
        times(&brokerage).len()
    );
}

#[test]
fn a_group_holding_securities_gets_the_week_landing_on_its_own_close() {
    // The brokerage group's recorded Thursday close is $400; the retirement group holds no
    // live-priced security, and cash none at all, so neither moves with the market.
    let group = |kind: &str, ids: &[&str], value: f64, close: f64| GroupHistory {
        account_type: kind.into(),
        account_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
        value: Some(value),
        points: vec![Point {
            date: "2026-09-10".into(),
            value: close,
        }],
    };
    let groups = [
        group("brokerage", &["snaptrade:brokerage"], 250.0, 400.0),
        group("retirement", &["plaid:hsa"], 500.0, 500.0),
        group("cash", &["plaid:cash"], 100.11, 100.11),
    ];
    let series = build_groups(&snapshot(), &groups, &week());
    assert!(!series.contains_key("group:retirement"));
    assert!(!series.contains_key("group:cash"));
    let line = values(&series, "group:brokerage");
    let value_at = |line: &[(i64, f64)], time: &str| {
        line.iter()
            .find(|(at_time, _)| *at_time == at(time).timestamp())
            .map(|(_, value)| *value)
            .unwrap()
    };
    assert!((value_at(&line, "2026-09-10T19:55:00Z") - 400.0).abs() < 1e-6);
    assert!((value_at(&line, "2026-09-10T14:00:00Z") - 390.0).abs() < 1e-6);
    // Today the group is its one brokerage, bar for bar.
    let brokerage = values(&series, "snaptrade:brokerage");
    for time in ["2026-09-11T14:00:00Z", "2026-09-11T15:00:00Z"] {
        assert!((value_at(&line, time) - value_at(&brokerage, time)).abs() < 1e-6);
    }
}

#[test]
fn the_accounts_line_leaves_out_days_its_own_history_lacks() {
    // Net worth has Thursday, but the accounts' history does not: the line keeps only today
    // rather than borrow another line's close.
    let line = values(
        &build_with(&snapshot(), &FinanceSettings::default(), &[], &week()),
        "accounts",
    );
    assert!(!line.is_empty());
    assert!(line
        .iter()
        .all(|(time, _)| *time >= at("2026-09-11T00:00:00Z").timestamp()));
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
fn leaves_out_a_day_whose_shares_changed_within_it() {
    let mut traded = snapshot();
    traded.trades[0].date = "2026-09-10".into();
    let net_worth = values(&build(&traded, &week()), "net-worth");
    assert!(!net_worth.is_empty());
    assert!(net_worth
        .iter()
        .all(|(time, _)| *time >= at("2026-09-11T00:00:00Z").timestamp()));
}

/// The fixture with a recorded close for Wednesday, September 2.
fn with_september_2() -> Snapshot {
    let mut snapshot = snapshot();
    snapshot.net_worth_history.push(Point {
        date: "2026-09-02".into(),
        value: 500.0,
    });
    snapshot
        .net_worth_history
        .sort_by(|a, b| a.date.cmp(&b.date));
    snapshot
}

fn value_at(line: &[(i64, f64)], time: &str) -> Option<f64> {
    line.iter()
        .find(|(at_time, _)| *at_time == at(time).timestamp())
        .map(|(_, value)| *value)
}

#[test]
fn the_month_draws_half_hours_before_the_week_at_the_shares_then_held() {
    let net_worth = values(&build(&with_september_2(), &month()), "net-worth");
    // September 2 held three shares, before the sale of one on the 8th: it lands on its
    // recorded close and is $12 lower in the morning, when the price was $4 lower.
    assert!((value_at(&net_worth, "2026-09-02T20:00:00Z").unwrap() - 500.0).abs() < 1e-6);
    assert!((value_at(&net_worth, "2026-09-02T14:00:00Z").unwrap() - 488.0).abs() < 1e-6);
    // Within the week, the five-minute bars stand: Thursday morning is priced at $100, not
    // the half hour's $99.
    assert!((value_at(&net_worth, "2026-09-10T14:00:00Z").unwrap() - 590.0).abs() < 1e-6);
    // The sale's own day is left out, and without the month the line starts with the week.
    assert!(net_worth
        .iter()
        .all(|(time, _)| { market_day(*time) != NaiveDate::from_ymd_opt(2026, 9, 8) }));
    let week_only = values(&build(&with_september_2(), &week()), "net-worth");
    assert!(value_at(&week_only, "2026-09-02T20:00:00Z").is_none());
}

#[test]
fn days_holding_a_position_since_sold_off_are_left_out() {
    // A position sold on the 9th is no longer held, so its earlier days can't be valued.
    let mut traded = with_september_2();
    let mut sale = traded.trades[0].clone();
    sale.date = "2026-09-09".into();
    sale.ticker = Some("GONE".into());
    traded.trades.push(sale);
    let net_worth = values(&build(&traded, &month()), "net-worth");
    assert!(value_at(&net_worth, "2026-09-02T20:00:00Z").is_none());
    assert!(value_at(&net_worth, "2026-09-10T19:55:00Z").is_some());
}

#[test]
fn days_before_a_share_change_that_cannot_be_undone_are_left_out() {
    let mut traded = with_september_2();
    let mut transfer = traded.trades[0].clone();
    transfer.date = "2026-09-04".into();
    transfer.r#type = "TRANSFER".into();
    traded.trades.push(transfer);
    let net_worth = values(&build(&traded, &month()), "net-worth");
    assert!(value_at(&net_worth, "2026-09-02T20:00:00Z").is_none());
    assert!(value_at(&net_worth, "2026-09-10T19:55:00Z").is_some());
}

#[test]
fn draws_nothing_when_a_quoted_holding_has_no_bars() {
    let bars = Bars {
        week: BTreeMap::new(),
        month: month().month,
    };
    assert!(build(&snapshot(), &bars).is_empty());
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

#[test]
fn a_first_frame_draws_the_saved_week_instead_of_two_points() {
    let snapshot = snapshot();
    let view = crate::insights::View {
        snapshot: snapshot.clone(),
        insights: None,
        account_names: BTreeMap::new(),
        spending_account_id: None,
    };
    let market = crate::providers::MarketSnapshots {
        snapshots: reference(),
        session: "regular".into(),
        feed: "iex".into(),
        delay_minutes: 0,
        as_of: Some("2026-09-11T15:30:00Z".into()),
        next_transition_at: None,
        poll_interval_ms: None,
        history_feed: "iex".into(),
        history_delay_minutes: 0,
    };
    let bars = week();
    let first = |saved| {
        let frame = super::super::frame(
            &snapshot,
            &FinanceSettings::default(),
            &market,
            saved,
            NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(),
            at("2026-09-11T15:30:00Z"),
        )
        .unwrap();
        serde_json::from_value::<BTreeMap<String, Vec<Value>>>(frame["chartSeries"].clone())
            .unwrap()
    };
    // Without saved bars, the previous close and now.
    assert_eq!(values(&first(None), "net-worth").len(), 2);
    // With them, the week the history message would bring, and now after it.
    let week = values(&build(&snapshot, &bars), "net-worth");
    let drawn = values(&first(Some((&view, &bars))), "net-worth");
    assert!(week.len() > 2);
    assert_eq!(drawn[..week.len()], week[..]);
}
