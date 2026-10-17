use chrono::{NaiveDate, TimeZone, Utc};

use super::*;
use crate::{insights::calendar::NamedZone, insights::View, synthetic::career};

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

/// The career person's view on the day it is read, at noon UTC.
fn career_view(state: &crate::storage::FinanceState) -> View {
    let today = career::today();
    let now = Utc.from_utc_datetime(&today.and_hms_opt(12, 0, 0).unwrap());
    crate::storage::derive_view(state, None, &Default::default(), now, &NamedZone::Utc)
        .unwrap()
        .0
}

/// What the evidence graph found, for reading by hand.
fn print(view: &View, today: NaiveDate) {
    let evidence = &view.insights.as_ref().unwrap().evidence;
    for (month, (income, spending)) in chapters::monthly(&view.snapshot.transactions, today) {
        let prefix = month.format("%Y-%m").to_string();
        let count = view
            .snapshot
            .transactions
            .iter()
            .filter(|transaction| transaction.date.starts_with(&prefix))
            .count();
        println!("{month}: income {income:.0} spending {spending:.0} records {count}");
    }
    for explanation in &evidence.explanations {
        println!(
            "\n{:?} {} — {} ({} → {:?})\n  id {} subject {} object {:?}",
            explanation.tier,
            explanation.title,
            explanation.says,
            explanation
                .from
                .map(|date| date.to_string())
                .unwrap_or_default(),
            explanation.to,
            explanation.id,
            explanation.subject,
            explanation.object
        );
        for step in &explanation.proof.steps {
            println!(
                "  [{}] {} — {:?}",
                if step.met { "x" } else { " " },
                step.says,
                step.detail
            );
        }
        for unchecked in &explanation.proof.unchecked {
            println!("  [?] {unchecked}");
        }
    }
    for alert in &evidence.alerts {
        println!("\nalert {:?} {}: {}", alert.kind, alert.id, alert.says);
    }
    for chapter in &evidence.chapters {
        println!(
            "\nchapter {} {:?} {} — {} → {:?} ({:?})",
            chapter.id, chapter.kind, chapter.title, chapter.start, chapter.end, chapter.precision
        );
        for anchor in &chapter.anchors {
            println!("  anchor: {}", anchor.says);
        }
        for member in &chapter.members {
            println!("  {:?}: {} — {}", member.tier, member.id, member.says);
        }
        for effect in &chapter.effects {
            println!(
                "  {}: {:.0} → {:.0}",
                effect.measure, effect.before, effect.after
            );
        }
        for item in &chapter.checklist {
            println!("  todo: {item}");
        }
        if !chapter.programs.is_empty() {
            println!("  programs: {:?}", chapter.programs);
        }
    }
    let trips = &view.insights.as_ref().unwrap().trips;
    println!("\nhome regions: {:?}", trips.home);
    for trip in &trips.trips {
        println!(
            "trip {} {} → {} ({} purchases)",
            trip.destination,
            trip.start,
            trip.end,
            trip.transaction_ids.len()
        );
    }
    println!("\nnow: {:?}", evidence.now);
    for residency in &evidence.residency {
        println!("residency: {residency:?}");
    }
}

