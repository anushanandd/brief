use super::*;

#[test]
fn family_names_match_their_serialized_form() {
    for family in [
        Family::Ledger,
        Family::CorporateAction,
        Family::Disclosure,
        Family::Macro,
        Family::News,
        Family::Statistical,
    ] {
        assert_eq!(serde_json::to_value(family).unwrap(), family.name());
    }
}

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn story(headline: &str, at: &str, url: &str, tags: Vec<NewsTag>) -> MarketNewsArticle {
    MarketNewsArticle {
        headline: headline.into(),
        summary: "Synthetic evidence".into(),
        source: "Benzinga".into(),
        url: url.into(),
        created_at: at.into(),
        symbols: vec!["TEST".into()],
        scope: NewsScope::Company,
        tags,
        relevance: 90,
    }
}

fn filing(form: &str, codes: &[&str], at: &str, accession: &str) -> sec::SecFiling {
    sec::SecFiling {
        form: form.into(),
        filed_at: at.into(),
        url: format!("https://www.sec.gov/Archives/edgar/data/1/{accession}/doc.htm"),
        items: codes
            .iter()
            .map(|code| sec::SecItem {
                code: (*code).into(),
                label: "Synthetic".into(),
            })
            .collect(),
        title: String::new(),
    }
}

struct World {
    trades: Vec<Trade>,
    transactions: Vec<Transaction>,
    accounts: BTreeSet<String>,
    actions: Vec<CorporateAction>,
    evidence: Vec<Evidence>,
    closes: DailyCloses,
    volumes: BTreeMap<String, BTreeMap<NaiveDate, f64>>,
    shares: BTreeMap<String, Shares>,
    held: BTreeSet<String>,
}

impl World {
    fn sources(&self) -> Sources<'_> {
        Sources {
            trades: &self.trades,
            transactions: &self.transactions,
            accounts: &self.accounts,
            corporate_actions: &self.actions,
            evidence: &self.evidence,
            market_news: &[],
            closes: &self.closes,
            volumes: &self.volumes,
            shares: &self.shares,
            held: &self.held,
            ingested_at: "2026-10-06T21:00:00+00:00",
        }
    }
}

/// A year of rising closes for TEST, ending at a new high on October 5 with heavy volume.
fn world() -> World {
    let mut closes = Vec::new();
    let mut volumes = BTreeMap::new();
    let mut date = day("2025-09-01");
    let mut price = 100.0;
    while date <= day("2026-10-05") {
        if session::is_trading_day(date) {
            price *= 1.001;
            closes.push((date, price));
            volumes.insert(
                date,
                if date == day("2026-10-05") {
                    5_000.0
                } else {
                    1_000.0
                },
            );
        }
        date = date.succ_opt().unwrap();
    }
    let trade: Trade = serde_json::from_value(json!({
        "id": "t1", "type": "BUY", "date": "2026-10-05", "amount": -500.0,
        "account": "Broker", "accountId": "broker", "ticker": "TEST", "units": 5.0,
    }))
    .unwrap();
    let dividend: Transaction = serde_json::from_value(json!({
        "id": "d1", "merchant": "Dividend · TEST", "category": "Dividend",
        "date": "2026-10-06", "amount": 12.5, "account": "Broker", "accountId": "broker",
        "pending": false, "description": null,
        "classification": { "mark": "dividend", "kind": "dividend", "spending": false, "zelle": false },
    }))
    .unwrap();
    World {
        trades: vec![trade],
        transactions: vec![dividend],
        accounts: BTreeSet::from(["broker".to_string()]),
        actions: vec![
            serde_json::from_value::<CorporateAction>(json!({
                "kind": "dividend", "symbol": "TEST", "exDate": "2026-10-05",
                "payableDate": "2026-10-06", "rate": 0.25
            }))
            .unwrap(),
            serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "OTHER", "exDate": "2026-10-05", "rate": 1.0 })).unwrap(),
        ],
        evidence: vec![Evidence {
            ticker: "TEST".into(),
            articles: vec![
                story(
                    "TEST Beats Third Quarter Estimates On Strong Cloud Sales",
                    "2026-10-05T12:00:00Z",
                    "https://example.com/beat",
                    vec![NewsTag::Earnings],
                ),
                story(
                    "TEST Beats Third Quarter Estimates On Strong Cloud Sales Growth",
                    "2026-10-05T13:00:00Z",
                    "https://example.com/beat-again",
                    vec![NewsTag::Earnings],
                ),
                story(
                    "Why Is TEST Stock Soaring Monday?",
                    "2026-10-05T16:00:00Z",
                    "https://example.com/why",
                    vec![NewsTag::Explainer],
                ),
            ],
            filings: vec![
                filing("8-K", &["2.02"], "2026-10-05T11:30:00+00:00", "0000001"),
                filing("8-K/A", &["2.02"], "2026-10-12T20:30:00+00:00", "0000002"),
                filing("10-Q", &[], "2026-11-02T21:30:00+00:00", "0000003"),
            ],
            own: BTreeMap::from([(day("2026-10-05"), (3.0, 1.0))]),
        }],
        closes: BTreeMap::from([("TEST".to_string(), closes)]),
        volumes: BTreeMap::from([("TEST".to_string(), volumes)]),
        shares: BTreeMap::from([(
            "TEST".to_string(),
            Shares::rebuild("TEST", 40.0, &[], day("2026-09-01")).unwrap(),
        )]),
        held: BTreeSet::from(["TEST".to_string()]),
    }
}

