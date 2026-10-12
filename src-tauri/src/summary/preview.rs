//! A summary of the synthetic seed for the browser preview, made by the same code the app runs:
//! the period reconciled from synthetic daily values and closes, the event log listing synthetic
//! filings and stories, and the day's brief with headlines picked from synthetic stories.
//! Every figure and story here is synthetic.
use super::eventlog::{self, Sources};
use super::*;
use crate::numeric::round2;
use crate::session::{Period, SessionClock};

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

/// The seed, whose holdings, prices, and month the preview summarizes.
fn seed() -> Value {
    serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap()
}

/// Each security's month, as a share of the portfolio's: NVDA strong, bonds flat.
const TILT: [(&str, f64, f64); 6] = [
    ("VTI", 1.0, 0.9),
    ("BND", 0.2, 0.25),
    ("VXUS", 0.5, 0.8),
    ("MSFT", 1.3, 1.2),
    ("AAPL", 0.6, 1.3),
    ("NVDA", 3.0, 2.0),
];

/// The month's sessions, ending with the seed's.
fn sessions() -> Vec<NaiveDate> {
    let mut sessions = Vec::new();
    let mut date = day("2026-07-31");
    while date <= day("2026-08-31") {
        if crate::session::is_trading_day(date) {
            sessions.push(date);
        }
        date = date.succ_opt().unwrap();
    }
    sessions
}

/// A repeatable wobble in [-1, 1).
fn wobble(seed: usize, index: usize) -> f64 {
    let mut state = (seed as u64 + 1).wrapping_mul(6364136223846793005);
    for _ in 0..=index {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    (state >> 33) as f64 / (1u64 << 30) as f64 - 1.0
}

struct Held {
    ticker: String,
    shares: f64,
    price: f64,
    daily_pct: f64,
}

fn held(seed: &Value) -> Vec<Held> {
    seed["holdings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|holding| Held {
            ticker: holding["ticker"].as_str().unwrap().into(),
            shares: holding["shares"].as_f64().unwrap(),
            price: holding["price"].as_f64().unwrap(),
            daily_pct: holding["dailyChangePct"].as_f64().unwrap(),
        })
        .collect()
}

/// The seed's combined brokerages at the end of July and of August.
fn month_values(seed: &Value) -> (f64, f64) {
    let points = seed["brokeragePerformance"][0]["points"]
        .as_array()
        .unwrap();
    let value = |index: usize| points[index]["value"].as_f64().unwrap();
    (value(points.len() - 2), value(points.len() - 1))
}

