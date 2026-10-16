use super::*;
use crate::providers::market_stream::{publish_market_backfill, ChartFrames};

#[tokio::test]
async fn empty_market_requests_match_frontend_shape_without_credentials() {
    let providers = Providers::new().unwrap();
    let response = providers
        .market_snapshots(vec!["bad,symbol".into()])
        .await
        .unwrap();
    let value = serde_json::to_value(response).unwrap();
    assert_eq!(value["snapshots"], json!({}));
    assert_eq!(value["session"], "Market closed");
    assert_eq!(value["pollIntervalMs"], Value::Null);
    assert_eq!(
        normalize_stock_symbols(vec![
            " aapl ".into(),
            "AAPL".into(),
            "BRK.B".into(),
            "bad,symbol".into()
        ]),
        vec!["AAPL", "BRK.B"],
    );
}

#[test]
fn parses_available_alpaca_daily_closes() {
    assert_eq!(
        parse_alpaca_history(&json!({
            "bars": [
                { "t": "2026-08-28T04:00:00Z", "c": 211.82 },
                { "t": "2026-08-29T04:00:00Z", "c": null }
            ]
        })),
        vec![json!({ "date": "2026-08-28", "value": 211.82 })]
    );
}

#[test]
fn weekly_reference_is_seven_calendar_days_earlier() {
    let now = DateTime::parse_from_rfc3339("2026-09-15T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(
        weekly_reference_window(now),
        (
            "2026-09-08".into(),
            "2026-09-01".into(),
            "2026-09-09".into()
        )
    );
}

#[test]
fn weekly_references_use_the_latest_bar_on_or_before_the_target() {
    assert_eq!(
        parse_weekly_reference_closes(
            &json!({
                "bars": {
                    "AAPL": [
                        { "t": "2026-09-04T04:00:00Z", "c": 230.0 },
                        { "t": "2026-09-02T04:00:00Z", "c": 220.0 },
                        { "t": "2026-09-03T04:00:00Z", "c": 225.0 }
                    ],
                    "NBIS": [{ "t": "2026-09-01T04:00:00Z", "c": 25.0 }]
                }
            }),
            "2026-09-03"
        ),
        BTreeMap::from([
            (
                "AAPL".into(),
                WeeklyReference {
                    price: 225.0,
                    date: "2026-09-03".into()
                }
            ),
            (
                "NBIS".into(),
                WeeklyReference {
                    price: 25.0,
                    date: "2026-09-01".into()
                }
            )
        ])
    );
}

#[test]
fn parses_alpaca_market_data_and_rejects_unsafe_symbols() {
    let snapshots = parse_market_snapshots(
        &json!({
            "snapshots": { "AAPL": {
                "latestTrade": { "p": 230.0, "t": "2026-09-01T17:00:00Z" },
                "prevDailyBar": { "c": 225.0, "t": "2026-08-31T04:00:00Z" }
            }}
        }),
        false,
    );
    let snapshot = snapshots.get("AAPL").unwrap();
    assert_eq!(snapshot.price, 230.0);
    assert_eq!(
        snapshot.previous_close_as_of.as_deref(),
        Some("2026-08-31T20:00:00+00:00")
    );
    assert!((snapshot.daily_change_pct - 2.222222).abs() < 0.000001);

    assert!(stock_symbol("AAPL,MSFT").is_err());
}

