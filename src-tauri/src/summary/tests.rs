use super::ledger::activity;
use super::*;
use crate::finance_contract::Transaction;

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

/// A month of a synthetic portfolio: two holdings and cash, a deposit, a sale, a dividend.
fn synthetic_report() -> PeriodReport {
    let points: Vec<Value> = [
        ("2026-08-31", 10_000.0, 10_000.0, 10_000.0),
        ("2026-09-15", 11_250.0, 11_000.0, 11_120.0),
        ("2026-09-30", 11_480.5, 11_000.0, 11_190.0),
    ]
    .iter()
    .map(|(date, value, deposits, benchmark)| {
        json!({ "date": date, "value": value, "netDeposits": deposits, "sp500": benchmark })
    })
    .collect();
    let snapshot = json!({
        "updatedAt": "2026-09-30T21:00:00Z", "netWorth": 11_480.5, "accounts": [],
        "netWorthHistory": [], "spending": { "monthTotal": 0, "categories": [] },
        "brokeragePerformance": [
            { "accountId": "total", "name": "Combined", "institution": "1 account",
              "currentValue": 11_480.5, "performanceMethod": "time-weighted", "points": points },
            { "accountId": "brokerage", "name": "Brokerage", "institution": "Synthetic",
              "currentValue": 11_480.5, "points": [] },
        ],
        "holdings": [
            { "ticker": "TEST", "name": "Test Corp", "accountId": "brokerage", "shares": 50.0,
              "price": 130.0, "value": 6_500.0, "costBasis": null, "dailyChangePct": null,
              "totalChangePct": null, "color": "#000000" },
            { "ticker": "OTHER", "name": "Other Corp", "accountId": "brokerage", "shares": 40.0,
              "price": 95.0, "value": 3_800.0, "costBasis": null, "dailyChangePct": null,
              "totalChangePct": null, "color": "#000000" },
        ],
        "trades": [
            trade("s1", "SELL", "2026-09-20", "OTHER", 950.0, Some(120.0)),
        ],
        "transactions": [
            dividend("d1", "2026-09-15", "Dividend · TEST", 31.0, "brokerage"),
        ],
    });
    let mut snapshot = snapshot;
    // The sale's 10 shares came out of 50 held at the start.
    snapshot["trades"][0]["units"] = json!(10.0);
    snapshot["transactions"][0]["classification"] =
        json!({ "mark": "dividend", "kind": "dividend", "spending": false, "zelle": false });
    snapshot["transactions"][0]["category"] = json!("Dividend");
    snapshot["transactions"][0]["account"] = json!("Brokerage");
    snapshot["transactions"][0]["description"] = Value::Null;
    let closes = BTreeMap::from([
        (
            "TEST".to_string(),
            vec![
                (day("2026-08-31"), 120.0),
                (day("2026-09-15"), 125.0),
                (day("2026-09-30"), 130.0),
            ],
        ),
        (
            "OTHER".to_string(),
            vec![
                (day("2026-08-31"), 100.0),
                (day("2026-09-15"), 98.0),
                (day("2026-09-30"), 95.0),
            ],
        ),
    ]);
    reconcile::report(&typed(&snapshot), &closes, day("2026-08-31"), None)
}

/// Sessions after `start` through `end`, asked for on `end`.
fn period(start: NaiveDate, end: NaiveDate) -> Period {
    Period {
        start,
        end,
        through: end,
    }
}

fn trade(id: &str, kind: &str, date: &str, ticker: &str, amount: f64, gain: Option<f64>) -> Trade {
    serde_json::from_value(json!({
        "id": id, "type": kind, "date": date, "amount": amount, "account": "Synthetic",
        "accountId": "brokerage", "ticker": ticker, "units": 5.0, "price": 10.0,
        "estimatedRealizedGain": gain
    }))
    .unwrap()
}

fn dividend(id: &str, date: &str, merchant: &str, amount: f64, account: &str) -> Transaction {
    serde_json::from_value(json!({
        "id": id, "merchant": merchant, "category": "Dividend", "account": "Brokerage",
        "date": date, "amount": amount, "accountId": account, "pending": false,
        "classification": { "mark": "dividend", "kind": "dividend", "spending": false, "zelle": false }
    }))
    .unwrap()
}

