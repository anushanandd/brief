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

const BONUS: &str = "example-checking-loyalty-bonus";
const WAIVER: &str = "example-premier-employee-fee-waiver";

#[test]
fn the_bonus_explains_the_fee_with_a_cited_proof_and_repays_it() {
    let view = career_view(&career::state());
    let evidence = evidence_of(&view);
    let offset = explanation(evidence, BONUS).unwrap();
    assert_eq!(offset.tier, Tier::Explained);
    assert_eq!(
        (offset.subject.as_str(), offset.object.as_deref()),
        ("transaction:loyalty-bonus", Some("transaction:account-fee"))
    );
    assert_eq!(
        offset.says,
        "The $100 loyalty bonus offsets the account's $100 annual fee"
    );
    assert!(offset
        .proof
        .steps
        .iter()
        .all(|step| step.met && !step.evidence.is_empty()));
    assert!(offset.proof.unchecked.is_empty());
    assert!(offset.proof.source.url.starts_with("https://bank.example/"));
    // Applied, the bonus repays the fee rather than counting as income.
    let bonus = view
        .snapshot
        .transactions
        .iter()
        .find(|transaction| transaction.id == "loyalty-bonus")
        .unwrap();
    assert_eq!(bonus.refund_of.as_deref(), Some("account-fee"));
    assert_ne!(bonus.kind(), Some(crate::transaction_policy::Kind::Income));
}

#[test]
fn the_employee_waiver_stays_suggested_and_watches_each_month() {
    let view = career_view(&career::state());
    let evidence = evidence_of(&view);
    let waiver = explanation(evidence, WAIVER).unwrap();
    // Enrollment online is a condition no record shows.
    assert_eq!(waiver.tier, Tier::Suggested);
    assert_eq!(waiver.proof.unchecked.len(), 1);
    // From the account's first deposits in April through August; September fell short.
    assert_eq!(
        (waiver.from, waiver.to),
        (Some(day("2026-04-01")), Some(day("2026-08-31")))
    );
    assert_eq!(waiver.subject, "account:plaid:premier");
    let watch = evidence
        .alerts
        .iter()
        .find(|alert| alert.kind == AlertKind::Watch)
        .unwrap();
    assert_eq!(
        watch.says,
        "Deposits to Premier Checking were under $5,000 in September 2026; the $25 monthly fee may apply."
    );
    // A past month can't be changed, so the note stays with the rule and never reaches Home.
    let needs = &view.insights.as_ref().unwrap().brief.needs;
    assert!(needs
        .iter()
        .all(|need| need.id != format!("program:{}", watch.id)));
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
    // At another bank, the rule can't bind the account or its fee.
    let mut state = career::state();
    for account in &mut state.snapshot.accounts {
        if account.id == "plaid:checking" {
            account.institution = "Other Bank".into();
        }
    }
    assert!(explanation(evidence_of(&career_view(&state)), BONUS).is_none());

    // Without the bonus, the fee still expects one, past its 60 days.
    let mut state = career::state();
    state
        .snapshot
        .transactions
        .retain(|transaction| transaction.id != "loyalty-bonus");
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
    let mut program = career::loyalty_bonus();
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
fn no_rule_concludes_or_alerts_from_nothing_or_before_its_records() {
    let view = career_view(&career::state());
    let snapshot = &view.snapshot;
    let employers = employment::employers(&[], &snapshot.transactions, career::today());
    for program in [career::employee_waiver(), career::loyalty_bonus()] {
        let (found, alerts) = engine::evaluate(
            &program,
            &engine::Facts {
                transactions: &[],
                accounts: &[],
                employers: &[],
                today: career::today(),
            },
        );
        assert!(
            found.is_empty() && alerts.is_empty(),
            "{} from nothing",
            program.id
        );
        // The same records read on a day before any of them conclude nothing and raise no
        // alert: a rule knows nothing dated after `today`, so nothing is Explained early.
        let (found, alerts) = engine::evaluate(
            &program,
            &engine::Facts {
                transactions: &snapshot.transactions,
                accounts: &snapshot.accounts,
                employers: &employers,
                today: day("2000-01-01"),
            },
        );
        assert!(
            found.is_empty(),
            "{} concluded before its records: {:?}",
            program.id,
            found
                .iter()
                .map(|found| (&found.says, found.from))
                .collect::<Vec<_>>()
        );
        assert!(
            alerts.is_empty(),
            "{} before its records: {:?}",
            program.id,
            alerts
                .iter()
                .map(|alert| (&alert.says, alert.date))
                .collect::<Vec<_>>()
        );
    }
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
                "Example Corp Services Group Inc.",
                day("2026-03-02"),
                true,
                false
            ),
        ]
    );
    // Each payroll deposit links to its employer, including the part split into another account.
    let corp = &employers[1];
    assert!(corp
        .deposits
        .iter()
        .any(|(_, _, id)| id.starts_with("corp-pay-premier")));
    assert_eq!(
        employers[0].states.get(&2025).map(String::as_str),
        Some("TX")
    );
    // Without documents, payroll alone still finds both jobs, as inferred.
    let inferred = employment::employers(&[], &state.snapshot.transactions, career::today());
    assert_eq!(inferred.len(), 2);
    assert!(inferred.iter().all(|employer| employer.inferred));
}

