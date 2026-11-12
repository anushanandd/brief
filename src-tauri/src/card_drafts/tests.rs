use std::collections::BTreeMap;

use chrono::NaiveDate;

use super::*;
use crate::finance_contract::{Account, Transaction};
use crate::insights::benefits::{Alignment, Cadence, RuleOrigin, RuleSet};
use crate::providers::card_catalog::parse_catalog;
use crate::providers::exa::Page;
use crate::research::{Gap, GapKind, Template};
use crate::settings::FinanceSettings;

const CATALOG: &str = r#"{
  "schemaVersion": 2,
  "cards": [
    { "id": "test-rewards", "name": "Test Rewards Card", "issuer": "Test Bank", "annualFee": 250,
      "url": "https://bank.example/rewards", "lastVerified": "2026-01-01",
      "benefits": [
        { "id": "streaming", "name": "Streaming credit", "type": "credit", "value": 10,
          "frequency": "monthly", "alignment": "calendar", "matchKeywords": ["streaming"],
          "descriptorExamples": ["TEST BANK STREAMING CREDIT"], "matchVerified": true },
        { "id": "dining", "name": "Dining credit", "type": "credit", "value": 50,
          "frequency": "quarterly", "alignment": "calendar", "matchKeywords": ["bistro"] },
        { "id": "travel", "name": "Travel credit", "type": "credit", "value": 100,
          "frequency": "annual", "alignment": "anniversary", "matchKeywords": ["travel"],
          "enrollmentRequired": true },
        { "id": "partner", "name": "Partner cash", "type": "credit", "value": 15,
          "frequency": "monthly", "alignment": "calendar", "inApp": true, "matchKeywords": ["partner"] },
        { "id": "renewal", "name": "Trusted traveler", "type": "credit", "value": 100,
          "frequency": "custom", "intervalMonths": 48, "matchKeywords": ["trusted traveler"] },
        { "id": "lounge", "name": "Lounge access", "type": "perk", "value": null, "frequency": "annual" },
        { "id": "weekly", "name": "Weekly thing", "type": "credit", "value": 5, "frequency": "weekly" }
      ] },
    { "id": "test-business", "name": "Test Rewards Business Card", "issuer": "Test Bank",
      "isBusiness": true, "benefits": [] },
    { "id": "test-old", "name": "Test Rewards Card", "issuer": "Test Bank", "discontinued": true,
      "benefits": [] },
    { "id": "other-card", "name": "Other Cash Card", "issuer": "Other Bank", "benefits": [] }
  ]
}"#;

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn account() -> Account {
    Account {
        id: "card".into(),
        name: "Rewards ...1234".into(),
        institution: "Test Bank".into(),
        r#type: "credit".into(),
        official_name: Some("Test Rewards Card".into()),
        ..Default::default()
    }
}

fn drafted() -> CardDraft {
    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    catalog::draft(
        catalog.card("test-rewards").unwrap(),
        Some(&account()),
        day("2026-03-01"),
    )
}

fn credit(id: &str, merchant: &str, amount: f64) -> Transaction {
    Transaction {
        id: id.into(),
        merchant: merchant.into(),
        amount,
        date: "2026-02-10".into(),
        account_id: Some("card".into()),
        ..Default::default()
    }
}

fn benefit<'a>(draft: &'a CardDraft, id: &str) -> &'a BenefitDraft {
    draft
        .benefits
        .iter()
        .find(|benefit| benefit.id == id)
        .unwrap()
}

fn term<'a>(draft: &'a CardDraft, id: &str, field: &str) -> &'a TermCheck {
    benefit(draft, id)
        .terms
        .iter()
        .find(|term| term.field == field)
        .unwrap()
}

#[test]
fn the_catalogs_brands_are_every_keyword_and_name_without_generic_words() {
    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    assert_eq!(
        catalog::brands(&catalog),
        [
            "access",
            "bistro",
            "partner",
            "trusted traveler",
            "weekly thing"
        ]
    );
}

#[test]
fn an_account_matches_its_current_card_by_the_names_plaid_reports() {
    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    let found = catalog::candidates(&catalog, &account());
    assert_eq!(
        found.first().map(|card| card.id.as_str()),
        Some("test-rewards")
    );
    assert!(found
        .iter()
        .all(|card| card.id != "test-business" && card.id != "test-old"));
    let searched = catalog::search(&catalog, "other cash");
    assert_eq!(
        searched.first().map(|card| card.id.as_str()),
        Some("other-card")
    );
    assert!(catalog::search(&catalog, "the card").is_empty());
}