/// A JSON snapshot as the typed contract reads it.
fn typed(snapshot: &Value) -> Snapshot {
    serde_json::from_value(snapshot.clone()).unwrap()
}

fn article(headline: &str, at: &str, tags: Vec<NewsTag>, url: &str) -> MarketNewsArticle {
    MarketNewsArticle {
        headline: headline.into(),
        summary: "Synthetic evidence".into(),
        source: "Example".into(),
        url: url.into(),
        created_at: at.into(),
        symbols: vec!["TEST".into()],
        scope: NewsScope::Company,
        tags,
        relevance: 90,
    }
}

fn filing(code: &str, label: &str, at: &str, url: &str) -> sec::SecFiling {
    sec::SecFiling {
        form: "8-K".into(),
        filed_at: at.into(),
        url: url.into(),
        items: vec![sec::SecItem {
            code: code.into(),
            label: label.into(),
        }],
        title: String::new(),
    }
}

#[test]
fn native_summary_contract_matches_the_renderer_fixture() {
    let mut summary = InvestmentSummary {
        brief: None,
        start: day("2026-08-31"),
        end: day("2026-09-30"),
        session: SessionClock::at("2026-09-30T21:00:00Z".parse().unwrap()),
        prices_as_of: Some("2026-09-30T19:59:00+00:00".into()),
        generated_at: "2026-09-30T21:05:00+00:00".into(),
        ticker: Some("TEST".into()),
        report: Some(synthetic_report()),
        notable: vec![NotableMove {
            ticker: "SMALL".into(),
            change_pct: Some(18.2),
            layers: layers_of(2.0, 1.0, 16.0, 5.2, "XLV"),
        }],
        period_move: Some(PeriodMove {
            change_pct: 6.1,
            market: Some(Benchmark {
                symbol: "SPY".into(),
                name: "S&P 500".into(),
                change_pct: 2.0,
            }),
            sector: Some(Benchmark {
                symbol: "XLK".into(),
                name: "Technology".into(),
                change_pct: 4.5,
            }),
            layers: Some(layers_of(2.0, 2.5, 1.6, 1.3, "XLK")),
            kind: Some("ownMove"),
        }),
        period_layers: Some(sessions::PeriodLayers {
            sessions: 21,
            missing: 1,
            market: 120.5,
            groups: vec![sessions::PeriodGroup {
                fund: "XLK".into(),
                name: Some("Technology".into()),
                change: 80.25,
                members: vec!["TEST".into()],
                flagged: 3,
            }],
            own: vec![sessions::PeriodOwn {
                ticker: "TEST".into(),
                change: 41.0,
                own_pct: 1.6,
                z: Some(1.2),
                fit: None,
            }],
            unmodeled: 0.0,
        }),
        activity: Activity {
            buys: 1,
            sells: 1,
            trades: vec![
                TradeFact {
                    date: "2026-09-12".into(),
                    side: "sell",
                    ticker: "TEST".into(),
                    units: Some(5.0),
                    amount: 70.0,
                    realized_gain: Some(-2.25),
                },
                TradeFact {
                    date: "2026-09-02".into(),
                    side: "buy",
                    ticker: "TEST".into(),
                    units: None,
                    amount: 100.0,
                    realized_gain: None,
                },
            ],
            realized: Some(-2.25),
            dividends: 3.1,
            dividend_payments: 1,
        },
        events: logged(
            &[holding(
                "TEST",
                vec![article(
                    "TEST names a CFO",
                    "2026-09-05T12:00:00Z",
                    vec![NewsTag::Leadership],
                    "https://example.com/cfo",
                )],
                vec![filing(
                    "2.02",
                    "Earnings results",
                    "2026-09-20T20:05:00+00:00",
                    "https://www.sec.gov/Archives/edgar/data/1/0000000001/synthetic.htm",
                )],
                Some(own_with("2026-01-01", 280, &[("2026-09-21", 4.2)])),
            )],
            period(day("2026-08-31"), day("2026-09-30")),
            3,
        ),
        calendar: vec![MarketDay {
            session: day("2026-09-16"),
            kind: "fomc".into(),
            title: "Federal Reserve rate decision".into(),
            at: Some("2026-09-16T18:00:00+00:00".into()),
            url: Some("https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm".into()),
        }],
        ahead: vec![
            Upcoming {
                date: day("2026-10-09"),
                kind: "cashDividend".into(),
                ticker: Some("TEST".into()),
                title: "TEST goes ex-dividend".into(),
                status: eventlog::Status::Confirmed,
                amount: Some(12.5),
                url: None,
            },
            Upcoming {
                date: day("2026-10-28"),
                kind: "fomc".into(),
                ticker: None,
                title: "Federal Reserve rate decision".into(),
                status: eventlog::Status::Scheduled,
                amount: None,
                url: Some("https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm".into()),
            },
        ],
        quiet: Some(Quiet {
            portfolio: false,
            z: Some(-1.8),
            holdings: vec!["OTHER".into()],
        }),
        fundamentals: Some(Fundamentals {
            frame: "CY2026Q2".into(),
            quarter_end: "2026-06-27".into(),
            revenue: 112_000_000.0,
            revenue_growth_pct: Some(12.0),
            net_margin_pct: None,
            form: "10-Q".into(),
            filed: "2026-07-30".into(),
            url: "https://www.sec.gov/Archives/edgar/data/1/000000000026000001/".into(),
        }),
        insiders: vec![InsiderTrade {
            date: "2026-09-14".into(),
            filed_at: "2026-09-16T20:00:00+00:00".into(),
            name: "Doe Jane A".into(),
            role: "Chief Executive Officer".into(),
            side: "sell".into(),
            shares: 1200.0,
            price: Some(110.5),
            value: Some(132600.0),
            planned: true,
            url:
                "https://www.sec.gov/Archives/edgar/data/1/000000000026000002/xslF345X05/form4.xml"
                    .into(),
        }],
        today: Some(TodayMove {
            as_of: Some("2026-09-30T19:59:00+00:00".into()),
            change_pct: Some(2.4),
            change: None,
            market: Some(Benchmark {
                symbol: "SPY".into(),
                name: "S&P 500".into(),
                change_pct: 0.3,
            }),
            sector: Some(Benchmark {
                symbol: "XLK".into(),
                name: "Technology".into(),
                change_pct: 0.5,
            }),
            kind: Some("ownMove"),
            positions: vec![TodayPosition {
                ticker: "TEST".into(),
                change_pct: 2.4,
                change: 48.5,
                kind: Some("ownMove"),
            }],
            sectors: vec![SectorToday {
                name: "Technology".into(),
                weight_pct: 66.6,
                change_pct: -2.83,
            }],
        }),
        coverage: vec![Coverage {
            ticker: "TEST".into(),
            stories: 4,
            top_relevance: Some(90),
        }],
        unavailable: vec![
            Unavailable {
                part: "fundamentals",
                symbols: Vec::new(),
            },
            Unavailable {
                part: "history",
                symbols: vec!["ZZZ".into()],
            },
        ],
    };
    let today = summary.today.as_ref().unwrap();
    summary.brief = Some(Brief {
        lines: brief::lines(today, Some("TEST"), Some(48.5), &summary.session),
        headlines: vec![headlines::Headline {
            tickers: vec!["TEST".into()],
            title: "Test Corp jumps after naming a new finance chief".into(),
            publisher: "CNBC".into(),
            url: "https://example.com/cfo".into(),
            published_at: "2026-09-30T13:00:00Z".into(),
        }],
    });
    let actual = serde_json::to_value(&summary).unwrap();
    // Regenerate with BRIEF_UPDATE_CONTRACT_FIXTURE=1, then format with oxfmt.
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../src/data/fixtures/native-summary.json"
            ),
            serde_json::to_string_pretty(&actual).unwrap() + "\n",
        )
        .unwrap();
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../src/data/fixtures/native-summary.json"
    ))
    .unwrap();
    assert_eq!(actual, fixture);
}