/// Prints what the evidence graph finds in a copy of a real finance database, for checking
/// detection by hand: `BRIEF_PROBE_DB=/path/to/copy.sqlite3 cargo test probe -- --ignored
/// --nocapture`. It opens the copy read-only and writes nothing; real data never enters a
/// fixture or a test that runs by default.
#[test]
#[ignore]
fn probe() {
    let Some(path) = std::env::var_os("BRIEF_PROBE_DB") else {
        return;
    };
    let database = crate::database::Database::open_read_only(std::path::Path::new(&path)).unwrap();
    let (state, _) = crate::storage::change::authoritative(&database).unwrap();
    let started = std::time::Instant::now();
    let (view, _) = crate::storage::derive_view(
        &state,
        None,
        &std::collections::BTreeMap::new(),
        chrono::Utc::now(),
        &crate::insights::calendar::LocalZone,
    )
    .unwrap();
    println!("derive_view: {:?}", started.elapsed());
    // What the evidence graph's part of that costs: rules, then chapters.
    let today = chrono::Local::now().date_naive();
    let snapshot = &view.snapshot;
    let started = std::time::Instant::now();
    let explained = explain(
        &snapshot.transactions,
        &snapshot.accounts,
        &state.documents,
        &state.settings,
        today,
    );
    let rules = started.elapsed();
    let started = std::time::Instant::now();
    let _ = evidence(
        explained,
        &snapshot.transactions,
        &snapshot.accounts,
        &Default::default(),
        &state.settings,
        today,
    );
    println!("rules: {rules:?}, chapters: {:?}", started.elapsed());
    print(&view, today);
}

/// The career person's evidence, printed: `cargo test career_probe -- --ignored --nocapture`.
#[test]
#[ignore]
fn career_probe() {
    print(&career_view(&career::state()), career::today());
}

fn evidence_of(view: &View) -> &Evidence {
    &view.insights.as_ref().unwrap().evidence
}

fn explanation<'a>(evidence: &'a Evidence, program: &str) -> Option<&'a Explanation> {
    evidence
        .explanations
        .iter()
        .find(|explanation| explanation.program == program)
}

const BONUS: &str = "ms-cashplus-engagement-bonus";
const WAIVER: &str = "ms-cashplus-employee-fee-waiver";

#[test]
fn the_bonus_explains_the_card_fee_with_a_cited_proof_and_repays_it() {
    let view = career_view(&career::state());
    let evidence = evidence_of(&view);
    let offset = explanation(evidence, BONUS).unwrap();
    assert_eq!(offset.tier, Tier::Explained);
    assert_eq!(
        (offset.subject.as_str(), offset.object.as_deref()),
        (
            "transaction:engagement-bonus",
            Some("transaction:membership-fee")
        )
    );
    assert_eq!(
        offset.says,
        "The $895 engagement bonus offsets the card's $895 annual fee"
    );
    assert!(offset
        .proof
        .steps
        .iter()
        .all(|step| step.met && !step.evidence.is_empty()));
    assert!(offset.proof.unchecked.is_empty());
    assert!(offset
        .proof
        .source
        .url
        .starts_with("https://www.morganstanley.com/"));
    // Applied, the bonus repays the fee rather than counting as income...
    let bonus = view
        .snapshot
        .transactions
        .iter()
        .find(|transaction| transaction.id == "engagement-bonus")
        .unwrap();
    assert_eq!(bonus.refund_of.as_deref(), Some("membership-fee"));
    assert_ne!(bonus.kind(), Some(crate::transaction_policy::Kind::Income));
    // ...and counts against the card's fee, the card known by the name its issuer reports.
    let insights = view.insights.as_ref().unwrap();
    let card = insights
        .other_card_benefits
        .iter()
        .find(|card| card.account_id == "plaid:msplatinum")
        .unwrap();
    assert_eq!(card.fee.as_ref().unwrap().offsets, 895.0);
}

#[test]
fn the_employee_waiver_stays_suggested_and_watches_each_month() {
    let view = career_view(&career::state());
    let evidence = evidence_of(&view);
    let waiver = explanation(evidence, WAIVER).unwrap();
    // Enrollment online is a condition no record shows.
    assert_eq!(waiver.tier, Tier::Suggested);
    assert_eq!(waiver.proof.unchecked.len(), 1);
    // From the account's first deposits in May through August; September fell short.
    assert_eq!(
        (waiver.from, waiver.to),
        (Some(day("2026-05-01")), Some(day("2026-08-31")))
    );
    assert_eq!(waiver.subject, "account:plaid:cashplus");
    let watch = evidence
        .alerts
        .iter()
        .find(|alert| alert.kind == AlertKind::Watch)
        .unwrap();
    assert_eq!(
        watch.says,
        "Deposits to Platinum CashPlus were under $5,000 in September 2026; the $55 monthly fee may apply."
    );
    // The note reaches Home's needs, and never a notification, since it names amounts.
    let needs = &view.insights.as_ref().unwrap().brief.needs;
    let need = needs
        .iter()
        .find(|need| need.id == format!("program:{}", watch.id))
        .unwrap();
    assert!(crate::notify::notices(std::slice::from_ref(need), |id| id.to_owned()).is_empty());
}