#[test]
fn each_catalog_credit_becomes_a_rule_and_perks_are_left_out() {
    let draft = drafted();
    let rules = &draft.rules;
    let ids = rules
        .benefits
        .iter()
        .map(|rule| rule.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["streaming", "dining", "travel", "partner", "renewal"]);
    // Every drafted benefit is a rule; the perk and the weekly credit become warnings instead.
    assert!(draft
        .benefits
        .iter()
        .all(|benefit| ids.contains(&benefit.id.as_str())));
    assert_eq!(draft.warnings.len(), 2);
    let rule = |id: &str| rules.benefits.iter().find(|rule| rule.id == id).unwrap();
    assert_eq!(
        (rule("streaming").cadence, rule("streaming").cap),
        (Cadence::Monthly, 10.0)
    );
    assert!(rule("streaming").credit_matches.is_some());
    assert_eq!(rule("travel").alignment, Alignment::Anniversary);
    assert!(rule("travel").instruction.starts_with("Enroll first."));
    assert!(!rule("partner").issuer_credits && rule("partner").estimate_from_purchases);
    assert_eq!(rule("renewal").cadence, Cadence::Renewal);
    assert_eq!(rule("renewal").renewal_terms[0].months, 48);
    let reference = rules.catalog.as_ref().unwrap();
    assert_eq!(
        (reference.id.as_str(), reference.hash.len()),
        ("test-rewards", 64)
    );
    assert_eq!(rules.annual_fee, Some(250.0));
    let compiled = RuleSet::new(rules.clone()).unwrap();
    assert!(compiled.names("Test Rewards Card"));
    assert_eq!(
        term(&draft, "streaming", "cap").confidence,
        Confidence::Suggested
    );
    assert_eq!(
        term(&draft, "dining", "creditMatches").confidence,
        Confidence::Unconfirmed
    );
}

#[test]
fn posted_credits_confirm_a_benefit_and_propose_wording_the_catalog_lacked() {
    let mut draft = drafted();
    history::observe(
        &mut draft,
        &[
            credit("a", "TEST BANK STREAMING CREDIT", 10.0),
            credit("b", "TEST BANK BISTRO CREDIT 0412", 50.0),
            credit("c", "TEST BANK BISTRO CREDIT 0513", 60.0),
            credit("d", "MYSTERY CREDIT", 5.0),
            Transaction {
                account_id: Some("other".into()),
                ..credit("e", "TEST BANK STREAMING CREDIT", 10.0)
            },
            Transaction {
                website: Some("https://www.bistro.example.com".into()),
                ..credit("f", "BISTRO NORTH", -30.0)
            },
        ],
    );
    let logo = term(&draft, "dining", "logoDomain");
    assert_eq!(
        (logo.value.as_str(), logo.confidence),
        ("bistro.example.com", Confidence::Observed)
    );
    let rule = |id: &str| {
        draft
            .rules
            .benefits
            .iter()
            .find(|rule| rule.id == id)
            .unwrap()
    };
    assert_eq!(
        rule("dining").logo_domain.as_deref(),
        Some("bistro.example.com")
    );
    assert_eq!(rule("streaming").logo_domain, None);
    assert_eq!(benefit(&draft, "streaming").credits_seen, 1);
    assert_eq!(
        term(&draft, "streaming", "creditMatches").confidence,
        Confidence::Observed
    );
    assert_eq!(
        term(&draft, "streaming", "cap").confidence,
        Confidence::Observed
    );
    assert_eq!(benefit(&draft, "dining").credits_seen, 2);
    let proposed = draft
        .rules
        .benefits
        .iter()
        .find(|rule| rule.id == "dining")
        .unwrap();
    let wording = regex::Regex::new(&format!(
        "(?i){}",
        proposed.credit_matches.as_ref().unwrap()
    ))
    .unwrap();
    assert!(wording.is_match("TEST BANK BISTRO CREDIT 0999"));
    assert_eq!(draft.unmatched_credits, ["MYSTERY CREDIT"]);
    assert!(draft.warnings.iter().any(|warning| warning.contains("$60")));
    assert!(draft.benefits.iter().all(|benefit| draft
        .rules
        .benefits
        .iter()
        .any(|rule| rule.id == benefit.id)));
}