#[test]
fn activity_counts_the_period_names_the_newest_and_never_guesses_realized_gains() {
    let (start, end) = (day("2026-08-31"), day("2026-09-30"));
    let trades = vec![
        trade("old", "BUY", "2026-08-31", "TEST", -100.0, None),
        trade("a", "BUY", "2026-09-02", "TEST", -100.0, None),
        trade("b", "SELL", "2026-09-10", "OTHER", 60.0, Some(12.5)),
        trade(
            "c",
            "SELL",
            "2026-09-12T15:00:00Z",
            "TEST",
            70.0,
            Some(-2.25),
        ),
        trade("d", "BUY", "2026-09-20", "OTHER", -10.0, None),
        trade(
            "reinvest",
            "DIVIDEND REINVESTMENT BUY",
            "2026-09-21",
            "TEST",
            -1.0,
            None,
        ),
        trade("future", "BUY", "2026-10-01", "TEST", -1.0, None),
    ];
    let usd = BTreeSet::from(["brokerage".to_string()]);
    let transactions = vec![
        dividend("d1", "2026-09-15", "Dividend · TEST", 3.1, "brokerage"),
        dividend("d2", "2026-09-16", "Dividend · OTHER", 1.2, "brokerage"),
        dividend("foreign", "2026-09-16", "Dividend · TEST", 9.0, "foreign"),
        dividend("before", "2026-08-30", "Dividend · TEST", 9.0, "brokerage"),
    ];
    let all = activity(&trades, &transactions, &usd, &period(start, end), None);
    assert_eq!((all.buys, all.sells), (2, 2));
    assert_eq!(
        all.trades
            .iter()
            .map(|trade| trade.date.as_str())
            .collect::<Vec<_>>(),
        ["2026-09-20", "2026-09-12", "2026-09-10"]
    );
    assert_eq!(all.trades[1].realized_gain, Some(-2.25));
    assert_eq!(all.realized, Some(10.25));
    assert_eq!((all.dividends, all.dividend_payments), (4.3, 2));

    let one = activity(
        &trades,
        &transactions,
        &usd,
        &period(start, end),
        Some("TEST"),
    );
    assert_eq!((one.buys, one.sells), (1, 1));
    assert_eq!((one.dividends, one.dividend_payments), (3.1, 1));

    // A sale without a known cost leaves the total unavailable, never partial.
    let mut unknown = trades.clone();
    unknown[2].estimated_realized_gain = None;
    assert_eq!(
        activity(&unknown, &[], &usd, &period(start, end), None).realized,
        None
    );
    assert_eq!(
        activity(&[], &[], &usd, &period(start, end), None),
        Activity::default()
    );
}

