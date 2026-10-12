use super::edgar::SEC_TICKERS_MAX_AGE;
use super::quota::DAILY_REQUESTS;
use super::*;
use crate::providers::{
    sec::{SecFiling, SecItem},
    NewsScope,
};

fn story(url: &str, created_at: &str) -> MarketNewsArticle {
    MarketNewsArticle {
        headline: "Synthetic news".into(),
        summary: "Synthetic evidence".into(),
        source: "Example".into(),
        url: url.into(),
        created_at: created_at.into(),
        symbols: vec!["TEST".into()],
        scope: NewsScope::Company,
        tags: vec![],
        relevance: 0,
    }
}

#[test]
fn reported_quarters_are_reused_for_a_week_and_failures_wait_a_day() {
    let cache = NewsCache::new(None).unwrap();
    let now = 1_789_992_000;
    assert_eq!(cache.fundamentals("TEST", now).unwrap(), None);
    let quarter = Fundamentals {
        frame: "CY2026Q2".into(),
        quarter_end: "2026-06-27".into(),
        revenue: 112.0,
        revenue_growth_pct: Some(12.0),
        net_margin_pct: None,
        form: "10-Q".into(),
        filed: "2026-07-30".into(),
        url: "https://www.sec.gov/Archives/edgar/data/1/000000000026000001/".into(),
    };
    cache
        .save_fundamentals("TEST", Some(&quarter), now)
        .unwrap();
    assert_eq!(
        cache.fundamentals("TEST", now + DAY).unwrap(),
        Some(Some(quarter.clone()))
    );
    assert_eq!(cache.fundamentals("TEST", now + 8 * DAY).unwrap(), None);
    // A failed check keeps the earlier answer for a day, then asks again.
    cache.fundamentals_failure("TEST", now + 8 * DAY).unwrap();
    assert_eq!(
        cache.fundamentals("TEST", now + 8 * DAY + 60).unwrap(),
        Some(Some(quarter))
    );
    assert_eq!(
        cache.fundamentals("TEST", now + 9 * DAY + 60).unwrap(),
        None
    );
    // EDGAR having no figures is an answer too, and a first failure has nothing to keep.
    cache.save_fundamentals("FUND", None, now).unwrap();
    assert_eq!(cache.fundamentals("FUND", now).unwrap(), Some(None));
    cache.fundamentals_failure("NEW", now).unwrap();
    assert_eq!(cache.fundamentals("NEW", now + 60).unwrap(), Some(None));
}

fn filing(filed_at: &str) -> SecFiling {
    SecFiling {
        form: "8-K".into(),
        filed_at: filed_at.into(),
        url: "https://www.sec.gov/Archives/edgar/data/1/1/synthetic.htm".into(),
        items: vec![SecItem {
            code: "2.02".into(),
            label: "Earnings results".into(),
        }],
        title: String::new(),
    }
}

#[test]
fn native_news_contract_carries_tags_relevance_and_filings() {
    let mut cache = NewsCache::new(None).unwrap();
    let now = 1789992000;
    cache.begin_refresh("TEST", now).unwrap();
    let mut company = story("https://example.com/story", "2026-09-21T12:00:00+00:00");
    company.headline = "TEST Beats Estimates, Raises FY Guidance".into();
    // Analyst ratings are saved as fetched but never read back as news.
    let mut analyst = story("https://example.com/analyst", "2026-09-21T12:30:00+00:00");
    analyst.headline = "Synthetic Bank Maintains Overweight on TEST, Lowers Price Target".into();
    let mut roundup = story("https://example.com/roundup", "2026-09-21T11:00:00+00:00");
    roundup.headline = "Stock Market Today: Synthetic Index Climbs".into();
    let mut filler = story("https://example.com/filler", "2026-09-21T10:00:00+00:00");
    filler.headline = "Exploring The Competitive Space: TEST Versus Industry Peers".into();
    cache
        .save("TEST", &[roundup, analyst, company, filler], now)
        .unwrap();
    cache.failure("TEST", "Synthetic refresh failure").unwrap();
    let company = SecCompany {
        cik: 1,
        name: "Synthetic Corp".into(),
        sic: Some(3674),
        insider_filings: Vec::new(),
        filings: vec![
            filing("2026-09-20T20:05:00+00:00"),
            filing("2026-07-01T20:05:00+00:00"),
        ],
    };
    cache.save_filings("TEST", Some(&company), now).unwrap();
    let result = cache
        .result("TEST", now + NEWS_REFRESH_INTERVAL, None)
        .unwrap();
    let actual = serde_json::to_value(&result).unwrap();
    // Regenerate with BRIEF_UPDATE_CONTRACT_FIXTURE=1, then format with oxfmt.
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../src/data/fixtures/native-news.json"
            ),
            serde_json::to_string_pretty(&actual).unwrap() + "\n",
        )
        .unwrap();
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../src/data/fixtures/native-news.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
    assert!(cache
        .result("OTHER", now, None)
        .unwrap()
        .articles
        .is_empty());
}