const TERMS: &str = "Test Rewards Card benefits. Earn up to $10 in statement credits each month on eligible streaming subscriptions. Get up to $25 back each quarter at participating bistro restaurants. Receive a $100 travel credit each cardmember year after enrollment. Spend up to $15 every month in our partner's app.";

#[test]
fn the_terms_explain_or_contradict_each_drafted_term() {
    let mut draft = drafted();
    terms::apply(
        &mut draft,
        "https://bank.example/rewards",
        TERMS,
        &BTreeMap::new(),
        day("2026-03-01"),
    );
    let streaming = term(&draft, "streaming", "cap");
    assert_eq!(streaming.confidence, Confidence::Explained);
    assert!(draft.sentences[streaming.sentence.unwrap()].contains("$10"));
    let dining = term(&draft, "dining", "cap");
    assert_eq!(dining.confidence, Confidence::Conflict);
    assert_eq!(dining.note.as_deref(), Some("The terms say $25."));
    assert_eq!(
        term(&draft, "dining", "cadence").confidence,
        Confidence::Explained
    );
    assert_eq!(
        term(&draft, "travel", "alignment").confidence,
        Confidence::Explained
    );
    assert_eq!(
        term(&draft, "renewal", "cap").confidence,
        Confidence::Suggested
    );
    assert!(draft.rules.terms_hash.is_some());
    let streaming_rule = draft
        .rules
        .benefits
        .iter()
        .find(|rule| rule.id == "streaming")
        .unwrap();
    assert!(streaming_rule
        .sources
        .iter()
        .any(|source| source.kind == SourceKind::Terms));
}

#[test]
fn a_model_pick_counts_only_when_rust_reads_its_amount_and_period() {
    let mut draft = drafted();
    let text = "These are the renewal terms for the card. You may receive $100 every four years for an application fee. A fee credit applies to the application.";
    let sentences = crate::providers::reading::sentences(text);
    assert_eq!(terms::verified(&sentences, &[1, 2, 9]), [1]);
    let picks = BTreeMap::from([("renewal".to_owned(), vec![1])]);
    terms::apply(
        &mut draft,
        "https://bank.example/renewal",
        text,
        &picks,
        day("2026-03-01"),
    );
    assert_eq!(
        term(&draft, "renewal", "cap").confidence,
        Confidence::Explained
    );
    let unverified = BTreeMap::from([("renewal".to_owned(), vec![2])]);
    let mut other = drafted();
    terms::apply(
        &mut other,
        "https://bank.example/renewal",
        text,
        &unverified,
        day("2026-03-01"),
    );
    assert_eq!(
        term(&other, "renewal", "cap").confidence,
        Confidence::Suggested
    );
}

#[test]
fn a_recheck_lists_each_term_that_changed() {
    let saved = drafted().rules;
    let mut fresh = saved.clone();
    fresh.annual_fee = Some(300.0);
    fresh.benefits.retain(|rule| rule.id != "renewal");
    fresh.benefits[0].cap = 15.0;
    let mut added = fresh.benefits[1].clone();
    added.id = "new".into();
    added.name = "New credit".into();
    fresh.benefits.push(added);
    let changes = diff(&saved, &fresh);
    let found = |field: &str, benefit: &str| {
        changes
            .iter()
            .find(|change| change.field == field && change.benefit == benefit)
            .cloned()
    };
    assert_eq!(
        found("annualFee", "Test Rewards Card")
            .unwrap()
            .after
            .as_deref(),
        Some("$300")
    );
    let cap = found("cap", "Streaming credit").unwrap();
    assert_eq!(
        (cap.before.as_deref(), cap.after.as_deref()),
        (Some("$10"), Some("$15"))
    );
    assert!(found("benefit", "Trusted traveler")
        .unwrap()
        .after
        .is_none());
    assert!(found("benefit", "New credit").unwrap().before.is_none());
    assert!(diff(&saved, &saved).is_empty());
}