/// A holding's own layer by session at a 0.5% typical day: `bumps` on given sessions, a small
/// wobble on the others, over the sessions from `start` for `days` calendar days.
fn own_with(start: &str, days: i64, bumps: &[(&str, f64)]) -> BTreeMap<NaiveDate, (f64, f64)> {
    let first = day(start);
    (0..days)
        .map(|offset| first + Duration::days(offset))
        .filter(|date| crate::session::is_trading_day(*date))
        .enumerate()
        .map(|(index, date)| {
            let bump = bumps
                .iter()
                .find(|(when, _)| day(when) == date)
                .map(|(_, size)| *size);
            (date, (bump.unwrap_or([0.3, -0.3][index % 2]), 0.5))
        })
        .collect()
}

/// A holding's period layers with `own_pct` over a typical period of `typical_pct`.
fn layers_of(
    market_pct: f64,
    group_pct: f64,
    own_pct: f64,
    typical_pct: f64,
    fund: &str,
) -> HoldingPeriod {
    HoldingPeriod {
        market_pct,
        group_pct,
        own_pct,
        own_z: Some((own_pct / typical_pct * 10.0).round() / 10.0),
        typical_pct: Some(typical_pct),
        fit: Some(layers::FitMeta {
            market_beta: 1.2,
            fund: Some(fund.into()),
            group_beta: Some(0.9),
            r_squared: 0.48,
            window: 250,
            shrunk: false,
        }),
        sessions: 21,
    }
}