fn record(z: f64) -> SessionRecord {
    serde_json::from_value(json!({
        "session": "2026-10-05", "method": 1, "computedAt": "", "marketPct": 0.1,
        "marketZ": 0.1, "groups": [],
        "calibration": { "modeled": 1, "fundGroups": 0, "looseGroups": 0, "notable": 1 },
        "holdings": [{
            "ticker": "TEST", "changePct": 3.2, "marketPct": 0.1, "groupPct": 0.1,
            "ownPct": 3.0, "ownZ": z, "sigma": 1.0, "change": 128.0, "market": 4.0,
            "group": 4.0, "own": 120.0, "fit": null,
        }],
    }))
    .unwrap()
}

fn kinds(events: &[Event]) -> Vec<(Family, &str)> {
    events
        .iter()
        .map(|event| (event.family, event.kind.as_str()))
        .collect()
}

#[test]
fn a_session_holds_every_family_with_where_each_fact_came_from() {
    let world = world();
    let events = session_events(day("2026-10-05"), Some(&record(3.0)), &world.sources());
    let found = kinds(&events);
    assert!(found.contains(&(Family::Ledger, "buy")));
    assert!(found.contains(&(Family::CorporateAction, "cashDividend")));
    assert!(found.contains(&(Family::Disclosure, "8-K")));
    assert!(found.contains(&(Family::News, "story")));
    assert!(found.contains(&(Family::Statistical, "abnormalReturn")));
    assert!(found.contains(&(Family::Statistical, "unusualVolume")));
    assert!(found.contains(&(Family::Statistical, "newHigh")));
    // Only held securities' actions; the dividend credit lands the next session.
    assert_eq!(
        events
            .iter()
            .filter(|event| event.family == Family::CorporateAction)
            .count(),
        1
    );
    let action = events
        .iter()
        .find(|event| event.kind == "cashDividend")
        .unwrap();
    assert_eq!(action.effect, Some(10.0));
    let filing = events.iter().find(|event| event.kind == "8-K").unwrap();
    assert_eq!(filing.id, "filing:0000001");
    assert_eq!(filing.provenance, Provenance::Sourced);
    assert_eq!(filing.subtype.as_deref(), Some("2.02"));
    let flag = events
        .iter()
        .find(|event| event.kind == "abnormalReturn")
        .unwrap();
    assert_eq!(flag.provenance, Provenance::Observed);
    assert_eq!((flag.effect, flag.z), (Some(120.0), Some(3.0)));
    // Built again, the same facts keep the same ids.
    let again = session_events(day("2026-10-05"), Some(&record(3.0)), &world.sources());
    assert_eq!(
        events.iter().map(|event| &event.id).collect::<Vec<_>>(),
        again.iter().map(|event| &event.id).collect::<Vec<_>>()
    );
    let next = session_events(day("2026-10-06"), None, &world.sources());
    assert!(kinds(&next).contains(&(Family::Ledger, "dividend")));
    assert_eq!(next[0].symbol.as_deref(), Some("TEST"));
}