#[test]
fn a_program_draft_takes_only_the_amounts_a_sentence_states() {
    let page = |text: &str| Page {
        url: "https://bank.example/terms".into(),
        title: "Account terms".into(),
        text: text.into(),
        published: None,
    };
    let account = Gap {
        id: "account:example".into(),
        kind: GapKind::Account,
        name: "Example Checking".into(),
        query: String::new(),
    };
    let waiver = programs::prefill(
        &account,
        &page("Example Checking has a $12 monthly service fee. The fee is waived with $500 in qualifying direct deposits each month."),
        day("2026-03-01"),
    );
    assert_eq!(waiver.draft.template, Template::EmployeeDepositWaiver);
    assert_eq!((waiver.draft.fee, waiver.draft.amount), (Some(12.0), 500.0));
    assert_eq!(waiver.quotes.len(), 2);
    let card = Gap {
        kind: GapKind::Card,
        ..account.clone()
    };
    let bonus = programs::prefill(
        &card,
        &page("Earn a $200 bonus each year after renewal."),
        day("2026-03-01"),
    );
    assert_eq!(
        (bonus.draft.template, bonus.draft.amount),
        (Template::BonusOffsetsFee, 200.0)
    );
    let empty = programs::prefill(
        &account,
        &page("Nothing here states an amount at all, sadly."),
        day("2026-03-01"),
    );
    assert_eq!((empty.draft.amount, empty.draft.fee), (0.0, None));
    assert!(empty.quotes.is_empty());
}

fn applied(settings: &FinanceSettings, accounts: &[Account]) -> FinanceSettings {
    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    apply::apply(settings, accounts, &[], &catalog, day("2026-03-01"))
}

#[test]
fn a_clear_match_applies_its_catalog_rules() {
    let checking = Account {
        id: "checking".into(),
        r#type: "depository".into(),
        official_name: Some("Other Cash Card".into()),
        ..Default::default()
    };
    let settings = applied(&FinanceSettings::default(), &[account(), checking]);
    let [rules] = settings.card_rules.as_slice() else {
        panic!("one product applied");
    };
    assert_eq!(rules.product, "test-rewards");
    assert_eq!(
        (rules.origin, rules.applied),
        (RuleOrigin::Catalog, Some(day("2026-03-01")))
    );
    assert_eq!(
        settings.benefit_cards.get("card").map(String::as_str),
        Some("test-rewards")
    );
    assert!(settings.card_choices.is_empty());
    assert_eq!(applied(&settings, &[account()]), settings);
    settings.clone().normalized().unwrap();
}

#[test]
fn a_close_match_waits_for_the_user_to_choose() {
    let unnamed = Account {
        official_name: None,
        institution: "Example Bank".into(),
        ..account()
    };
    let settings = applied(&FinanceSettings::default(), std::slice::from_ref(&unnamed));
    assert!(settings.card_rules.is_empty());
    let [choice] = settings.card_choices.as_slice() else {
        panic!("one choice");
    };
    assert_eq!(choice.account_id, "card");
    assert_eq!(choice.candidates[0].id, "test-rewards");
    settings.clone().normalized().unwrap();

    let twins = parse_catalog(
        br#"{ "cards": [
            { "id": "alpha-gold", "name": "Gold Card", "issuer": "Alpha", "benefits": [] },
            { "id": "beta-gold", "name": "Gold Card", "issuer": "Beta", "benefits": [] }
        ] }"#,
    )
    .unwrap();
    let gold = Account {
        official_name: Some("Gold Card".into()),
        institution: "Example".into(),
        ..account()
    };
    let apply::Match::Close(cards) = apply::best(&twins, &gold) else {
        panic!("two cards with one name are a close call");
    };
    assert_eq!(cards.len(), 2);

    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    let mut chosen = settings;
    assert!(apply::adopt(
        &mut chosen,
        &unnamed,
        catalog.card("test-rewards").unwrap(),
        &[],
        day("2026-03-01"),
    ));
    assert_eq!(chosen.card_rules[0].origin, RuleOrigin::Catalog);
    assert!(chosen.card_choices.is_empty());
}

#[test]
fn the_users_rules_are_never_changed() {
    let mut rules = drafted().rules;
    rules.product = "mine".into();
    rules.official_names = None;
    rules.benefits[0].cap = 12.0;
    rules.catalog.as_mut().unwrap().hash = "older".into();
    let spending = Account {
        id: "spending".into(),
        name: "Everyday ...9".into(),
        institution: "Other Bank".into(),
        r#type: "credit".into(),
        ..Default::default()
    };
    let settings = FinanceSettings {
        card_rules: vec![rules.clone()],
        spending_account_id: Some("spending".into()),
        ..Default::default()
    };
    let next = applied(&settings, &[spending, account()]);
    assert_eq!(next.card_rules[0], rules);
    assert_eq!(next.card_rules[1].origin, RuleOrigin::Catalog);
    // The spending card read the only product; adding another must not end that.
    assert_eq!(
        next.benefit_cards.get("spending").map(String::as_str),
        Some("mine")
    );
    assert!(!diff(&rules, &drafted().rules).is_empty());
}