/// A session record per session from each holding's own series, as the layers would make.
fn records_of(evidence: &[Evidence]) -> Vec<SessionRecord> {
    let days: BTreeSet<NaiveDate> = evidence
        .iter()
        .flat_map(|holding| holding.own.keys().copied())
        .collect();
    days.into_iter()
        .map(|session| SessionRecord {
            session,
            method: sessions::METHOD,
            computed_at: String::new(),
            market_pct: 0.0,
            market_z: 0.0,
            holdings: evidence
                .iter()
                .filter_map(|holding| {
                    let (own, sigma) = *holding.own.get(&session)?;
                    Some(sessions::HoldingSession {
                        ticker: holding.ticker.clone(),
                        change_pct: own,
                        market_pct: 0.0,
                        group_pct: 0.0,
                        own_pct: own,
                        own_z: Some(own / sigma),
                        sigma: Some(sigma),
                        change: None,
                        market: None,
                        group: None,
                        own: None,
                        fit: None,
                    })
                })
                .collect(),
            groups: Vec::new(),
            calibration: sessions::Calibration::default(),
            quiet: None,
        })
        .collect()
}

/// The period's listed events for `evidence`, built through the event log as a summary does.
fn logged(evidence: &[Evidence], window: Period, per_symbol: usize) -> Vec<ListedEvent> {
    let held: BTreeSet<String> = evidence
        .iter()
        .map(|holding| holding.ticker.clone())
        .collect();
    let (closes, volumes, shares) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let accounts = BTreeSet::new();
    let sources = eventlog::Sources {
        trades: &[],
        transactions: &[],
        accounts: &accounts,
        corporate_actions: &[],
        evidence,
        market_news: &[],
        closes: &closes,
        volumes: &volumes,
        shares: &shares,
        held: &held,
        ingested_at: "2026-10-06T21:00:00+00:00",
    };
    let records = records_of(evidence);
    let mut events = Vec::new();
    let mut session = Session::on_or_after(window.start.succ_opt().unwrap());
    while session.date <= window.through {
        let record = records.iter().find(|record| record.session == session.date);
        events.extend(eventlog::session_events(session.date, record, &sources));
        session = session.next();
    }
    let links = eventlog::link(&events);
    eventlog::listed(&events, &links, 0.0, per_symbol, LISTED_EVENTS)
}

fn titles(events: &[ListedEvent]) -> Vec<&str> {
    events.iter().map(|event| event.title.as_str()).collect()
}

fn holding(
    ticker: &str,
    articles: Vec<MarketNewsArticle>,
    filings: Vec<sec::SecFiling>,
    own: Option<BTreeMap<NaiveDate, (f64, f64)>>,
) -> Evidence {
    Evidence {
        ticker: ticker.into(),
        articles,
        filings,
        own: own.unwrap_or_default(),
    }
}

#[test]
fn events_rank_by_consequence_then_recency_and_show_a_story_once() {
    let (start, end) = (day("2026-08-31"), day("2026-09-30"));
    let evidence = vec![holding(
        "TEST",
        vec![
            article(
                "TEST beats estimates",
                "2026-09-20T21:00:00Z",
                vec![NewsTag::Earnings],
                "https://example.com/beat",
            ),
            article(
                "TEST opens a store",
                "2026-09-25T12:00:00Z",
                vec![],
                "https://example.com/store",
            ),
            article(
                "TEST names a CFO",
                "2026-09-05T12:00:00Z",
                vec![NewsTag::Leadership],
                "https://example.com/cfo",
            ),
            article(
                "Old TEST deal",
                "2026-08-20T12:00:00Z",
                vec![NewsTag::Deal],
                "https://example.com/old",
            ),
        ],
        vec![
            filing(
                "2.02",
                "Earnings results",
                "2026-09-20T20:05:00+00:00",
                "https://www.sec.gov/a",
            ),
            filing(
                "4.02",
                "Restatement",
                "2026-09-01T20:05:00+00:00",
                "https://www.sec.gov/b",
            ),
            filing(
                "7.01",
                "Investor disclosure",
                "2026-09-28T20:05:00+00:00",
                "https://www.sec.gov/c",
            ),
        ],
        None,
    )];
    // Without prices, kind decides: a restatement, then the newest of the frequent kinds.
    let events = logged(&evidence, period(start, end), 3);
    assert_eq!(
        titles(&events),
        ["Restatement", "Earnings results", "TEST names a CFO"]
    );
    // The earnings filing stands for the story about it, which comes along as coverage.
    assert_eq!(events[1].coverage[0].title, "TEST beats estimates");
    assert_eq!(events[0].provenance, eventlog::Provenance::Sourced);
    assert_eq!(events[0].z, None);
    // On the portfolio a holding supplies one event.
    assert_eq!(logged(&evidence, period(start, end), 1).len(), 1);
    assert!(logged(&evidence, period(day("2026-10-01"), day("2026-10-02")), 3).is_empty());
}