#[test]
fn descriptors_name_employers_by_word_prefix_or_whole_name() {
    assert!(employment::names(
        "EXAMPLE STATE UNIV PAYROLL",
        "Example State University"
    ));
    assert!(employment::names(
        "BOOK TRANSFER B/O SAMPLE GLOBAL INC",
        "Sample Global, Inc."
    ));
    assert!(employment::names(
        "EXAMPLE CORP SERV PAYROLL",
        "Example Corp Services Group Inc."
    ));
    // One short word, or a different employer, is not a match.
    assert!(!employment::names(
        "EXA PAYROLL",
        "Example State University"
    ));
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
                Some(day("2026-02-06"))
            ),
            (
                "home:tx:2025-01",
                day("2025-01-01"),
                Some(day("2026-02-28"))
            ),
            (
                "milestone:purchase:furniture",
                day("2025-11-03"),
                Some(day("2025-11-03"))
            ),
            ("home:wa:2026-03", day("2026-03-01"), None),
            (
                "work:example-corp-services-group-inc",
                day("2026-03-02"),
                None
            ),
            (
                "milestone:accounts:2026-04",
                day("2026-04-15"),
                Some(day("2026-05-15"))
            ),
        ]
    );
    assert!(evidence
        .chapters
        .iter()
        .all(|chapter| chapter.tier == Tier::Suggested && !chapter.confirmed));
    let corp = &evidence.chapters[4];
    assert_eq!(corp.programs, vec![WAIVER.to_owned()]);
    // Accounts opened near the start are only suggested members.
    assert!(corp
        .members
        .iter()
        .any(|member| member.id == "account:plaid:premier" && member.tier == Tier::Suggested));
    // The Chicago trip says nothing about where home is.
    assert!(!evidence
        .chapters
        .iter()
        .any(|chapter| chapter.state.as_deref() == Some("IL")));
    assert_eq!(
        evidence.now,
        chapters::Now {
            work: Some("work:example-corp-services-group-inc".into()),
            work_title: Some("Example Corp Services Group Inc.".into()),
            home: Some("Washington".into()),
            month: Some(8),
        }
    );
    let residency = evidence
        .residency
        .iter()
        .map(|span| (span.year, span.state.as_str(), span.days))
        .collect::<Vec<_>>();
    assert_eq!(
        residency,
        vec![(2025, "TX", 365), (2026, "TX", 59), (2026, "WA", 231)]
    );
}