#[test]
fn reviews_confirm_or_dismiss_a_suggestion() {
    let view = career_view(&career::state());
    let id = explanation(evidence_of(&view), WAIVER).unwrap().id.clone();
    let mut state = career::state();
    state.settings.explanation_reviews =
        [(id.clone(), crate::settings::ExplanationReview::Confirmed)].into();
    let confirmed = career_view(&state);
    assert_eq!(
        explanation(evidence_of(&confirmed), WAIVER).unwrap().tier,
        Tier::Observed
    );
    state.settings.explanation_reviews =
        [(id, crate::settings::ExplanationReview::Dismissed)].into();
    assert!(explanation(evidence_of(&career_view(&state)), WAIVER).is_none());
}

#[test]
fn a_missing_condition_concludes_nothing_and_a_missing_bonus_is_expected() {
    // Without the issuer's name for the card, the rule can't bind the card or its fee.
    let mut state = career::state();
    for account in &mut state.snapshot.accounts {
        if account.id == "plaid:msplatinum" {
            account.official_name = Some("Example Travel Card".into());
        }
    }
    assert!(explanation(evidence_of(&career_view(&state)), BONUS).is_none());

    // Without the bonus, the fee's renewal still expects one, past its 60 days.
    let mut state = career::state();
    state
        .snapshot
        .transactions
        .retain(|transaction| transaction.id != "engagement-bonus");
    let view = career_view(&state);
    let evidence = evidence_of(&view);
    assert!(explanation(evidence, BONUS).is_none());
    let expect = evidence
        .alerts
        .iter()
        .find(|alert| alert.kind == AlertKind::Expect)
        .unwrap();
    assert_eq!(expect.program, BONUS);
    assert!(
        expect
            .says
            .ends_with("none has since the fee on 2026-05-22."),
        "{}",
        expect.says
    );
}

#[test]
fn terms_that_dont_cover_the_date_leave_a_conclusion_suggested() {
    let view = career_view(&career::state());
    let snapshot = &view.snapshot;
    let mut program = programs::programs(&[])
        .into_iter()
        .find(|program| program.id == BONUS)
        .unwrap();
    // Terms beginning after the bonus posted: a missing year is never extrapolated.
    for term in &mut program.terms {
        term.from = day("2026-08-01");
        term.to = None;
    }
    let employers = employment::employers(&[], &snapshot.transactions, career::today());
    let (found, _) = engine::evaluate(
        &program,
        &engine::Facts {
            transactions: &snapshot.transactions,
            accounts: &snapshot.accounts,
            employers: &employers,
            today: career::today(),
        },
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tier, Tier::Suggested);
}