#[test]
fn the_markets_reaction_outranks_how_much_was_written() {
    let (start, end) = (day("2026-08-31"), day("2026-09-30"));
    // A large company's earnings, widely covered, that its stock shrugged off.
    let mut covered: Vec<MarketNewsArticle> = (0..6)
        .map(|index| {
            article(
                &format!("BIG coverage {index}"),
                "2026-09-20T21:00:00Z",
                vec![],
                &format!("https://example.com/big{index}"),
            )
        })
        .collect();
    covered.push(article(
        "BIG beats estimates",
        "2026-09-20T21:00:00Z",
        vec![NewsTag::Earnings],
        "https://example.com/big",
    ));
    let big = holding(
        "BIG",
        covered,
        vec![],
        Some(own_with("2026-01-01", 280, &[])),
    );
    // A small company's one deal story that moved its stock 4% beyond its sector.
    let small = holding(
        "SMALL",
        vec![article(
            "SMALL agrees to a deal",
            "2026-09-10T13:00:00Z",
            vec![NewsTag::Deal],
            "https://example.com/small",
        )],
        vec![],
        Some(own_with("2026-01-01", 280, &[("2026-09-10", 4.0)])),
    );
    // A third company fell 6% beyond its sector with nothing written about it.
    let quiet = holding(
        "QUIET",
        vec![],
        vec![],
        Some(own_with("2026-01-01", 280, &[("2026-09-18", -6.0)])),
    );
    let events = logged(&[big, small, quiet], period(start, end), 1);
    assert_eq!(
        events
            .iter()
            .map(|event| (event.ticker.as_deref().unwrap(), event.family))
            .collect::<Vec<_>>(),
        [
            ("SMALL", eventlog::Family::News),
            ("QUIET", eventlog::Family::Statistical),
            ("BIG", eventlog::Family::News),
        ]
    );
    // An event with a cause outranks an unexplained move of a similar size; an unexplained one
    // is observed, never a cause; the widely covered earnings the stock shrugged off come last.
    assert!(events[0].rank.key() > events[1].rank.key());
    assert_eq!(events[1].provenance, eventlog::Provenance::Observed);
    assert_eq!(events[1].kind, "abnormalReturn");
    assert_eq!(events[2].rank.z_bucket, 0);
    // A story near the drop explains it: it becomes the event, the drop its evidence.
    let explained = holding(
        "QUIET",
        vec![article(
            "QUIET cuts its outlook",
            "2026-09-17T21:00:00Z",
            vec![],
            "https://example.com/q",
        )],
        vec![],
        Some(own_with("2026-01-01", 280, &[("2026-09-18", -6.0)])),
    );
    let events = logged(&[explained], period(start, end), 1);
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].family, events[0].title.as_str()),
        (eventlog::Family::News, "QUIET cuts its outlook")
    );
    assert_eq!(events[0].flags, ["abnormalReturn"]);
    assert!(events[0].z.unwrap() < -5.0);
}

#[test]
fn a_launch_that_moves_a_high_beta_stock_today_leads_the_week() {
    // A stock that moves about twice its sector, quiet all week until today, when it rose
    // 7 points beyond what its sector explains, on the day of a launch story. An earlier
    // foreign issuer's report drew no reaction.
    let (start, end) = (day("2026-09-29"), day("2026-10-06"));
    let own = own_with("2025-10-01", 371, &[("2026-10-06", 7.1)]);
    let mut report = filing("", "", "2026-10-02T11:00:00+00:00", "https://www.sec.gov/r");
    report.form = "6-K".into();
    report.items.clear();
    let nebius = holding(
        "NBIS",
        vec![
            article(
                "Nebius Rolls Out Agentic Open Model Beam",
                "2026-10-06T13:10:00Z",
                vec![NewsTag::Product],
                "https://example.com/beam",
            ),
            article(
                "Why Is Nebius Group Stock Soaring Tuesday?",
                "2026-10-06T16:02:00Z",
                vec![],
                "https://example.com/why",
            ),
        ],
        vec![report],
        Some(own),
    );
    let events = logged(std::slice::from_ref(&nebius), period(start, end), 3);
    assert_eq!(
        titles(&events),
        ["Nebius Rolls Out Agentic Open Model Beam"]
    );
    assert_eq!(events[0].z, Some(14.2));
    // The week counts today's session, so its own layer leads.
    let closes = BTreeMap::from([("NBIS".to_string(), 100.0), ("XLK".to_string(), 100.0)]);
    let prices = BTreeMap::from([("NBIS".to_string(), 108.8), ("XLK".to_string(), 104.0)]);
    let moved = period_move(
        "NBIS",
        Some(("XLK", "Technology")),
        &closes,
        &prices,
        Some(layers_of(0.4, 1.3, 7.1, 1.1, "XLK")),
    )
    .unwrap();
    assert_eq!(moved.kind, Some("ownMove"));
}

