use super::cache::cache_connection;
use super::run::consume_shared_chart;
use super::*;

fn daily_history() -> PriceHistory {
    let now = DateTime::parse_from_rfc3339("2026-09-18T14:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let bars = (1..=40)
        .map(|day| {
            let time = (now - Duration::days(41 - day)).timestamp();
            PriceBar {
                time,
                end_time: time + 3600,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1000.0,
                trades: 100,
                feed: "sip".into(),
            }
        })
        .collect::<Vec<_>>();
    PriceHistory {
        symbol: "TEST".into(),
        range: 0,
        resolution: 86400,
        start: (now - Duration::days(41)).timestamp(),
        end: now.timestamp(),
        fetched_at: now.timestamp_millis(),
        full_verified_at: now.timestamp_millis(),
        calendar: None,
        bars,
        feeds: vec!["sip".into()],
        delay_minutes: 0,
        adjustment: "split".into(),
        cached: false,
    }
}

#[test]
fn incremental_history_reuses_prefix_and_replaces_overlap_atomically() {
    let previous = daily_history();
    let now = DateTime::from_timestamp_millis(previous.fetched_at).unwrap() + Duration::hours(1);
    let start = incremental_start(&previous, "TEST", true, now).unwrap();
    assert!(start.timestamp() > previous.start);
    let mut recent = previous.clone();
    recent.start = start.timestamp();
    recent.end += 3600;
    recent.fetched_at += 3600000;
    recent.bars.retain(|bar| bar.time >= recent.start);
    let overlap_count = recent.bars.len();
    let mut new = previous.bars.last().unwrap().clone();
    new.time = previous.end;
    new.end_time = recent.end;
    recent.bars.push(new.clone());
    let merged = reconcile_history(&previous, recent).unwrap();
    assert!(overlap_count < previous.bars.len());
    assert_eq!(merged.bars.len(), previous.bars.len() + 1);
    assert_eq!(&merged.bars[..previous.bars.len()], previous.bars);
    assert_eq!(merged.bars.last(), Some(&new));
    assert_eq!(merged.full_verified_at, previous.full_verified_at);
    assert_eq!(merged.start, previous.start);
}

#[test]
fn splits_corrections_deletions_and_missing_overlap_require_full_replacement() {
    let previous = daily_history();
    for kind in 0..4 {
        let mut recent = previous.clone();
        match kind {
            0 => {
                for bar in &mut recent.bars {
                    bar.open /= 2.0;
                    bar.high /= 2.0;
                    bar.low /= 2.0;
                    bar.close /= 2.0;
                    bar.volume *= 2.0;
                }
            }
            1 => recent.bars[3].volume += 1.0,
            2 => {
                recent.bars.remove(3);
            }
            _ => recent.bars.clear(),
        }
        assert!(reconcile_history(&previous, recent).is_none());
    }
    let mut recent = previous.clone();
    recent.start = previous.end;
    recent.bars.clear();
    assert!(reconcile_history(&previous, recent).is_none());
    assert_eq!(previous.bars.len(), 40);
}

#[test]
fn incremental_history_requires_recent_full_verification_and_identical_scope() {
    let previous = daily_history();
    let now = DateTime::from_timestamp_millis(previous.fetched_at).unwrap();
    assert!(incremental_start(&previous, "TEST", true, now).is_some());
    assert!(incremental_start(&previous, "OTHER", true, now).is_none());
    assert!(incremental_start(&previous, "TEST", false, now).is_none());
    assert!(incremental_start(&previous, "TEST", true, now + Duration::hours(11)).is_none());
    assert!(incremental_start(&previous, "TEST", true, now + Duration::days(1)).is_none());
    let mut legacy = previous.clone();
    legacy.full_verified_at = 0;
    assert!(incremental_start(&legacy, "TEST", true, now).is_none());
    legacy = previous.clone();
    legacy.range = 31536000;
    assert!(incremental_start(&legacy, "TEST", true, now).is_none());
    legacy = previous.clone();
    legacy.full_verified_at += 1;
    assert!(incremental_start(&legacy, "TEST", true, now).is_none());
}

#[test]
fn history_cadence_tracks_resolution_transition_and_closed_tail() {
    let now = Utc::now();
    let mut schedule = AlpacaMarketSchedule {
        session: AlpacaMarketSession::Core,
        next_transition_at: now + Duration::hours(8),
    };
    for (resolution, expected) in [(60, 60), (300, 300), (1800, 1800), (86400, 3600)] {
        assert_eq!(
            history_refresh_delay(resolution, &schedule, now, None).as_secs(),
            expected
        );
    }
    schedule.next_transition_at = now + Duration::seconds(30);
    assert_eq!(
        history_refresh_delay(86400, &schedule, now, None).as_secs(),
        31
    );
    schedule.session = AlpacaMarketSession::Closed;
    schedule.next_transition_at = now + Duration::hours(8);
    assert_eq!(
        history_refresh_delay(60, &schedule, now, Some(now)).as_secs(),
        1200
    );
    assert_eq!(
        history_refresh_delay(60, &schedule, now, Some(now - Duration::minutes(21))).as_secs(),
        28801
    );
}

fn sample_bar(time: &str) -> Value {
    json!({"T":"b", "S":"TEST", "t":time, "o":100.0, "h":102.0,
        "l":99.0, "c":101.0, "v":20, "n":3})
}

#[test]
fn every_intraday_range_queries_both_sessions_without_a_current_session_dependency() {
    for range in [86400, 604800, 2592000] {
        let (_, seconds) = resolution(range).unwrap();
        assert_eq!(history_feeds(seconds), ["sip", "boats"]);
    }
    for range in [31536000, 0] {
        assert_eq!(history_feeds(resolution(range).unwrap().1), ["sip"]);
    }
    assert!(resolution(123).is_err());
    for session in [
        AlpacaMarketSession::Core,
        AlpacaMarketSession::PreMarket,
        AlpacaMarketSession::AfterHours,
    ] {
        assert_eq!(stream_feed(session, true, false), "sip");
        assert_eq!(stream_feed(session, false, true), "delayed_sip");
    }
    assert_eq!(
        stream_feed(AlpacaMarketSession::Overnight, true, false),
        "overnight"
    );
    assert_eq!(
        stream_feed(AlpacaMarketSession::Overnight, false, true),
        "boats"
    );
}

#[test]
fn chart_calendar_preserves_early_closes_and_rejects_invalid_sessions() {
    let days = parse_calendar(&json!([
        {"date":"2026-12-24","open":"09:30","close":"13:00"},
        {"date":"2026-12-28","open":"09:30","close":"16:00"}
    ]))
    .unwrap();
    assert_eq!(days[0].close, 13 * 3600);
    assert_eq!(days.len(), 2);
    assert!(parse_calendar(&json!([
        {"date":"2026-12-24","open":"16:00","close":"13:00"}
    ]))
    .is_err());
    assert!(parse_calendar(&json!([
        {"date":"2026-12-24","close":"13:00"}
    ]))
    .is_err());
}

#[test]
fn daily_end_uses_provider_midnight_offset_and_calendar_early_close() {
    let calendar = parse_closes(&json!([
        {"date":"2026-03-06", "close":"16:00"},
        {"date":"2026-03-09", "close":"16:00"},
        {"date":"2026-11-27", "close":"13:00"}
    ]))
    .unwrap();
    for (start, end) in [
        ("2026-03-06T05:00:00Z", "2026-03-06T21:00:00Z"),
        ("2026-03-09T04:00:00Z", "2026-03-09T20:00:00Z"),
        ("2026-11-27T05:00:00Z", "2026-11-27T18:00:00Z"),
    ] {
        let bar = parse_bar(&sample_bar(start), "sip").unwrap();
        assert_eq!(
            bar_end(&bar, 86400, &calendar).unwrap(),
            DateTime::parse_from_rfc3339(end).unwrap().timestamp()
        );
    }
    let holiday = parse_bar(&sample_bar("2026-12-25T05:00:00Z"), "sip").unwrap();
    assert!(bar_end(&holiday, 86400, &calendar).is_err());
    assert!(parse_closes(&json!([{"date":"2026-01-01", "close":"bad"}])).is_err());
}

#[test]
fn rejects_malformed_bars_and_retains_all_ohlcv_fields() {
    let value = sample_bar("2026-09-17T14:00:00Z");
    let bar = parse_bar(&value, "sip").unwrap();
    assert_eq!(
        (bar.open, bar.high, bar.low, bar.close, bar.volume, bar.trades),
        (100.0, 102.0, 99.0, 101.0, 20.0, 3)
    );
    for (field, invalid) in [
        ("c", json!(-1)),
        ("h", json!(90)),
        ("v", json!(-1)),
        ("n", Value::Null),
        ("t", json!("invalid")),
    ] {
        let mut invalid_bar = value.clone();
        invalid_bar[field] = invalid;
        assert!(parse_bar(&invalid_bar, "boats").is_err());
    }
}

#[test]
fn cache_replacement_removes_corrections_and_failed_commit_preserves_prior_history() {
    let directory = std::env::temp_dir().join(format!("brief-price-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("prices.sqlite3");
    let mut bar = parse_bar(&sample_bar("2026-09-17T14:00:00Z"), "sip").unwrap();
    bar.end_time = bar.time + 60;
    let mut history = PriceHistory {
        symbol: "TEST".into(),
        range: 86400,
        resolution: 60,
        start: bar.time - 86400,
        end: bar.end_time,
        fetched_at: bar.end_time * 1000,
        full_verified_at: bar.end_time * 1000,
        calendar: None,
        bars: vec![bar],
        feeds: vec!["sip".into(), "boats".into()],
        delay_minutes: 15,
        adjustment: "split".into(),
        cached: false,
    };
    save_history(&path, &history).unwrap();
    assert!(cached_history(&path, "TEST", 86400).unwrap().cached);
    assert!(cached_history(&path, "OTHER", 86400).is_none());
    history.bars.clear();
    let db = cache_connection(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_cache BEFORE INSERT ON price_history BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(save_history(&path, &history).is_err());
    assert_eq!(cached_history(&path, "TEST", 86400).unwrap().bars.len(), 1);
    db.execute_batch("DROP TRIGGER fail_cache").unwrap();
    save_history(&path, &history).unwrap();
    assert!(cached_history(&path, "TEST", 86400)
        .unwrap()
        .bars
        .is_empty());
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn basic_portfolio_keeps_iex_while_entitled_prices_share_the_canonical_feed() {
    assert_eq!(
        valuation_feed(AlpacaMarketSession::Core, false, false),
        "iex"
    );
    assert_eq!(
        stream_feed(AlpacaMarketSession::Core, false, false),
        "delayed_sip"
    );
    assert_eq!(
        valuation_feed(AlpacaMarketSession::Core, true, false),
        "sip"
    );
    assert_eq!(
        valuation_feed(AlpacaMarketSession::Overnight, true, true),
        "boats"
    );
    assert_eq!(
        valuation_feed(AlpacaMarketSession::Overnight, false, false),
        "overnight"
    );
}

#[test]
fn warmed_history_cannot_survive_a_correction_or_expiry_as_verified() {
    let providers = Providers::new().unwrap();
    let mut history = daily_history();
    history.fetched_at = Utc::now().timestamp_millis();
    let epoch = providers.market_streams.epoch.load(Ordering::SeqCst);
    providers.remember_chart(&history, epoch);
    assert!(providers
        .fresh_chart(&history.symbol, history.range)
        .is_some());
    providers
        .market_streams
        .epoch
        .fetch_add(1, Ordering::SeqCst);
    assert!(providers
        .fresh_chart(&history.symbol, history.range)
        .is_none());
    providers.remember_chart(&history, epoch);
    assert!(providers
        .fresh_chart(&history.symbol, history.range)
        .is_none());
    history.fetched_at -= 86_400_000;
    providers.remember_chart(&history, epoch + 1);
    assert!(providers
        .fresh_chart(&history.symbol, history.range)
        .is_none());
}

#[tokio::test]
async fn ticker_and_range_changes_reuse_shared_feed_and_reject_old_quotes() {
    let feed = market_stream::Feed::test();
    {
        let mut state = feed.state.lock().unwrap();
        state.subscribed = ["TEST".into(), "OTHER".into()].into();
        state.quotes.insert(
            "OTHER".into(),
            json!({"T":"t", "S":"OTHER", "p":104, "t":"2026-09-17T14:00:02Z"}),
        );
    }
    let (selection_tx, selection) = tokio::sync::watch::channel(ChartSelection {
        symbol: "TEST".into(),
        range: 86400,
        request_id: "first".into(),
        warm_symbols: Vec::new(),
    });
    let (observed_tx, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let (repair, _) = tokio::sync::watch::channel((0, 0));
    let publish = |value: Value| {
        observed_tx.send(value).unwrap();
    };
    let status = |selected: &ChartSelection, status: &str| {
        publish(selected.tag(json!({"kind":"status", "status":status})));
    };
    let consumer = consume_shared_chart(
        &feed,
        selection,
        "sip",
        Utc::now() + Duration::minutes(1),
        &publish,
        &status,
        &repair,
    );
    let driver = async {
        assert_eq!(observed.recv().await.unwrap()["status"], "subscribed");
        selection_tx.send_replace(ChartSelection {
            symbol: "OTHER".into(),
            range: 86400,
            request_id: "second".into(),
            warm_symbols: Vec::new(),
        });
        assert_eq!(observed.recv().await.unwrap()["status"], "subscribed");
        let quote = observed.recv().await.unwrap();
        assert_eq!(quote["requestId"], "second");
        assert_eq!(quote["price"].as_f64(), Some(104.0));
        feed.events
            .send(vec![
                json!({"T":"t", "S":"TEST", "p":999, "t":"2026-09-17T14:00:03Z"}),
                sample_bar("2026-09-17T14:00:00Z"),
                json!({"T":"x", "S":"OTHER"}),
            ])
            .unwrap();
        assert_eq!(observed.recv().await.unwrap()["kind"], "invalidate");
        selection_tx.send_replace(ChartSelection {
            symbol: "OTHER".into(),
            range: 604800,
            request_id: "third".into(),
            warm_symbols: Vec::new(),
        });
        assert_eq!(observed.recv().await.unwrap()["requestId"], "third");
        assert_eq!(observed.recv().await.unwrap()["requestId"], "third");
        assert!(observed.try_recv().is_err());
        drop(selection_tx);
    };
    tokio::time::timeout(StdDuration::from_secs(5), async {
        let (_, result) = tokio::join!(driver, consumer);
        result.unwrap();
    })
    .await
    .unwrap();
    assert_eq!(*repair.borrow(), (2, 1));
    // Chart selection/cleanup leaves the shared transport and all symbols intact.
    assert_eq!(feed.state.lock().unwrap().subscribed.len(), 2);
}