#[test]
fn filings_survive_failed_refreshes_and_funds_have_none() {
    let mut cache = NewsCache::new(None).unwrap();
    cache
        .save_sec_tickers(&[("TEST-B".into(), 7), ("FUND".into(), 8)], 100)
        .unwrap();
    assert!(cache.sec_tickers_fresh(101).unwrap());
    assert!(!cache.sec_tickers_fresh(100 + SEC_TICKERS_MAX_AGE).unwrap());
    assert_eq!(cache.sec_cik("TEST.B").unwrap(), Some(7));
    assert_eq!(cache.sec_cik("OTHER").unwrap(), None);
    let company = SecCompany {
        cik: 7,
        name: "Synthetic".into(),
        sic: None,
        filings: vec![filing("2026-09-20T20:05:00+00:00")],
        insider_filings: Vec::new(),
    };
    cache.save_filings("TEST", Some(&company), 100).unwrap();
    cache.filings_failure("TEST", "Synthetic failure").unwrap();
    let (saved, warning) = cache.company("TEST").unwrap();
    assert_eq!(saved.unwrap().filings.len(), 1);
    assert_eq!(warning.as_deref(), Some("Synthetic failure"));
    cache.save_filings("FUND", None, 100).unwrap();
    assert!(cache.company("FUND").unwrap().0.is_none());
    cache.filings_failure("NEW", "Synthetic failure").unwrap();
    assert!(cache.company("NEW").unwrap().0.is_none());
}

