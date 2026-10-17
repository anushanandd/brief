use chrono::TimeZone;
use serde_json::json;

use super::*;
use crate::{evidence::programs::programs, objects::Tier};

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn account(id: &str, kind: &str, name: &str, institution: &str, official: Option<&str>) -> Account {
    serde_json::from_value(json!({
        "id": id, "name": name, "institution": institution, "type": kind, "value": 1.0,
        "currency": "USD", "officialName": official,
    }))
    .unwrap()
}

fn work(title: &str) -> Chapter {
    Chapter {
        id: format!("work:{title}"),
        kind: ChapterKind::Work,
        title: title.into(),
        start: day("2026-01-05"),
        end: None,
        precision: crate::evidence::employment::Precision::Day,
        tier: Tier::Suggested,
        confirmed: false,
        state: None,
        anchors: Vec::new(),
        members: Vec::new(),
        programs: Vec::new(),
        effects: Vec::new(),
        checklist: Vec::new(),
        planned_costs: Vec::new(),
    }
}

#[test]
fn public_names_drop_what_records_add() {
    assert_eq!(
        public_name("Platinum CashPlus - 8549").as_deref(),
        Some("Platinum CashPlus")
    );
    assert_eq!(
        public_name("Morgan Stanley Platinum Card®").as_deref(),
        Some("Morgan Stanley Platinum Card")
    );
    assert_eq!(
        public_name("ACME 401K Plan #12 Acme").as_deref(),
        Some("ACME Plan")
    );
    assert_eq!(public_name("1234 5678"), None);
    assert_eq!(public_name("X"), None);
}

#[test]
fn gaps_are_what_no_rule_covers_and_queries_carry_no_digits() {
    let accounts = [
        // The card's benefits are known, and Morgan Stanley's programs cover the account.
        account(
            "a",
            "credit",
            "Card",
            "American Express",
            Some("Morgan Stanley Platinum Card®"),
        ),
        account(
            "b",
            "cash",
            "Platinum CashPlus - 8549",
            "Morgan Stanley",
            None,
        ),
        // Neither of these has a rule.
        account(
            "c",
            "credit",
            "Card 4421",
            "Chase",
            Some("Sapphire Reserve 4421"),
        ),
        account(
            "d",
            "cash",
            "Everyday Checking ...0042",
            "Wells Fargo",
            Some("Everyday Checking"),
        ),
        account("e", "investment", "Brokerage", "Fidelity", None),
    ];
    let chapters = [
        work("Morgan Stanley Services Group Inc"),
        work("University of Denver"),
    ];
    let gaps = gaps(&accounts, &chapters, &programs(&[]));
    let found = gaps
        .iter()
        .map(|gap| (gap.kind, gap.name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        vec![
            (GapKind::Account, "Wells Fargo Everyday Checking"),
            (GapKind::Card, "Sapphire Reserve"),
            (GapKind::Employer, "University of Denver"),
        ]
    );
    for gap in &gaps {
        // Template words aside, nothing in a query came with a digit.
        assert!(
            !gap.query.contains("4421") && !gap.query.contains("0042"),
            "{}",
            gap.query
        );
        assert!(gap.query.starts_with(&gap.name));
    }
}

fn waiver() -> Draft {
    Draft {
        template: Template::EmployeeDepositWaiver,
        name: "Checking employee waiver".into(),
        organization: "Example Corp".into(),
        summary: "Employees with enough deposits pay no monthly fee.".into(),
        source_url: "https://bank.example/employees".into(),
        source_title: "Employee banking".into(),
        effective: day("2026-01-01"),
        account_name: "Premier Checking".into(),
        institution: "Example Bank".into(),
        card_name: None,
        bonus_words: None,
        amount: 2_500.0,
        fee: Some(25.0),
        unobservable: Some("Paperless statements".into()),
    }
}

#[test]
fn drafts_become_valid_rules_matching_plain_words() {
    let program = draft(&waiver(), day("2026-10-17")).unwrap();
    assert_eq!(program.id, "approved-example-corp-premier-checking-waiver");
    assert_eq!(program.reviewed, day("2026-10-17"));
    assert_eq!(
        program.values("deposits", day("2026-03-01")),
        Some(&[2_500.0][..])
    );
    // Before the terms begin, nothing is covered.
    assert_eq!(program.values("deposits", day("2025-12-31")), None);
    assert!(matches!(
        program.conditions.last(),
        Some(Condition::Unobservable { .. })
    ));

    // A name with pattern characters matches itself, not a pattern.
    let mut bonus = waiver();
    bonus.template = Template::BonusOffsetsFee;
    bonus.card_name = Some("Card (Plus)+".into());
    bonus.bonus_words = Some("engagement bonus".into());
    let program = draft(&bonus, day("2026-10-17")).unwrap();
    let Condition::Card { official_name, .. } = &program.conditions[1] else {
        panic!("card condition");
    };
    let pattern = crate::evidence::programs::matcher(official_name).unwrap();
    assert!(pattern.is_match("The Card (Plus)+ by Example"));
    assert!(!pattern.is_match("Card Plus"));
}

#[test]
fn drafts_are_refused_when_incomplete_or_uncited() {
    let today = day("2026-10-17");
    let mut plain = waiver();
    plain.source_url = "http://bank.example".into();
    assert!(draft(&plain, today).is_err());
    let mut future = waiver();
    future.effective = day("2026-11-01");
    assert!(draft(&future, today).is_err());
    let mut free = waiver();
    free.fee = None;
    assert!(draft(&free, today).is_err());
    let mut zero = waiver();
    zero.amount = 0.0;
    assert!(draft(&zero, today).is_err());
    let mut bonus = waiver();
    bonus.template = Template::BonusOffsetsFee;
    assert!(draft(&bonus, today).is_err());
}

#[test]
fn sources_are_rechecked_monthly_and_changes_stay_flagged() {
    let first = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
    assert!(due(None, first));
    let check = checked(None, "https://bank.example", "Fee  is $55", first);
    assert_eq!(check.changed_at, None);
    assert!(!due(Some(&check), first + chrono::Duration::days(29)));
    let later = first + chrono::Duration::days(30);
    assert!(due(Some(&check), later));
    // Spacing and case alone aren't a change.
    let same = checked(Some(&check), "https://bank.example", "fee is $55", later);
    assert_eq!(same.changed_at, None);
    let changed = checked(Some(&same), "https://bank.example", "Fee is $65", later);
    assert_eq!(changed.changed_at, Some(later.to_rfc3339()));
    let after = later + chrono::Duration::days(30);
    let kept = checked(Some(&changed), "https://bank.example", "Fee is $65", after);
    assert_eq!(kept.changed_at, changed.changed_at);
}

#[test]
fn entries_record_what_came_back_or_why_not() {
    let now = Utc.with_ymd_and_hms(2026, 10, 17, 9, 0, 0).unwrap();
    let mut entry = Entry::begin(
        EntryKind::Search,
        "Example query".into(),
        Some("card:x".into()),
        now,
    );
    assert_eq!(entry.status, Status::Running);
    assert_eq!(entry.id.len(), 16);
    entry.finish(Err("Exa rejected the API key".into()), now);
    assert_eq!(entry.status, Status::Failed);
    assert_eq!(entry.error.as_deref(), Some("Exa rejected the API key"));
}