#[test]
fn selects_alpaca_feeds_from_the_authoritative_market_clock_and_parses_quotes() {
    let schedule = |nasdaq_phase: &str, boats_phase: &str| {
        parse_alpaca_market_schedule(
            &json!({ "clocks": [
                {
                    "market": { "acronym": "NASDAQ" },
                    "phase": nasdaq_phase,
                    "phase_until": "2026-09-08T20:00:00Z"
                },
                {
                    "market": { "acronym": "BOATS" },
                    "phase": boats_phase,
                    "phase_until": "2026-09-08T08:00:00Z"
                }
            ]}),
            "2026-09-08T01:00:00Z".parse().unwrap(),
        )
        .unwrap()
        .session
    };
    assert_eq!(schedule("pre", "closed"), AlpacaMarketSession::PreMarket);
    assert_eq!(schedule("core", "closed"), AlpacaMarketSession::Core);
    assert_eq!(schedule("post", "closed"), AlpacaMarketSession::AfterHours);
    assert_eq!(schedule("closed", "core"), AlpacaMarketSession::Overnight);
    assert_eq!(schedule("closed", "closed"), AlpacaMarketSession::Closed);

    let holiday = parse_alpaca_market_schedule(
        &json!({ "clocks": [{
            "market": { "acronym": "NASDAQ" },
            "phase": "closed",
            "phase_until": "2026-09-08T08:00:00-04:00"
        }]}),
        "2026-09-07T14:00:00Z".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(holiday.session, AlpacaMarketSession::Closed);
    assert_eq!(
        holiday.next_transition_at.to_rfc3339(),
        "2026-09-08T12:00:00+00:00"
    );

    assert_eq!(
        quote_midpoint(&json!({ "bp": 229.0, "ap": 231.0 })),
        Some(230.0)
    );

    let snapshots = parse_market_snapshots(
        &json!({ "snapshots": { "AAPL": {
            "latestTrade": { "p": 225.0, "t": "2026-09-02T23:45:00Z" },
            "latestQuote": { "bp": 229.0, "ap": 231.0, "t": "2026-09-03T00:00:00Z" },
            "prevDailyBar": { "c": 224.0 }
        }}}),
        true,
    );
    assert_eq!(snapshots["AAPL"].price, 230.0);
    assert_eq!(snapshots["AAPL"].as_of, "2026-09-03T00:00:00Z");
}

#[test]
fn groups_minute_bars_and_preserves_price_references() {
    let seed = BTreeMap::from([(
        "AAPL".into(),
        MarketSnapshot {
            symbol: "AAPL".into(),
            price: 230.0,
            previous_close: 225.0,
            previous_close_as_of: Some("2026-09-05T20:00:00Z".into()),
            daily_change_pct: 0.0,
            weekly_change_pct: Some(4.5),
            weekly_reference_price: Some(220.0),
            weekly_reference_date: Some("2026-09-01".into()),
            as_of: "2026-09-08T01:02:00Z".into(),
        },
    )]);
    let frames = parse_market_bar_frames(
        &json!({ "bars": { "AAPL": [
            { "t": "2026-09-08T01:00:00Z", "c": 228.0 },
            { "t": "2026-09-08T01:01:00Z", "c": 229.0 }
        ]}}),
        &seed,
    );

    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].snapshots["AAPL"].price, 228.0);
    assert_eq!(frames[0].snapshots["AAPL"].previous_close, 225.0);
    assert_eq!(frames[1].as_of, "2026-09-08T01:01:00Z");

    let market = MarketSnapshots {
        snapshots: seed.clone(),
        session: "Market closed".into(),
        feed: "iex".into(),
        delay_minutes: 0,
        as_of: Some("2026-09-08T01:02:00Z".into()),
        next_transition_at: Some("2026-09-08T08:00:00Z".into()),
        poll_interval_ms: None,
        history_feed: "iex".into(),
        history_delay_minutes: 0,
    };
    let mut chart = ChartFrames::new(seed);
    let mut published = Vec::new();
    let mut publish = |tick: MarketStreamTick| {
        published.push(tick.chart_as_of.unwrap());
        Ok(())
    };
    publish_market_backfill(&market, frames.clone(), &mut chart, &mut publish).unwrap();
    publish_market_backfill(&market, frames, &mut chart, &mut publish).unwrap();
    assert_eq!(published, ["2026-09-08T01:00:00Z", "2026-09-08T01:01:00Z"]);
    assert_eq!(AlpacaMarketSession::Overnight.history_feed(), "boats");
    assert_eq!(AlpacaMarketSession::Overnight.history_delay_minutes(), 15);
}