#[test]
fn legacy_alpha_vantage_payloads_read_as_missing() {
    let mut cache = NewsCache::new(None).unwrap();
    cache.begin_refresh("TEST", 100).unwrap();
    cache
        .0
        .execute(
            "UPDATE news SET payload=?1, saved=100 WHERE symbol='TEST'",
            [r#"{"feed":[{"title":"Synthetic news"}]}"#],
        )
        .unwrap();
    let result = cache.result("TEST", 200, None).unwrap();
    assert!(result.saved_at.is_none());
    assert!(result.articles.is_empty());
}

#[test]
fn failed_reservation_does_not_spend_budget() {
    let mut cache = NewsCache::new(None).unwrap();
    cache.0.execute_batch("CREATE TRIGGER fail_budget BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT, 'synthetic'); END;").unwrap();
    assert!(cache.reserve(100).is_err());
    cache.0.execute_batch("DROP TRIGGER fail_budget").unwrap();
    for _ in 0..DAILY_REQUESTS {
        cache.reserve(101).unwrap();
    }
    assert!(cache.reserve(101).is_err());
}

#[test]
fn refreshes_add_to_saved_stories_and_portfolio_refreshes_wait_ten_minutes() {
    let cache = NewsCache::new(None).unwrap();
    cache
        .save(
            "TEST",
            &[story(
                "https://example.com/old",
                "2026-09-20T12:00:00+00:00",
            )],
            100,
        )
        .unwrap();
    cache
        .save(
            "TEST",
            &[
                story("https://example.com/new", "2026-09-21T12:00:00+00:00"),
                story("https://example.com/old", "2026-09-20T12:00:00+00:00"),
            ],
            200,
        )
        .unwrap();
    let urls: Vec<String> = cache
        .articles("TEST")
        .unwrap()
        .into_iter()
        .map(|article| article.url)
        .collect();
    assert_eq!(urls, ["https://example.com/new", "https://example.com/old"]);
    assert!(cache.begin_portfolio_refresh(1_000).unwrap());
    assert!(!cache.begin_portfolio_refresh(1_000 + 9 * MINUTE).unwrap());
    assert!(cache.begin_portfolio_refresh(1_000 + 10 * MINUTE).unwrap());
}

#[test]
fn whole_feed_stories_are_read_from_a_time_and_kept_three_days() {
    let cache = NewsCache::new(None).unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-24T12:00:00+00:00")
        .unwrap()
        .timestamp();
    let mut macro_story = story("https://example.com/oil", "2026-09-24T09:00:00+00:00");
    macro_story.symbols.clear();
    cache
        .save_market_news(
            &[
                story("https://example.com/stale", "2026-09-20T12:00:00+00:00"),
                story("https://example.com/monday", "2026-09-23T12:00:00+00:00"),
                macro_story,
            ],
            now,
        )
        .unwrap();
    let urls = |since: &str| -> Vec<String> {
        cache
            .market_news(since)
            .unwrap()
            .into_iter()
            .map(|story| story.url)
            .collect()
    };
    assert_eq!(
        urls("2026-09-01T00:00:00+00:00"),
        ["https://example.com/monday", "https://example.com/oil"]
    );
    assert_eq!(
        urls("2026-09-24T00:00:00+00:00"),
        ["https://example.com/oil"]
    );
}

#[test]
fn insider_trades_are_read_once_and_kept() {
    let cache = NewsCache::new(None).unwrap();
    let url = "https://www.sec.gov/Archives/edgar/data/1/1/form4.xml";
    assert_eq!(cache.insider_trades(url).unwrap(), None);
    cache.save_insider_trades(url, &[]).unwrap();
    assert_eq!(cache.insider_trades(url).unwrap(), Some(Vec::new()));
}

#[test]
fn cache_bounds_tickers_without_expiring_saved_stories() {
    let mut cache = NewsCache::new(None).unwrap();
    for index in 0..130 {
        let now = (index + 1) * DAY;
        cache.begin_refresh(&format!("TEST{index}"), now).unwrap();
        cache.save(&format!("TEST{index}"), &[], now).unwrap();
    }
    let count: usize = cache
        .0
        .query_row("SELECT count(*) FROM news", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 128);
    assert!(cache
        .result("TEST0", 131 * DAY, None)
        .unwrap()
        .saved_at
        .is_none());
    assert!(cache
        .result("TEST2", 131 * DAY, None)
        .unwrap()
        .saved_at
        .is_some());
}

#[test]
fn saved_news_and_refresh_interval_survive_reopens_without_spending_earnings_budget() {
    let path = std::env::temp_dir().join(format!("brief-news-{}.sqlite3", uuid::Uuid::new_v4()));
    let now = 1_800_000_000;
    {
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        cache.begin_refresh("TEST", now).unwrap();
        cache.save("TEST", &[], now).unwrap();
    }
    let mut cache = NewsCache::new(Some(&path)).unwrap();
    let saved = cache.result("TEST", now + 1, None).unwrap();
    assert!(saved.saved_at.is_some());
    assert!(!saved.can_refresh);
    assert!(cache
        .begin_refresh("TEST", now + 1)
        .unwrap_err()
        .contains("in a minute"));
    cache.begin_refresh("OTHER", now + 1).unwrap();
    cache.begin_refresh("TEST", now + MINUTE).unwrap();
    cache.failure("TEST", "Synthetic failure").unwrap();
    let failed = cache.result("TEST", now + MINUTE, None).unwrap();
    assert!(failed.saved_at.is_some());
    assert_eq!(failed.warning.as_deref(), Some("Synthetic failure"));
    for _ in 0..DAILY_REQUESTS {
        cache.reserve(now + MINUTE).unwrap();
    }
    assert!(cache.reserve(now + MINUTE).is_err());
    drop(cache);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn provider_quota_cooldown_survives_reopens_and_failed_saves_preserve_news() {
    let path = std::env::temp_dir().join(format!("brief-news-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut cache = NewsCache::new(Some(&path)).unwrap();
    cache.begin_refresh("TEST", 100).unwrap();
    cache.save("TEST", &[], 100).unwrap();
    cache.0.execute_batch("CREATE TRIGGER fail_news BEFORE UPDATE OF payload ON news BEGIN SELECT RAISE(ABORT, 'synthetic'); END;").unwrap();
    cache.begin_refresh("TEST", 160).unwrap();
    assert!(cache.save("TEST", &[], 160).is_err());
    cache.reserve(101).unwrap();
    cache
        .provider_limit(
            &serde_json::json!({"Information": "25 requests per day rate limit"}),
            101,
        )
        .unwrap();
    drop(cache);
    let mut cache = NewsCache::new(Some(&path)).unwrap();
    assert!(cache.reserve(102).is_err());
    cache
        .provider_limit(
            &serde_json::json!({"Information": "25 requests per day rate limit"}),
            161,
        )
        .unwrap();
    assert!(cache
        .reserve(162)
        .unwrap_err()
        .contains("daily limit reached"));
    assert!(cache
        .result("TEST", 162, None)
        .unwrap()
        .saved_at
        .unwrap()
        .contains("00:01:40"));
    assert!(cache.reserve(161 + DAY).is_ok());
    let next_incident = 161 + DAY + 1;
    cache
        .provider_limit(
            &serde_json::json!({"Information": "25 requests per day rate limit"}),
            next_incident,
        )
        .unwrap();
    assert!(cache.reserve(next_incident + 1).is_err());
    assert!(cache.reserve(next_incident + MINUTE).is_ok());
    cache
        .provider_limit(
            &serde_json::json!({"Information": "25 requests per day rate limit"}),
            next_incident + MINUTE,
        )
        .unwrap();
    assert!(cache.reserve(next_incident + MINUTE + 1).is_err());
    drop(cache);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unspecified_rate_limits_and_legacy_cooldowns_last_one_minute() {
    let mut cache = NewsCache::new(None).unwrap();
    cache
        .provider_limit(&serde_json::json!({"Note": "rate limit"}), 100)
        .unwrap();
    assert!(cache.reserve(101).unwrap_err().contains("in a minute"));
    assert!(cache.reserve(159).is_err());
    assert!(cache.reserve(160).is_ok());

    let path = std::env::temp_dir().join(format!(
        "brief-legacy-news-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE requests (at INTEGER NOT NULL);
         CREATE TABLE cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL);
         INSERT INTO requests VALUES (100);
         INSERT INTO cooldown VALUES (1, 86500);",
    )
    .unwrap();
    drop(db);
    let mut migrated = NewsCache::new(Some(&path)).unwrap();
    assert!(migrated.reserve(159).is_err());
    assert!(migrated.reserve(160).is_ok());
    drop(migrated);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn old_daily_pause_gets_one_short_retry() {
    let path = std::env::temp_dir().join(format!(
        "brief-old-daily-news-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let db = Connection::open(&path).unwrap();
    let now = 1_800_000_000;
    db.execute_batch(
        "CREATE TABLE requests (at INTEGER NOT NULL);
         CREATE TABLE cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL, period TEXT NOT NULL);",
    )
    .unwrap();
    db.execute("INSERT INTO requests VALUES (?1)", [now - MINUTE])
        .unwrap();
    db.execute("INSERT INTO cooldown VALUES (1, ?1, 'daily')", [now + DAY])
        .unwrap();
    drop(db);
    let mut cache = NewsCache::new(Some(&path)).unwrap();
    assert!(cache.reserve(now).is_ok());
    cache
        .provider_limit(
            &serde_json::json!({"Information": "25 requests per day rate limit"}),
            now,
        )
        .unwrap();
    drop(cache);
    let mut cache = NewsCache::new(Some(&path)).unwrap();
    assert!(cache.reserve(now + MINUTE).is_err());
    drop(cache);
    std::fs::remove_file(path).unwrap();
}