#[test]
fn today_names_the_holdings_and_sectors_that_moved() {
    let quote = |symbol: &str, change: f64| {
        (
            symbol.to_string(),
            MarketSnapshot {
                symbol: symbol.into(),
                price: 100.0 + change,
                previous_close: 100.0,
                previous_close_as_of: Some("2026-10-06T20:00:00Z".into()),
                daily_change_pct: change,
                weekly_change_pct: None,
                weekly_reference_price: None,
                weekly_reference_date: None,
                as_of: "2026-10-07T18:00:00Z".into(),
            },
        )
    };
    let snapshots = BTreeMap::from([quote("NBIS", 8.7), quote("XLK", 2.2), quote("SPY", 0.4)]);
    let today = holding_today("NBIS", Some(("XLK", "Technology")), &snapshots).unwrap();
    // The kind comes from the session's layers, never from the raw change.
    assert_eq!(today.kind, None);
    assert_eq!(today.sector.as_ref().unwrap().change_pct, 2.2);
    assert_eq!(today.market.as_ref().unwrap().change_pct, 0.4);
    // Without a previous close there is no day change, never a flat one.
    let mut stale = snapshots.clone();
    stale.get_mut("NBIS").unwrap().previous_close_as_of = None;
    assert!(holding_today("NBIS", None, &stale).is_none());
    let held = [
        HeldSecurity {
            ticker: "NBIS".into(),
            name: "Nebius".into(),
            shares: 10.0,
            value: 1_087.0,
            kind: Some("equity".into()),
        },
        HeldSecurity {
            ticker: "FUND".into(),
            name: "Fund".into(),
            shares: 10.0,
            value: 913.0,
            kind: Some("etf".into()),
        },
    ];
    let sectors = BTreeMap::from([("NBIS".to_string(), ("XLK", "Technology"))]);
    let portfolio = portfolio_today_move(
        &portfolio::portfolio_today(&held, &snapshots, &sectors),
        &held,
    )
    .unwrap();
    assert_eq!(portfolio.positions[0].ticker, "NBIS");
    // Technology is 54.4% of the portfolio, and its holdings rose 8.7% together; a fund
    // has no sector.
    assert_eq!(
        portfolio.sectors,
        vec![SectorToday {
            name: "Technology".into(),
            weight_pct: 54.4,
            change_pct: 8.7,
        }]
    );
}

#[test]
fn coverage_counts_only_relevant_company_stories_in_the_period() {
    let (start, end) = (day("2026-09-30"), day("2026-10-07"));
    let mut passing = article("Mention", "2026-10-02T12:00:00Z", vec![], "https://e.com/m");
    passing.relevance = 10;
    let stories = vec![
        article("In", "2026-10-02T12:00:00Z", vec![], "https://e.com/a"),
        article("Before", "2026-09-01T12:00:00Z", vec![], "https://e.com/b"),
        passing,
    ];
    let covered = coverage(
        &[holding("TEST", stories, vec![], None)],
        &period(start, end),
    );
    assert_eq!(
        covered,
        vec![Coverage {
            ticker: "TEST".into(),
            stories: 1,
            top_relevance: Some(90),
        }]
    );
}