#[test]
fn a_catalog_change_updates_catalog_rules_and_dates_the_new_allowance() {
    let mut rules = drafted().rules;
    rules.origin = RuleOrigin::Catalog;
    rules.applied = Some(day("2026-01-01"));
    rules.benefits[0].cap = 8.0;
    rules.catalog.as_mut().unwrap().hash = "older".into();
    let settings = FinanceSettings {
        card_rules: vec![rules],
        benefit_cards: BTreeMap::from([("card".into(), "test-rewards".into())]),
        ..Default::default()
    };
    let next = applied(&settings, &[account()]);
    let updated = &next.card_rules[0];
    let catalog = parse_catalog(CATALOG.as_bytes()).unwrap();
    assert_eq!(
        updated.catalog.as_ref().unwrap().hash,
        catalog.card("test-rewards").unwrap().hash
    );
    assert_eq!(updated.applied, Some(day("2026-03-01")));
    let streaming = &updated.benefits[0];
    assert_eq!(streaming.cap, 8.0);
    assert_eq!(
        streaming
            .allowance_changes
            .iter()
            .map(|change| (change.from, change.cap))
            .collect::<Vec<_>>(),
        [(day("2026-03-01"), 10.0)]
    );
    assert!(updated.benefits[1].allowance_changes.is_empty());
    assert_eq!(applied(&next, &[account()]), next);
    // Another catalog change that leaves this allowance alone adds no second change.
    let mut again = next.clone();
    again.card_rules[0].catalog.as_mut().unwrap().hash = "older".into();
    assert_eq!(
        applied(&again, &[account()]).card_rules[0].benefits[0].allowance_changes,
        streaming.allowance_changes
    );
}

#[test]
fn the_users_rules_gain_only_missing_categories_from_the_catalog() {
    let catalog = parse_catalog(
        br#"{ "cards": [
            { "id": "test-rewards", "name": "Test Rewards Card", "issuer": "Test Bank",
              "benefits": [
                { "id": "streaming", "name": "Streaming credit", "category": "Entertainment",
                  "value": 10, "frequency": "monthly" },
                { "id": "table", "name": "Table credit", "category": "dining", "value": 50,
                  "frequency": "quarterly", "matchKeywords": ["bistro"] },
                { "id": "trips", "name": "Trip credit", "category": "airline", "value": 100,
                  "frequency": "annual" }
              ] }
        ] }"#,
    )
    .unwrap();
    let mut rules = drafted().rules;
    rules.catalog = None;
    rules.product = "my-rewards".into();
    // Matched by name, by keyword, and one the user already categorized.
    rules.benefits[1].id = "my-dining".into();
    rules.benefits[2].category = Some("travel".into());
    let filled = apply::with_display_details(&rules, &catalog).unwrap();
    let categories = filled
        .benefits
        .iter()
        .map(|rule| rule.category.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        categories[..3],
        [Some("entertainment"), Some("dining"), Some("travel")]
    );
    let terms = |rules: &CardRules| {
        rules
            .benefits
            .iter()
            .map(|rule| {
                (
                    rule.id.clone(),
                    rule.cap,
                    rule.cadence,
                    rule.matches.clone(),
                    rule.credit_matches.clone(),
                    rule.allowance_changes.len(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(terms(&filled), terms(&rules));
    assert_eq!(
        (filled.annual_fee, filled.origin, &filled.catalog),
        (rules.annual_fee, RuleOrigin::User, &rules.catalog)
    );
    assert!(apply::with_display_details(&filled, &catalog).is_none());

    let settings = FinanceSettings {
        card_rules: vec![rules],
        benefit_cards: BTreeMap::from([("card".into(), "my-rewards".into())]),
        ..Default::default()
    };
    let next = apply::apply(&settings, &[account()], &[], &catalog, day("2026-03-01"));
    assert_eq!(next.card_rules, [filled]);
}

#[test]
fn with_the_catalog_off_nothing_is_fetched_or_applied() {
    let settings = FinanceSettings {
        card_catalog_off: true,
        ..Default::default()
    };
    assert!(!apply::wanted(&settings, &[account()]));
    assert!(apply::wanted(&FinanceSettings::default(), &[account()]));
    assert!(!apply::wanted(&FinanceSettings::default(), &[]));
    assert_eq!(applied(&settings, &[account()]), settings);
}