/// Closes for each security that end at its seed price after its seed day, and start the month
/// where the portfolio's July value puts them, given the tilts.
fn closes(seed: &Value) -> DailyCloses {
    let sessions = sessions();
    let held = held(seed);
    let (start_value, end_value) = month_values(seed);
    let gain = end_value - start_value;
    // Each security's month return is its tilt times a common rate solved so the holdings
    // gain the portfolio's whole gain (cash earns nothing in the preview).
    let tilt = |ticker: &str| TILT.iter().find(|(t, ..)| *t == ticker).unwrap();
    let ended = |rate: f64| -> f64 {
        held.iter()
            .map(|h| h.shares * h.price * (1.0 - 1.0 / (1.0 + tilt(&h.ticker).1 * rate)))
            .sum()
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..80 {
        let middle = (low + high) / 2.0;
        if ended(middle) < gain {
            low = middle;
        } else {
            high = middle;
        }
    }
    let rate = (low + high) / 2.0;
    held.iter()
        .enumerate()
        .map(|(seed_index, h)| {
            let (_, month, volatility) = *tilt(&h.ticker);
            let last = sessions.len() - 1;
            let start = h.price / (1.0 + month * rate);
            let before_last = h.price / (1.0 + h.daily_pct / 100.0);
            // A wobbling path from the start to the day before, then the seed's last day.
            let steps = last - 1;
            let mut wander = vec![0.0];
            for index in 1..=steps {
                let r = volatility / 100.0 * wobble(seed_index, index);
                wander.push(wander[index - 1] + r);
            }
            let drift = wander[steps];
            let series = sessions
                .iter()
                .enumerate()
                .map(|(index, date)| {
                    let price = if index == last {
                        h.price
                    } else {
                        let along = index as f64 / steps as f64;
                        let log =
                            start.ln() + along * (before_last.ln() - start.ln()) + wander[index]
                                - along * drift;
                        log.exp()
                    };
                    (*date, round2(price))
                })
                .collect();
            (h.ticker.clone(), series)
        })
        .collect()
}

/// The seed with its combined brokerages' history made daily from the closes; the cash beside
/// the holdings stays put, and the S&P 500 line runs between the seed's monthly marks.
fn snapshot(seed: &Value, closes: &DailyCloses) -> Value {
    let held = held(seed);
    let (_, end_value) = month_values(seed);
    let holdings_end: f64 = held.iter().map(|h| h.shares * h.price).sum();
    let cash = end_value - holdings_end;
    let points = seed["brokeragePerformance"][0]["points"]
        .as_array()
        .unwrap();
    let deposits = points.last().unwrap()["netDeposits"].as_f64().unwrap();
    let mark = |index: usize| points[index]["sp500"].as_f64().unwrap();
    let (sp_start, sp_end) = (mark(points.len() - 2), mark(points.len() - 1));
    let sessions = sessions();
    let last = sessions.len() - 1;
    let daily: Vec<Value> = sessions
        .iter()
        .enumerate()
        .map(|(index, date)| {
            let holdings: f64 = held
                .iter()
                .map(|h| h.shares * closes[&h.ticker][index].1)
                .sum();
            let along = index as f64 / last as f64;
            let sp500 =
                sp_start * (sp_end / sp_start).powf(along) * (1.0 + 0.002 * wobble(9, index));
            json!({
                "date": date.to_string(),
                "value": round2(holdings + cash),
                "netDeposits": deposits,
                "sp500": if index == last { sp_end } else { round2(sp500) },
            })
        })
        .collect();
    let mut snapshot = seed.clone();
    snapshot["brokeragePerformance"][0]["points"] = Value::Array(daily);
    snapshot
}

fn story(ticker: &str, headline: &str, at: &str, tags: Vec<NewsTag>) -> MarketNewsArticle {
    MarketNewsArticle {
        headline: headline.into(),
        summary: "Synthetic preview story.".into(),
        source: "Benzinga".into(),
        url: format!("https://example.com/preview/{}", headline.len()),
        created_at: at.into(),
        symbols: vec![ticker.into()],
        scope: NewsScope::Company,
        tags,
        relevance: 88,
    }
}

/// The browser preview's summary of the seed's month.
pub(crate) fn summary() -> Value {
    let seed = seed();
    let closes = closes(&seed);
    let snapshot = snapshot(&seed, &closes);
    let held = held(&seed);
    let period = Period {
        start: day("2026-07-31"),
        end: day("2026-08-31"),
        through: day("2026-08-31"),
    };
    let clock = SessionClock::at("2026-08-31T21:42:00Z".parse().unwrap());
    let mut summary = InvestmentSummary::new(period, None, clock);
    summary.generated_at = "2026-08-31T21:44:00+00:00".into();
    summary.prices_as_of = Some("2026-08-31T20:00:00+00:00".into());
    let typed: Snapshot = serde_json::from_value(snapshot.clone()).unwrap();
    summary.report = Some(reconcile::report(&typed, &closes, period.start, None));
    let launch = story(
        "NVDA",
        "Nvidia Wins Supply Agreement For Data Center Chips",
        "2026-08-31T12:30:00Z",
        vec![NewsTag::Deal],
    );
    let before = |h: &Held| h.shares * h.price / (1.0 + h.daily_pct / 100.0);
    let positions: Vec<TodayPosition> = held
        .iter()
        .map(|h| TodayPosition {
            ticker: h.ticker.clone(),
            change_pct: h.daily_pct,
            change: round2(h.shares * h.price - before(h)),
            kind: match h.ticker.as_str() {
                "NVDA" => Some("ownMove"),
                "VTI" | "MSFT" => Some("withMarket"),
                _ => None,
            },
        })
        .collect();
    let change: f64 = positions.iter().map(|position| position.change).sum();
    let (_, end_value) = month_values(&seed);
    // Today from the seed's daily changes, with the S&P 500 up 0.41%.
    summary.today = Some(TodayMove {
        as_of: Some("2026-08-31T20:00:00+00:00".into()),
        change_pct: Some(round2(change / (end_value - change) * 100.0)),
        change: Some(round2(change)),
        market: Some(Benchmark {
            symbol: "SPY".into(),
            name: "S&P 500".into(),
            change_pct: 0.41,
        }),
        sector: None,
        kind: None,
        positions,
        sectors: Vec::new(),
    });
    // The month's events, listed by the event log.
    let evidence = vec![
        Evidence {
            ticker: "NVDA".into(),
            articles: vec![launch],
            filings: Vec::new(),
            own: BTreeMap::from([(day("2026-08-31"), (1.73, 2.6))]),
        },
        Evidence {
            ticker: "MSFT".into(),
            articles: vec![story(
                "MSFT",
                "Microsoft Names New Head Of Cloud Business",
                "2026-08-19T13:00:00Z",
                vec![NewsTag::Leadership],
            )],
            filings: vec![sec::SecFiling {
                form: "8-K".into(),
                filed_at: "2026-08-19T12:05:00+00:00".into(),
                url: "https://www.sec.gov/Archives/edgar/data/1/0000000001/preview.htm".into(),
                items: vec![sec::SecItem {
                    code: "5.02".into(),
                    label: "Leadership change".into(),
                }],
                title: String::new(),
            }],
            own: BTreeMap::from([(day("2026-08-19"), (-1.1, 1.0))]),
        },
    ];
    let tickers: BTreeSet<String> = held.iter().map(|h| h.ticker.clone()).collect();
    let accounts = BTreeSet::new();
    let (volumes, shares) = (BTreeMap::new(), BTreeMap::new());
    let actions = vec![CorporateAction::Dividend {
        symbol: "VTI".into(),
        ex_date: day("2026-09-25"),
        payable_date: Some(day("2026-09-30")),
        rate: 0.95,
        special: false,
        currency: "USD".into(),
    }];
    let sources = Sources {
        trades: &[],
        transactions: &[],
        accounts: &accounts,
        corporate_actions: &actions,
        evidence: &evidence,
        market_news: &[],
        closes: &closes,
        volumes: &volumes,
        shares: &shares,
        held: &tickers,
        ingested_at: "2026-08-31T21:43:00+00:00",
    };
    let mut events = Vec::new();
    for session in sessions() {
        events.extend(eventlog::session_events(session, None, &sources));
    }
    let links = eventlog::link(&events);
    summary.events = eventlog::listed(&events, &links, end_value, 1, LISTED_EVENTS);
    summary.ahead = upcoming(period.end, &sources);
    summary.quiet = Some(eventlog::Quiet {
        portfolio: false,
        z: Some(-1.4),
        holdings: vec!["BND".into()],
    });
    // The day's headlines, from an editor's story and a market wrap.
    let wire = |headline: &str, at: &str, url: &str| MarketNewsArticle {
        headline: headline.into(),
        summary: "Synthetic preview story.".into(),
        source: "CNBC".into(),
        url: url.into(),
        created_at: at.into(),
        symbols: Vec::new(),
        scope: NewsScope::Market,
        tags: Vec::new(),
        relevance: 0,
    };
    let stories: Vec<MarketNewsArticle> = evidence
        .iter()
        .flat_map(|holding| holding.articles.iter().cloned())
        .chain([
            wire(
                "Nvidia climbs after winning a data center supply deal",
                "2026-08-31T17:05:00Z",
                "https://example.com/preview/nvidia-climbs",
            ),
            wire(
                "Stocks rise as tech rallies into month-end",
                "2026-08-31T20:25:00Z",
                "https://example.com/preview/stocks-rise",
            ),
        ])
        .collect();
    let securities = held_securities(&typed.holdings);
    summary.brief = summary.today.as_ref().map(|today| {
        day_brief(
            today,
            None,
            &securities,
            &stories,
            &summary.session,
            period.end,
        )
    });
    serde_json::to_value(&summary).unwrap()
}

#[test]
fn the_preview_summary_is_complete_with_its_brief_and_headlines() {
    let preview = summary();
    assert!(preview["report"]["statement"]["investmentGain"].is_number());
    assert!(!preview["report"]["attribution"]["contributions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!preview["events"].as_array().unwrap().is_empty());
    assert!(!preview["ahead"].as_array().unwrap().is_empty());
    assert_eq!(preview["brief"]["lines"].as_array().unwrap().len(), 2);
    let headlines = preview["brief"]["headlines"].as_array().unwrap();
    let titles: Vec<&str> = headlines
        .iter()
        .map(|headline| headline["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Nvidia climbs after winning a data center supply deal",
            "Stocks rise as tech rallies into month-end"
        ]
    );
}