#[test]
fn a_jobs_end_lists_what_changes() {
    // When payroll stops, the job's chapter ends and its fee waiver with it.
    let mut state = career::state();
    let cutoff = "2026-08-01";
    state.snapshot.transactions.retain(|transaction| {
        !(transaction.id.starts_with("corp-pay") && transaction.date.as_str() >= cutoff)
    });
    state
        .documents
        .retain(|document| document.date.as_deref().is_none_or(|date| date < cutoff));
    let view = career_view(&state);
    let corp = evidence_of(&view)
        .chapters
        .iter()
        .find(|chapter| chapter.id == "work:example-corp-services-group-inc")
        .unwrap();
    assert_eq!(corp.end, Some(day("2026-07-31")));
    assert!(
        corp.checklist
            .iter()
            .any(|item| item.contains("employee waiver lapses")),
        "{:?}",
        corp.checklist
    );
    assert!(corp
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
            id: "work:example-corp-services-group-inc".into(),
            status: ChapterStatus::Confirmed,
            title: Some("Example Corp".into()),
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
    let corp = evidence
        .chapters
        .iter()
        .find(|chapter| chapter.id == "work:example-corp-services-group-inc")
        .unwrap();
    assert_eq!(
        (corp.title.as_str(), corp.tier, corp.confirmed),
        ("Example Corp", Tier::Observed, true)
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
        Some(day("2026-03-02"))
    );
    assert_eq!(
        insights.plan.baseline_since, None,
        "March 2 is more than three months back"
    );
    assert_eq!(evidence.now.work_title.as_deref(), Some("Example Corp"));
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
    for program in programs::programs(&career::state().settings.programs) {
        program.validate().unwrap();
        assert!(
            !program.stale(career::today()),
            "{} needs its source re-read",
            program.id
        );
    }
    // Chapters and programs are objects, linked to what defines and depends on them.
    assert!(ids.contains("chapter:work:example-corp-services-group-inc"));
    assert!(ids.contains(format!("program:{WAIVER}").as_str()));
    assert!(graph.links.iter().any(|link| {
        link.from == "chapter:work:example-corp-services-group-inc"
            && link.relation == crate::objects::Relation::Program
            && link.to == format!("program:{WAIVER}")
    }));
    // A member only timing suggests is not a link; one a record shows is.
    let member_of = |chapter: &str| {
        graph.links.iter().any(|link| {
            link.from == "account:plaid:premier"
                && link.relation == crate::objects::Relation::Chapter
                && link.to == chapter
        })
    };
    assert!(!member_of("chapter:work:example-corp-services-group-inc"));
    assert!(member_of("chapter:milestone:accounts:2026-04"));
}

#[test]
fn rules_are_checked_before_use() {
    let bonus = career::loyalty_bonus();
    let mut broken = bonus.clone();
    broken.source.url = "http://example.com".into();
    assert!(broken.validate().is_err());
    let mut unbound = bonus.clone();
    unbound.conclusion.object = Some("nothing".into());
    assert!(unbound.validate().is_err());
    let mut pattern = bonus.clone();
    if let programs::Condition::Transaction { description, .. } = &mut pattern.conditions[1] {
        *description = "(".into();
    }
    assert!(pattern.validate().is_err());
    // An approved rule that fails its checks is left out.
    let approved = programs::programs(&[bonus.clone(), broken]);
    assert_eq!(approved, vec![bonus.clone()]);
    let mut old = bonus.clone();
    old.reviewed = day("2025-01-01");
    assert!(old.stale(day("2026-10-17")));
}

/// Every ledger tool's result for the career person, for writing the eval set by hand.
#[test]
#[ignore]
fn career_tools() {
    let view = career_view(&career::state());
    let ledger = crate::insights::assistant_tools::Ledger::new(&view, career::today());
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
            serde_json::json!({ "text": "loyalty bonus" }),
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
            serde_json::json!({ "name": "since joining Example Corp" }),
        ),
    ] {
        println!(
            "\n## {name} {arguments}\n{}",
            serde_json::to_string_pretty(&ledger.call(name, &arguments)).unwrap()
        );
    }
}