#[test]
fn precedence_chains_and_clusters_link_a_fact_reported_twice() {
    let world = world();
    let mut events = Vec::new();
    for date in ["2026-10-05", "2026-10-06", "2026-10-13", "2026-11-03"] {
        let date = day(date);
        let record = (date == day("2026-10-05")).then(|| record(3.0));
        events.extend(session_events(date, record.as_ref(), &world.sources()));
    }
    let links = link(&events);
    let id = |family: Family, kind: &str| {
        events
            .iter()
            .find(|event| event.family == family && event.kind == kind)
            .unwrap()
            .id
            .clone()
    };
    // The ledger's credit beats the action that expected it.
    assert_eq!(
        links
            .superseded_by
            .get(&id(Family::CorporateAction, "cashDividend")),
        Some(&id(Family::Ledger, "dividend"))
    );
    // The 8-K beats the news about the same earnings; its rewrite clusters with it.
    let first = events
        .iter()
        .find(|event| {
            event.source.as_ref().unwrap().url.as_deref() == Some("https://example.com/beat")
        })
        .unwrap();
    assert_eq!(
        links.superseded_by.get(&first.id),
        Some(&"filing:0000001".to_string())
    );
    let rewrite = events
        .iter()
        .find(|event| {
            event.source.as_ref().unwrap().url.as_deref() == Some("https://example.com/beat-again")
        })
        .unwrap();
    assert_eq!(links.cluster.get(&rewrite.id), Some(&first.id));
    assert!(links.first.contains(&first.id) && !links.first.contains(&rewrite.id));
    // The amendment revises the 8-K and folds into it; the 10-Q completes its earnings
    // chain.
    assert!(links.revised.contains("filing:0000001"));
    assert_eq!(
        links.superseded_by.get("filing:0000002"),
        Some(&"filing:0000001".to_string())
    );
    assert_eq!(
        links.chain.get("filing:0000001"),
        Some(&"earnings:TEST:2026Q3".to_string())
    );
    assert_eq!(
        links.chain.get("filing:0000003"),
        links.chain.get("filing:0000001")
    );
    // The flag is evidence for the filing, never a second event.
    let flag = id(Family::Statistical, "abnormalReturn");
    assert_eq!(
        links.evidence_for.get(&flag),
        Some(&"filing:0000001".to_string())
    );
    // Explainers are never causes.
    let why = events
        .iter()
        .find(|event| event.subtype.as_deref() == Some("explainer"))
        .unwrap();
    assert_eq!(why.source.as_ref().unwrap().input_class, InputClass::Green);
}

#[test]
fn events_rank_by_visible_components_never_a_score() {
    let world = world();
    let events = session_events(day("2026-10-05"), Some(&record(3.0)), &world.sources());
    let links = link(&events);
    let filing = events.iter().find(|event| event.kind == "8-K").unwrap();
    let rank = rank(filing, &links, 10_000.0);
    // The filing is worth its holding's own $120 that session: the 1% bucket of $10,000.
    assert_eq!(
        (rank.dollar_bucket, rank.z_bucket, rank.prior, rank.novel),
        (3, 3, 2, true)
    );
    let flag = events
        .iter()
        .find(|event| event.kind == "abnormalReturn")
        .unwrap();
    // Same dollars and move, but a measurement ranks below an earnings report.
    assert_eq!(super::rank(flag, &links, 10_000.0).dollar_bucket, 3);
    assert!(rank.key() > super::rank(flag, &links, 10_000.0).key());
    // A story that was a rewrite isn't novel.
    let rewrite = events
        .iter()
        .find(|event| {
            event.source.as_ref().unwrap().url.as_deref() == Some("https://example.com/beat-again")
        })
        .unwrap();
    assert!(!super::rank(rewrite, &links, 10_000.0).novel);
}