#[test]
fn a_minor_filing_the_stock_ignored_is_left_out() {
    let (start, end) = (day("2026-08-31"), day("2026-09-30"));
    let mut report = filing("", "", "2026-09-02T20:05:00+00:00", "https://www.sec.gov/f");
    report.form = "6-K".into();
    report.items.clear();
    // Without prices it is the only thing known, so it stays.
    let unpriced = holding("FOREIGN", vec![], vec![report.clone()], None);
    assert_eq!(logged(&[unpriced], period(start, end), 3).len(), 1);
    // With prices showing no reaction, it says nothing.
    let priced = holding(
        "FOREIGN",
        vec![],
        vec![report],
        Some(own_with("2026-01-01", 280, &[])),
    );
    assert!(logged(&[priced], period(start, end), 3).is_empty());
}

#[test]
fn a_holdings_period_is_named_by_its_layers_and_names_its_benchmarks() {
    let closes = BTreeMap::from([
        ("TEST".to_string(), 100.0),
        ("SPY".to_string(), 500.0),
        ("XLK".to_string(), 200.0),
    ]);
    let prices = BTreeMap::from([
        ("TEST".to_string(), 112.0),
        ("SPY".to_string(), 510.0),
        ("XLK".to_string(), 202.0),
    ]);
    let sector = Some(("XLK", "Technology"));
    // Without layers there is no judgement, never a guess from the change alone.
    let moved = period_move("TEST", sector, &closes, &prices, None).unwrap();
    assert_eq!(moved.change_pct, 12.0);
    assert_eq!(moved.sector.as_ref().map(|b| b.change_pct), Some(1.0));
    assert_eq!(moved.market.as_ref().map(|b| b.change_pct), Some(2.0));
    assert_eq!((moved.kind, moved.layers.clone()), (None, None));
    // For a stock that wanders 4% a day on its own, the same month is ordinary: its group
    // explained most of it.
    let ordinary = period_move(
        "TEST",
        sector,
        &closes,
        &prices,
        Some(layers_of(2.4, 7.0, 2.6, 18.0, "XLK")),
    )
    .unwrap();
    assert_eq!(ordinary.kind, Some("withSector"));
    // For a steady stock, the month's own part is several typical months: its own move.
    let own = period_move(
        "TEST",
        sector,
        &closes,
        &prices,
        Some(layers_of(2.4, 1.0, 8.6, 2.0, "XLK")),
    )
    .unwrap();
    assert_eq!(own.kind, Some("ownMove"));
    // Without a start close there is no change, and a missing benchmark is simply absent.
    assert!(period_move("TEST", None, &BTreeMap::new(), &prices, None).is_none());
    let partial =
        period_move("TEST", Some(("XLV", "Health care")), &closes, &prices, None).unwrap();
    assert!(partial.sector.is_none());
}

#[test]
fn unusual_moves_are_named_by_how_unusual_not_by_size() {
    // Two holdings over a month of sessions: BIG wanders, SMALL drifts steadily on its own.
    let record = |session: NaiveDate, big: f64, small: f64| SessionRecord {
        session,
        method: sessions::METHOD,
        computed_at: String::new(),
        market_pct: 0.0,
        market_z: 0.0,
        holdings: [("BIG", big), ("SMALL", small)]
            .into_iter()
            .map(|(ticker, own_pct)| sessions::HoldingSession {
                ticker: ticker.into(),
                change_pct: own_pct,
                market_pct: 0.0,
                group_pct: 0.0,
                own_pct,
                own_z: Some(own_pct / 0.5),
                sigma: Some(0.5),
                change: None,
                market: None,
                group: None,
                own: None,
                fit: None,
            })
            .collect(),
        groups: Vec::new(),
        calibration: sessions::Calibration::default(),
        quiet: None,
    };
    let records: Vec<SessionRecord> = own_with("2026-09-01", 30, &[])
        .into_iter()
        .map(|(date, (wobble, _))| record(date, wobble * 3.0, 0.6))
        .collect();
    let refs: Vec<&SessionRecord> = records.iter().collect();
    let tickers = vec!["BIG".to_string(), "SMALL".to_string()];
    let changes = BTreeMap::from([("SMALL".to_string(), 18.2)]);
    let notable = notable_moves(&refs, &tickers, &changes);
    assert_eq!(notable.len(), 1);
    assert_eq!(notable[0].ticker, "SMALL");
    assert_eq!(notable[0].change_pct, Some(18.2));
    assert!(notable[0].layers.own_z.unwrap() >= NOTABLE_Z);
}