#[test]
fn employers_come_from_pay_statements_w2s_and_payroll() {
    let state = career::state();
    let employers = employment::employers(
        &state.documents,
        &state.snapshot.transactions,
        career::today(),
    );
    let summary = employers
        .iter()
        .map(|employer| {
            (
                employer.name.as_str(),
                employer.first,
                employer.ongoing,
                employer.inferred,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            ("Example Analytics LLC", day("2024-12-23"), false, false),
            (
                "Morgan Stanley Services Group Inc.",
                day("2026-04-20"),
                true,
                false
            ),
        ]
    );
    // Every payroll deposit joined its employer, the CashPlus split included.
    let morgan = &employers[1];
    assert!(morgan
        .deposits
        .iter()
        .any(|(_, _, id)| id.starts_with("ms-pay-cashplus")));
    assert_eq!(
        employers[0].states.get(&2025).map(String::as_str),
        Some("CO")
    );
    // Without documents, payroll alone still finds both jobs, as inferred.
    let inferred = employment::employers(&[], &state.snapshot.transactions, career::today());
    assert_eq!(inferred.len(), 2);
    assert!(inferred.iter().all(|employer| employer.inferred));
}

#[test]
fn descriptors_name_employers_by_word_prefix_or_whole_name() {
    assert!(employment::names(
        "UNIVERSITY OF DE PAYROLL",
        "University of Denver"
    ));
    assert!(employment::names(
        "BOOK TRANSFER B/O FORGE GLOBAL INC",
        "Forge Global, Inc."
    ));
    assert!(employment::names(
        "MORGAN STANLEY SERV PAYROLL",
        "Morgan Stanley Services Group Inc."
    ));
    // One short word, or a different employer, is not a match.
    assert!(!employment::names("UNI PAYROLL", "University of Denver"));
    assert!(!employment::names(
        "EXAMPLE ANALYTICS PAYROLL",
        "Example Bank"
    ));
}

#[test]
fn chapters_tell_the_career() {
    let view = career_view(&career::state());
    let evidence = evidence_of(&view);
    let spans = evidence
        .chapters
        .iter()
        .map(|chapter| (chapter.id.as_str(), chapter.start, chapter.end))
        .collect::<Vec<_>>();
    assert_eq!(
        spans,
        vec![
            (
                "work:example-analytics-llc",
                day("2024-12-23"),
                Some(day("2026-03-27"))
            ),
            (
                "home:co:2025-01",
                day("2025-01-01"),
                Some(day("2026-03-31"))
            ),
            (
                "milestone:purchase:furniture",
                day("2025-11-03"),
                Some(day("2025-11-03"))
            ),
            ("home:ca:2026-04", day("2026-04-01"), None),
            (
                "work:morgan-stanley-services-group-inc",
                day("2026-04-20"),
                None
            ),
            (
                "milestone:accounts:2026-05",
                day("2026-05-10"),
                Some(day("2026-05-15"))
            ),
        ]
    );
    assert!(evidence
        .chapters
        .iter()
        .all(|chapter| chapter.tier == Tier::Suggested && !chapter.confirmed));
    let morgan = &evidence.chapters[4];
    assert_eq!(morgan.programs, vec![WAIVER.to_owned()]);
    // Accounts opened near the start are only suggested members.
    assert!(morgan
        .members
        .iter()
        .any(|member| member.id == "account:plaid:cashplus" && member.tier == Tier::Suggested));
    // The Chicago trip says nothing about where home is.
    assert!(!evidence
        .chapters
        .iter()
        .any(|chapter| chapter.state.as_deref() == Some("IL")));
    assert_eq!(
        evidence.now,
        chapters::Now {
            work: Some("work:morgan-stanley-services-group-inc".into()),
            work_title: Some("Morgan Stanley Services Group Inc.".into()),
            home: Some("California".into()),
            month: Some(6),
        }
    );
    let residency = evidence
        .residency
        .iter()
        .map(|span| (span.year, span.state.as_str(), span.days))
        .collect::<Vec<_>>();
    assert_eq!(
        residency,
        vec![(2025, "CO", 365), (2026, "CO", 90), (2026, "CA", 200)]
    );
}

#[test]
fn a_jobs_end_lists_what_changes() {
    // Morgan Stanley's pay stops after July: the job ends, and the waiver with it.
    let mut state = career::state();
    let cutoff = "2026-08-01";
    state.snapshot.transactions.retain(|transaction| {
        !(transaction.id.starts_with("ms-pay") && transaction.date.as_str() >= cutoff)
    });
    state
        .documents
        .retain(|document| document.date.as_deref().is_none_or(|date| date < cutoff));
    let view = career_view(&state);
    let morgan = evidence_of(&view)
        .chapters
        .iter()
        .find(|chapter| chapter.id == "work:morgan-stanley-services-group-inc")
        .unwrap();
    assert_eq!(morgan.end, Some(day("2026-07-31")));
    assert!(
        morgan
            .checklist
            .iter()
            .any(|item| item.contains("employee waiver lapses")),
        "{:?}",
        morgan.checklist
    );
    assert!(morgan
        .checklist
        .iter()
        .any(|item| item.starts_with("Check the final paycheck")));
}

#[test]
fn choices_confirm_dismiss_retitle_and_plan_chapters() {
    use crate::settings::{ChapterChoice, ChapterStatus, PlannedChapter, PlannedCost};
    let mut state = career::state();
    state.settings.chapters = vec![
        ChapterChoice {
            id: "work:morgan-stanley-services-group-inc".into(),
            status: ChapterStatus::Confirmed,
            title: Some("Morgan Stanley".into()),
            start: None,
            end: None,
        },
        ChapterChoice {
            id: "milestone:purchase:furniture".into(),
            status: ChapterStatus::Dismissed,
            title: None,
            start: None,
            end: None,
        },
    ];
    state.settings.planned_chapters = vec![PlannedChapter {
        id: "planned:move".into(),
        title: "Moving out".into(),
        start: day("2026-12-01"),
        end: None,
        costs: vec![PlannedCost {
            label: "Rent".into(),
            monthly: 2_000.0,
        }],
    }];
    let view = career_view(&state);
    let evidence = evidence_of(&view);
    let morgan = evidence
        .chapters
        .iter()
        .find(|chapter| chapter.id == "work:morgan-stanley-services-group-inc")
        .unwrap();
    assert_eq!(
        (morgan.title.as_str(), morgan.tier, morgan.confirmed),
        ("Morgan Stanley", Tier::Observed, true)
    );
    // A dismissal is kept, so the chapter isn't suggested again.
    assert!(!evidence
        .chapters
        .iter()
        .any(|chapter| chapter.id == "milestone:purchase:furniture"));
    let planned = evidence
        .chapters
        .iter()
        .find(|chapter| chapter.id == "planned:move")
        .unwrap();
    assert_eq!(
        (planned.kind, planned.tier),
        (chapters::ChapterKind::Planned, Tier::Observed)
    );
    // The confirmed job starts the new normal.
    let insights = view.insights.as_ref().unwrap();
    assert_eq!(
        chapters::baseline(&evidence.chapters, career::today()),
        Some(day("2026-04-20"))
    );
    assert_eq!(
        insights.plan.baseline_since, None,
        "April 20 is more than three months back"
    );
    assert_eq!(evidence.now.work_title.as_deref(), Some("Morgan Stanley"));
}

/// Gate B: every Explained fact has a proof, every proof step was met by named records, and
/// every rule cites an HTTPS source.
#[test]
fn every_explained_fact_has_a_proof_and_a_cited_source() {
    let state = career::state();
    let view = career_view(&state);
    let graph = crate::objects::graph(&state, &view);
    let explained = graph
        .facts
        .iter()
        .filter(|fact| fact.tier == Tier::Explained)
        .collect::<Vec<_>>();
    assert!(!explained.is_empty());
    let ids = graph
        .objects
        .iter()
        .map(|object| object.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for fact in explained {
        let proof = fact
            .proof
            .as_ref()
            .expect("an Explained fact carries its proof");
        assert!(
            proof.source.url.starts_with("https://"),
            "{}",
            proof.source.url
        );
        assert!(!proof.steps.is_empty() && proof.unchecked.is_empty());
        for step in &proof.steps {
            assert!(step.met, "{}", step.says);
            assert!(!step.evidence.is_empty(), "{}", step.says);
            for id in &step.evidence {
                assert!(ids.contains(id.as_str()), "{id} is in the graph");
            }
        }
        assert!(fact
            .sources
            .iter()
            .any(|source| source.starts_with("program:")));
    }
    for program in programs::programs(&[]) {
        program.validate().unwrap();
        assert!(
            !program.stale(career::today()),
            "{} needs its source re-read",
            program.id
        );
    }
    // Chapters and programs are objects, linked to what defines and depends on them.
    assert!(ids.contains("chapter:work:morgan-stanley-services-group-inc"));
    assert!(ids.contains(format!("program:{WAIVER}").as_str()));
    assert!(graph.links.iter().any(|link| {
        link.from == "chapter:work:morgan-stanley-services-group-inc"
            && link.relation == crate::objects::Relation::Program
            && link.to == format!("program:{WAIVER}")
    }));
    // A member only timing suggests is not a link; one a record shows is.
    let member_of = |chapter: &str| {
        graph.links.iter().any(|link| {
            link.from == "account:plaid:cashplus"
                && link.relation == crate::objects::Relation::Chapter
                && link.to == chapter
        })
    };
    assert!(!member_of("chapter:work:morgan-stanley-services-group-inc"));
    assert!(member_of("chapter:milestone:accounts:2026-05"));
}

#[test]
fn rules_are_checked_before_use() {
    let built = programs::programs(&[]);
    let mut broken = built[0].clone();
    broken.source.url = "http://example.com".into();
    assert!(broken.validate().is_err());
    let mut unbound = built[0].clone();
    unbound.conclusion.object = Some("nothing".into());
    assert!(unbound.validate().is_err());
    let mut pattern = built[0].clone();
    if let programs::Condition::Card { official_name, .. } = &mut pattern.conditions[1] {
        *official_name = "(".into();
    }
    assert!(pattern.validate().is_err());
    // An approved rule replaces the built-in rule of its id; an invalid one is left out.
    let mut approved = built[0].clone();
    approved.name = "Reviewed again".into();
    let merged = programs::programs(&[approved, broken]);
    assert_eq!(merged.len(), built.len());
    assert!(merged
        .iter()
        .any(|program| program.name == "Reviewed again"));
    let mut old = built[0].clone();
    old.reviewed = day("2025-01-01");
    assert!(old.stale(day("2026-10-17")));
}

/// Every ledger tool's result for the career person, for writing the eval set by hand.
#[test]
#[ignore]
fn career_tools() {
    let view = career_view(&career::state());
    let ledger = crate::insights::ledger::Ledger::new(&view, career::today());
    for (name, arguments) in [
        ("listAccounts", serde_json::json!({})),
        (
            "spendingByCategory",
            serde_json::json!({ "start": "2026-09-01", "end": "2026-09-30" }),
        ),
        (
            "cashFlow",
            serde_json::json!({ "start": "2026-09-01", "end": "2026-09-30" }),
        ),
        (
            "cashFlow",
            serde_json::json!({ "start": "2025-01-01", "end": "2025-12-31" }),
        ),
        (
            "searchTransactions",
            serde_json::json!({ "text": "engagement bonus" }),
        ),
        (
            "merchantHistory",
            serde_json::json!({ "merchant": "Example Residences" }),
        ),
        ("upcomingActivity", serde_json::json!({ "days": 30 })),
        ("monthPlan", serde_json::json!({})),
        ("creditCards", serde_json::json!({})),
        ("taxSummary", serde_json::json!({})),
        ("trips", serde_json::json!({})),
        ("subscriptions", serde_json::json!({})),
        ("explanations", serde_json::json!({})),
        ("chapters", serde_json::json!({})),
        (
            "chapterSpan",
            serde_json::json!({ "name": "since I moved" }),
        ),
        (
            "chapterSpan",
            serde_json::json!({ "name": "since joining Morgan Stanley" }),
        ),
    ] {
        println!(
            "\n## {name} {arguments}\n{}",
            serde_json::to_string_pretty(&ledger.call(name, &arguments)).unwrap()
        );
    }
}