#[test]
fn quiet_is_a_result_and_needs_no_flag_and_an_ordinary_day() {
    let world = world();
    let calm = record(0.4);
    let events: Vec<Event> = session_events(day("2026-10-05"), Some(&calm), &world.sources())
        .into_iter()
        .filter(|event| event.family == Family::Macro)
        .collect();
    let quiet = quiet(&calm, &events, &BTreeMap::new(), Some(500.0));
    assert!(quiet.portfolio);
    assert_eq!(quiet.holdings, ["TEST"]);
    // A flagged holding, an unusual volume, or a large day is not quiet.
    let loud = quiet_of(&world, 3.0);
    assert!(!loud.portfolio);
    assert!(loud.holdings.is_empty());
    let big = super::quiet(&calm, &events, &BTreeMap::new(), Some(50.0));
    assert!(!big.portfolio);
    assert_eq!(big.z, Some(2.56));
}

fn quiet_of(world: &World, z: f64) -> Quiet {
    let loud = record(z);
    let events = session_events(day("2026-10-05"), Some(&loud), &world.sources());
    quiet(&loud, &events, &world.volumes, Some(500.0))
}

#[test]
fn scheduled_market_wide_events_come_from_the_reviewed_calendar_and_published_rules() {
    let world = world();
    let ahead = scheduled_macro(day("2026-10-08"), day("2026-12-31"), &world.sources());
    let found: Vec<(&str, NaiveDate)> = ahead
        .iter()
        .map(|event| (event.kind.as_str(), event.session))
        .collect();
    assert!(found.contains(&("fomc", day("2026-10-28"))));
    assert!(found.contains(&("fomc", day("2026-12-09"))));
    assert!(found.contains(&("holiday", day("2026-11-27"))));
    assert!(found.contains(&("earlyClose", day("2026-11-27"))));
    // The Monday after the third Friday of December.
    assert!(found.contains(&("indexRebalance", day("2026-12-21"))));
    assert!(ahead.iter().all(|event| event.status == Status::Scheduled));
    // A BLS release is never inferred.
    assert!(!found.iter().any(|(kind, _)| *kind == "cpi"));
    assert!(rebalance(day("2026-09-21")));
    assert!(!rebalance(day("2026-09-14")));
}

#[test]
fn a_period_lists_each_fact_once_its_first_report_and_its_chains_ends() {
    let world = world();
    let mut events = Vec::new();
    for date in ["2026-10-05", "2026-10-06", "2026-10-13", "2026-11-03"] {
        let date = day(date);
        let record = (date == day("2026-10-05")).then(|| record(3.0));
        events.extend(session_events(date, record.as_ref(), &world.sources()));
    }
    let links = link(&events);
    let listed = listed(&events, &links, 10_000.0, 3, 10);
    let ids: Vec<&str> = listed.iter().map(|event| event.id.as_str()).collect();
    // The 8-K stands for the earnings news, carries it as coverage, and is marked revised;
    // the new high is listed, the explainer and ledger never are.
    let filing = listed
        .iter()
        .find(|event| event.id == "filing:0000001")
        .unwrap();
    assert_eq!(filing.status, Status::Revised);
    assert_eq!(filing.coverage.len(), 2);
    // Flags measured on the session attach to the filing rather than listing again.
    assert_eq!(filing.flags, ["abnormalReturn", "unusualVolume", "newHigh"]);
    assert!(!ids.iter().any(|id| id.starts_with("stat:")));
    assert!(!listed
        .iter()
        .any(|event| matches!(event.family, Family::Ledger | Family::Macro)));
    assert!(!listed
        .iter()
        .any(|event| event.subtype.as_deref() == Some("explainer")));
    // The chain lists its first and completing steps; the amendment folds in.
    assert!(ids.contains(&"filing:0000003"));
    assert!(!ids.contains(&"filing:0000002"));
    // Three per company at most.
    assert!(listed.len() <= 3);
}

#[test]
fn publishers_carry_their_model_input_class() {
    assert_eq!(input_class("Benzinga"), InputClass::Green);
    assert_eq!(input_class("CNBC"), InputClass::Amber);
    assert_eq!(input_class("The Wall Street Journal"), InputClass::Red);
}
