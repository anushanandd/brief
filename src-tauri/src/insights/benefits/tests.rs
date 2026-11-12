use chrono::NaiveDate;
use serde_json::{json, Value};

use super::history::{BenefitPeriod, PeriodState, PlannedCredit, PlannedSource, ReviewKind};
use super::*;
use crate::insights::calendar::NamedZone;

/// A card with only the rules a test exercises.
fn test_rules(benefits: Value) -> CardRules {
    serde_json::from_value(json!({
        "product": "test-card",
        "name": "Test Card",
        "version": "1",
        "termsReviewed": "Jan 1, 2025",
        "annualFee": 450,
        "annualFeeMatches": "annual membership fee|annual fee",
        "officialNames": "^(?:the\\s+)?test card\\b",
        "officialNameExcludes": "business",
        "issuerCreditMatches": "^\\s*test card\\b.*\\b(?:credit|reimbursement)\\b",
        "benefits": benefits,
    }))
    .unwrap()
}

fn card_with(benefits: Value) -> RuleSet {
    RuleSet::new(test_rules(benefits)).unwrap()
}

/// A benefit of `cap` each `cadence` for purchases matching `matches`, credited by
/// "`id` … credit"; `extra` sets anything else.
fn rule(id: &str, cadence: &str, cap: f64, matches: &str, extra: Value) -> Value {
    let mut rule = json!({
        "id": id,
        "name": id,
        "limit": format!("${cap}"),
        "cap": cap,
        "cadence": cadence,
        "matches": matches,
        "creditMatches": format!("{id}.*credit"),
        "instruction": "",
        "url": "https://example.com",
        "effectiveFrom": "2025-01-01",
    });
    for (key, value) in extra.as_object().unwrap() {
        rule[key] = value.clone();
    }
    rule
}

fn streaming() -> Value {
    rule(
        "streaming",
        "monthly",
        15.0,
        "streaming|example tv|example news|example music",
        json!({ "recurring": true }),
    )
}

fn dining() -> Value {
    rule("dining", "quarterly", 50.0, "example tables", json!({}))
}

/// Terms that name no merchant, like a catalog entry's.
fn entertainment() -> Value {
    rule(
        "entertainment",
        "monthly",
        25.0,
        "digital entertainment",
        json!({ "category": "entertainment" }),
    )
}

/// Annual, then half-yearly from Sep 1, 2025.
fn hotel() -> Value {
    rule(
        "hotel",
        "annual",
        150.0,
        "hotel\\s?credit|example hotels",
        json!({
            "creditMatches": "hotel\\s?credit",
            "timeZone": "America/Chicago",
            "postingDelayDays": 90,
            "allowanceChanges": [
                { "from": "2025-09-01", "cap": 200, "limit": "$200 / half-year", "cadence": "semiannual" }
            ],
        }),
    )
}

fn travel_fee() -> Value {
    rule(
        "travel-fee",
        "annual",
        150.0,
        "travel fee (?:credit|reimbursement)|checked bag",
        json!({ "creditMatches": "travel fee (?:credit|reimbursement)" }),
    )
}

/// Retired after June 2026.
fn store() -> Value {
    rule(
        "store",
        "semiannual",
        40.0,
        "example store",
        json!({
            "name": "Example Store",
            "creditMatches": "example store.*(?:credit|reimbursement|benefit)",
            "effectiveUntil": "2026-06-30",
        }),
    )
}

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn categorized(id: &str, date: &str, merchant: &str, amount: f64, category: &str) -> Transaction {
    Transaction {
        category: category.into(),
        ..txn(id, date, merchant, amount)
    }
}

fn txn(id: &str, date: &str, merchant: &str, amount: f64) -> Transaction {
    serde_json::from_value(json!({
        "id": id,
        "merchant": merchant,
        "category": "Shopping",
        "date": date,
        "amount": amount,
        "account": "Test Card",
        "pending": false,
        "description": null,
    }))
    .unwrap()
}

/// A card transaction as Plaid reports it: dated when the merchant processed it, posted later.
fn dated(id: &str, occurred: &str, posted: &str, merchant: &str, amount: f64) -> Transaction {
    let mut transaction = txn(id, occurred, merchant, amount);
    transaction.occurred_on = Some(occurred.into());
    transaction.posted_on = Some(posted.into());
    transaction
}

fn noon(date: &str) -> String {
    format!("{date}T12:00:00Z")
}

fn window(card: &RuleSet, transactions: &[Transaction], today: &str, id: &str) -> BenefitWindow {
    let refs = transactions.iter().collect::<Vec<_>>();
    card.tracker(&refs, &noon(today), &noon(today), &NamedZone::Utc)
        .into_iter()
        .find(|window| window.id == id)
        .unwrap()
}

fn history(card: &RuleSet, transactions: &[Transaction], today: &str) -> Vec<BenefitHistory> {
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = card.activity(&refs, &noon(today));
    let start = refs
        .iter()
        .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
        .min();
    card.history(&matched, start, day(today), &[])
}

/// A benefit's periods that started on or after `from` and ended before `today`.
fn closed(
    card: &RuleSet,
    transactions: &[Transaction],
    from: &str,
    today: &str,
    id: &str,
) -> Vec<BenefitPeriod> {
    history(card, transactions, today)
        .into_iter()
        .find(|benefit| benefit.id == id)
        .unwrap()
        .periods
        .into_iter()
        .filter(|period| period.start >= day(from) && period.end < day(today))
        .collect()
}

fn activity(card: &RuleSet, transactions: &[Transaction], today: &str) -> Vec<BenefitActivity> {
    let refs = transactions.iter().collect::<Vec<_>>();
    card.activity(&refs, &noon(today))
}

fn review(
    card: &RuleSet,
    transactions: &[Transaction],
    today: &str,
) -> Vec<(ReviewKind, String, Option<String>)> {
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = card.activity(&refs, &noon(today));
    card.review(&refs, &matched)
        .into_iter()
        .map(|item| (item.kind, item.transaction_id, item.suggestion_id))
        .collect()
}

fn item<'a>(activity: &'a [BenefitActivity], id: &str) -> &'a BenefitActivity {
    activity.iter().find(|item| item.id == id).unwrap()
}

fn range(low: f64, high: f64) -> Option<AmountRange> {
    Some(AmountRange { low, high })
}

#[test]
fn a_monthly_credit_paired_with_its_purchase_leaves_an_exact_remainder() {
    let card = card_with(json!([streaming()]));
    let transactions = [
        txn("tv", "2026-08-15", "Example TV", -9.99),
        txn("credit", "2026-08-17", "Streaming credit", 9.99),
    ];
    let current = window(&card, &transactions, "2026-08-20", "streaming");
    assert!(!current.period_uncertain);
    assert_eq!(current.credited_amount, 9.99);
    assert_eq!(current.remaining_amount, Some(5.01));
    assert_eq!(current.remaining_range, range(5.01, 5.01));
    let items = activity(&card, &transactions, "2026-08-20");
    assert_eq!(item(&items, "credit").paired_id.as_deref(), Some("tv"));
    assert_eq!(
        item(&items, "tv").credit,
        Some(PurchaseCredit::Credited { amount: 9.99 })
    );
}

#[test]
fn a_credit_posted_after_its_window_closes_counts_toward_the_purchase_window() {
    let card = card_with(json!([dining()]));
    let transactions = [
        txn("history", "2025-12-30", "Coffee", -4.0),
        txn("dinner", "2026-03-29", "Example Tables", -80.4),
        txn("credit", "2026-04-02", "Example Tables dining credit", 50.0),
    ];
    let current = window(&card, &transactions, "2026-06-15", "dining");
    assert_eq!(current.window_start, day("2026-04-01"));
    assert_eq!(current.remaining_amount, Some(50.0));
    assert!(current.activity.is_empty());
    let ended = closed(&card, &transactions, "2026-01-01", "2026-06-15", "dining");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].state, PeriodState::Full);
    assert_eq!(ended[0].unused, range(0.0, 0.0));
    assert_eq!(ended[0].activity_ids, ["credit", "dinner"]);
}

#[test]
fn a_purchase_counts_toward_the_window_of_its_transaction_date_not_its_posting() {
    let card = card_with(json!([hotel(), dining()]));
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        dated(
            "booking",
            "2026-06-30",
            "2026-07-01",
            "Example Hotels",
            -357.87,
        ),
        dated(
            "hotel",
            "2026-06-30",
            "2026-07-02",
            "Test Card Hotel Credit",
            200.0,
        ),
        dated(
            "dinner",
            "2026-09-29",
            "2026-10-01",
            "Example Tables",
            -67.56,
        ),
        dated(
            "dining",
            "2026-09-29",
            "2026-10-02",
            "Example Tables dining credit",
            50.0,
        ),
    ];
    let period = |id: &str, start: &str| {
        history(&card, &transactions, "2026-10-14")
            .into_iter()
            .find(|benefit| benefit.id == id)
            .unwrap()
            .periods
            .into_iter()
            .find(|period| period.start == day(start))
            .unwrap()
    };
    assert_eq!(period("hotel", "2026-01-01").credited, 200.0);
    assert_eq!(period("hotel", "2026-07-01").credited, 0.0);
    assert_eq!(period("dining", "2026-07-01").credited, 50.0);
    assert_eq!(period("dining", "2026-07-01").state, PeriodState::Full);
    let current = window(&card, &transactions, "2026-10-14", "dining");
    assert_eq!(current.window_start, day("2026-10-01"));
    assert_eq!(current.remaining_amount, Some(50.0));
}

#[test]
fn a_quarter_the_history_reaches_partway_is_judged_by_what_it_shows() {
    let card = card_with(json!([dining()]));
    let transactions = [
        txn("history", "2026-05-09", "Coffee", -4.0),
        txn("dining", "2026-05-11", "Example Tables dining credit", 49.8),
    ];
    let quarter = closed(&card, &transactions, "2026-04-01", "2026-08-20", "dining")
        .into_iter()
        .find(|period| period.start == day("2026-04-01"))
        .unwrap();
    assert_eq!(quarter.state, PeriodState::Full);
    assert_eq!(quarter.unused, range(0.2, 0.2));
}

#[test]
fn a_window_that_leaves_only_cents_was_used_in_full_and_still_counts_them() {
    let card = card_with(json!([streaming()]));
    let month = |id: &str, date: &str| {
        [
            txn(&format!("{id}-charge"), date, "Example TV", -14.98),
            txn(
                &format!("{id}-credit"),
                date,
                "Test Card Streaming Credit",
                14.98,
            ),
        ]
    };
    let transactions = [
        vec![txn("history", "2026-07-01", "Coffee", -4.0)],
        month("august", "2026-08-03").to_vec(),
        month("september", "2026-09-03").to_vec(),
    ]
    .concat();
    let august = closed(
        &card,
        &transactions,
        "2026-08-01",
        "2026-09-20",
        "streaming",
    );
    assert_eq!(august[0].state, PeriodState::Full);
    assert_eq!(august[0].unused, range(0.02, 0.02));
    // Nothing spendable is left, so it isn't a credit to use first.
    let current = window(&card, &transactions, "2026-09-20", "streaming");
    assert_eq!(current.remaining_range, range(0.0, 0.0));
    assert_eq!(current.unusable_remainder, Some(0.02));
    assert!(!use_first(&[current]).contains(&"streaming".to_owned()));
}

#[test]
fn an_unpaired_credit_carries_its_window_in_its_transaction_date() {
    let card = card_with(json!([dining()]));
    let transactions = [dated(
        "credit",
        "2026-03-31",
        "2026-04-02",
        "Example Tables dining credit",
        25.0,
    )];
    let items = activity(&card, &transactions, "2026-04-20");
    assert_eq!(
        item(&items, "credit").credit_window,
        Some(day("2026-01-01"))
    );
    assert_eq!(item(&items, "credit").alternate_credit_window, None);
    let current = window(&card, &transactions, "2026-04-20", "dining");
    assert!(!current.period_uncertain);
}

#[test]
fn a_credit_is_due_a_delay_after_its_purchase_posts() {
    let card = card_with(json!([dining()]));
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        dated(
            "dinner",
            "2026-09-29",
            "2026-10-01",
            "Example Tables",
            -67.56,
        ),
    ];
    let items = activity(&card, &transactions, "2026-10-03");
    // Counted from posting, not the transaction date.
    assert_eq!(
        item(&items, "dinner").credit,
        Some(PurchaseCredit::Expected {
            amount: 50.0,
            by: day("2026-10-08"),
        })
    );
}

#[test]
fn an_unpaired_credit_soon_after_a_reset_bounds_both_windows() {
    let card = card_with(json!([dining()]));
    let transactions = [txn(
        "credit",
        "2026-04-05",
        "Example Tables dining credit",
        25.0,
    )];
    let current = window(&card, &transactions, "2026-04-20", "dining");
    assert!(current.period_uncertain);
    assert_eq!(current.credited_amount, 25.0);
    assert_eq!(current.uncertain_credit_amount, 25.0);
    assert_eq!(current.remaining_amount, None);
    assert_eq!(current.remaining_range, range(25.0, 50.0));
    let items = activity(&card, &transactions, "2026-04-20");
    assert_eq!(
        item(&items, "credit").credit_window,
        Some(day("2026-04-01"))
    );
    assert_eq!(
        item(&items, "credit").alternate_credit_window,
        Some(day("2026-01-01"))
    );
}

#[test]
fn a_full_previous_window_settles_where_an_open_credit_belongs() {
    let card = card_with(json!([dining()]));
    let transactions = [
        txn("dinner", "2026-03-10", "Example Tables", -60.0),
        txn("march", "2026-03-12", "Example Tables dining credit", 50.0),
        txn("credit", "2026-04-05", "Example Tables dining credit", 25.0),
    ];
    let current = window(&card, &transactions, "2026-04-20", "dining");
    assert!(!current.period_uncertain);
    assert_eq!(current.remaining_amount, Some(25.0));
}

#[test]
fn an_allowance_that_excludes_tax_never_settles_an_open_credit() {
    let card = card_with(json!([rule(
        "club",
        "monthly",
        9.99,
        "example club",
        json!({ "creditMatches": "example club.*credit", "trackRemaining": false, "recurring": true }),
    )]));
    let transactions = [txn(
        "credit",
        "2026-09-01",
        "Test Card Example Club Credit",
        10.73,
    )];
    let current = window(&card, &transactions, "2026-09-03", "club");
    assert!(current.period_uncertain);
    assert_eq!(current.remaining_range, None);
}

#[test]
fn a_reversal_undoes_its_own_credit_in_that_credit_window() {
    let card = card_with(json!([streaming()]));
    let transactions = [
        txn("tv", "2026-08-15", "Example TV", -9.99),
        txn("credit", "2026-08-17", "Streaming credit", 9.99),
        txn("reversal", "2026-08-25", "Streaming credit reversal", -9.99),
    ];
    let current = window(&card, &transactions, "2026-08-28", "streaming");
    assert!(!current.period_uncertain);
    assert_eq!(current.credited_amount, 0.0);
    assert_eq!(current.remaining_amount, Some(15.0));
}

#[test]
fn a_remainder_below_the_smallest_qualifying_charge_is_used_up() {
    let card = card_with(json!([rule(
        "membership",
        "annual",
        90.0,
        "example membership",
        json!({ "minimumCharge": 7.99, "recurring": true }),
    )]));
    let transactions = (1..=11)
        .flat_map(|month| {
            [
                txn(
                    &format!("fee-{month}"),
                    &format!("2026-{month:02}-05"),
                    "Example Membership",
                    -7.99,
                ),
                txn(
                    &format!("credit-{month}"),
                    &format!("2026-{month:02}-07"),
                    "Membership credit",
                    7.99,
                ),
            ]
        })
        .collect::<Vec<_>>();
    let current = window(&card, &transactions, "2026-12-20", "membership");
    assert_eq!(current.credited_amount, 87.89);
    assert_eq!(current.remaining_amount, Some(0.0));
    assert_eq!(current.unusable_remainder, Some(2.11));
}

#[test]
fn a_purchase_must_also_match_a_dated_pattern_before_its_date() {
    let card = card_with(json!([rule(
        "dining",
        "quarterly",
        50.0,
        "example tables|example booking",
        json!({ "matchRequiresBefore": { "pattern": "example tables", "date": "2026-09-15" } }),
    )]));
    let transactions = [
        txn("before", "2026-09-10", "Example Booking", -30.0),
        txn("after", "2026-09-20", "Example Booking", -30.0),
    ];
    let items = activity(&card, &transactions, "2026-09-25");
    assert!(items.iter().all(|item| item.id != "before"));
    assert_eq!(item(&items, "after").kind, ActivityKind::Purchase);
}

#[test]
fn an_exact_charge_at_a_merchant_matches_one_benefit_and_is_excluded_from_another() {
    let card = card_with(json!([
        rule(
            "membership",
            "annual",
            90.0,
            "example membership",
            json!({ "merchantAmountMatches": [{ "merchant": "example partner", "amount": -7.99 }] }),
        ),
        rule(
            "partner",
            "monthly",
            10.0,
            "^example partner(?:\\s|$)",
            json!({ "excludeMerchantAmounts": [{ "merchant": "example partner", "amount": -7.99 }] }),
        ),
    ]));
    let transactions = [
        txn("fee", "2026-09-05", "Example Partner", -7.99),
        txn("order", "2026-09-06", "Example Partner", -12.0),
    ];
    let items = activity(&card, &transactions, "2026-09-10");
    assert_eq!(item(&items, "fee").benefit_id, "membership");
    assert_eq!(item(&items, "order").benefit_id, "partner");
}

#[test]
fn credit_spent_outside_the_card_is_not_visible_until_a_card_charge_suggests_its_use() {
    let card = card_with(json!([rule(
        "partner",
        "monthly",
        10.0,
        "^example partner(?:\\s|$)",
        json!({
            "issuerCredits": false,
            "trackRemaining": false,
            "estimateFromPurchases": true,
            "monthCaps": { "12": 25 },
        }),
    )]));
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        txn("order", "2026-07-09", "Example Partner", -24.1),
    ];
    let months = closed(&card, &transactions, "2026-06-01", "2026-08-20", "partner");
    let states = months
        .iter()
        .map(|period| (period.end, period.state, period.unused))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            (day("2026-06-30"), PeriodState::NotVisible, None),
            (day("2026-07-31"), PeriodState::LikelyUsed, None),
        ]
    );
}

#[test]
fn only_settled_windows_that_the_history_reaches_can_be_missed() {
    let card = card_with(json!([streaming()]));
    let transactions = [
        txn("history", "2026-02-10", "Coffee", -4.0),
        txn("tv", "2026-02-15", "Example TV", -9.99),
        txn("credit", "2026-02-17", "Streaming credit", 9.99),
    ];
    // March's credits may still post within the usual week after it ends.
    let months = closed(
        &card,
        &transactions,
        "2026-01-01",
        "2026-04-05",
        "streaming",
    );
    let states = months
        .iter()
        .map(|period| (period.end, period.state, period.unused))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            (day("2026-01-31"), PeriodState::BeforeHistory, None),
            (day("2026-02-28"), PeriodState::Partial, range(5.01, 5.01)),
            (day("2026-03-31"), PeriodState::Settling, None),
        ]
    );
    assert_eq!(months[1].credited, 9.99);
    let later = closed(
        &card,
        &transactions,
        "2026-03-01",
        "2026-06-01",
        "streaming",
    );
    assert_eq!(later[0].state, PeriodState::Missed);
    assert_eq!(later[0].unused, range(15.0, 15.0));
}

#[test]
fn an_unused_benefit_is_missed_whether_or_not_the_card_ever_used_it() {
    let card = card_with(json!([dining()]));
    let transactions = [txn("history", "2026-01-01", "Coffee", -4.0)];
    let quarters = closed(&card, &transactions, "2026-01-01", "2026-06-15", "dining");
    assert_eq!(quarters.len(), 1);
    assert_eq!(quarters[0].state, PeriodState::Missed);
    assert_eq!(quarters[0].unused, range(50.0, 50.0));
    let dining = history(&card, &transactions, "2026-06-15");
    let (_, missed) = credit_summary(&[], &dining, day("2026-01-01"), day("2026-06-30"));
    assert_eq!(missed, 50.0);
}

#[test]
fn an_unpaired_credit_is_placed_by_the_usual_delay_not_the_terms_limit() {
    // The terms allow eight weeks, which would reach back to September; credits usually post
    // within a week, so it is October's.
    let card = card_with(json!([dining()]));
    let transactions = [
        txn("history", "2026-08-01", "Coffee", -4.0),
        txn("credit", "2026-10-22", "Test Card Dining Credit", 30.0),
    ];
    let current = window(&card, &transactions, "2026-10-25", "dining");
    assert!(!current.period_uncertain);
    assert!(!current.credit_lag_learned);
    assert_eq!(current.credit_lag_days, 7);
    assert_eq!(current.remaining_amount, Some(20.0));
}

#[test]
fn a_window_that_began_before_the_card_history_is_judged_by_what_it_shows() {
    let card = card_with(json!([travel_fee(), dining()]));
    let transactions = [
        txn("history", "2026-05-09", "Coffee", -4.0),
        txn("credit", "2026-06-10", "Travel fee credit", 50.0),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = card.activity(&refs, &noon("2026-10-13"));
    let windows = card.tracker_for(
        &matched,
        &noon("2026-10-13"),
        &noon("2026-10-13"),
        Some(day("2026-05-09")),
        &NamedZone::Utc,
    );
    let travel = windows.iter().find(|w| w.id == "travel-fee").unwrap();
    assert_eq!(travel.history_from, Some(day("2026-05-09")));
    assert_eq!(travel.remaining_range, range(100.0, 100.0));
    assert_eq!(travel.remaining_amount, Some(100.0));
    // A window the history covers is unaffected.
    let dining = windows.iter().find(|w| w.id == "dining").unwrap();
    assert_eq!(dining.history_from, None);
    assert_eq!(dining.remaining_amount, Some(50.0));
}

#[test]
fn a_renewal_credit_lasts_as_long_as_its_amount_says() {
    let card = card_with(json!([rule(
        "renewal",
        "renewal",
        100.0,
        "example renewal",
        json!({ "renewalTerms": [{ "above": 50, "months": 60 }, { "above": 0, "months": 36 }] }),
    )]));
    let renewal = |amount: f64| {
        let transactions = [txn(
            "credit",
            "2026-03-31",
            "Test Card Renewal Credit",
            amount,
        )];
        window(&card, &transactions, "2026-04-02", "renewal").renewal_check
    };
    assert_eq!(renewal(100.0).as_deref(), Some("Mar 31, 2031"));
    assert_eq!(renewal(40.0).as_deref(), Some("Mar 31, 2029"));
}

#[test]
fn a_learned_credit_delay_tracks_late_and_missing_credits() {
    let card = card_with(json!([streaming()]));
    let mut transactions = (6..=8)
        .flat_map(|month| {
            [
                txn(
                    &format!("fee-{month}"),
                    &format!("2026-{month:02}-01"),
                    "Example News",
                    -10.0,
                ),
                txn(
                    &format!("credit-{month}"),
                    &format!("2026-{month:02}-03"),
                    "Streaming credit",
                    10.0,
                ),
            ]
        })
        .collect::<Vec<_>>();
    transactions.extend([
        txn("old", "2026-07-10", "Example TV", -4.99),
        txn("late", "2026-09-01", "Example TV", -7.99),
        txn("fresh", "2026-09-08", "Example Music", -4.99),
    ]);
    let items = activity(&card, &transactions, "2026-09-10");
    assert_eq!(
        item(&items, "old").credit,
        Some(PurchaseCredit::NotCredited)
    );
    assert_eq!(
        item(&items, "late").credit,
        Some(PurchaseCredit::Late {
            amount: 7.99,
            by: day("2026-10-27")
        })
    );
    assert_eq!(
        item(&items, "fresh").credit,
        Some(PurchaseCredit::Expected {
            amount: 4.99,
            by: day("2026-09-13")
        })
    );
    let current = window(&card, &transactions, "2026-09-10", "streaming");
    assert!(current.credit_lag_learned);
    assert_eq!(current.credit_lag_days, 5);
    assert_eq!(current.awaiting_credit_amount, 12.98);
}

#[test]
fn a_purchase_after_the_allowance_is_used_expects_no_credit() {
    let card = card_with(json!([streaming()]));
    let transactions = [
        txn("first", "2026-09-01", "Example TV", -15.0),
        txn("credit", "2026-09-02", "Streaming credit", 15.0),
        txn("second", "2026-09-05", "Example Music", -9.99),
    ];
    let items = activity(&card, &transactions, "2026-09-06");
    assert_eq!(
        item(&items, "second").credit,
        Some(PurchaseCredit::OverAllowance)
    );
}

#[test]
fn credits_count_against_the_fee_from_its_posting_or_from_january_without_one() {
    let card = card_with(json!([streaming(), dining()]));
    let transactions = vec![
        txn(
            "old-credit",
            "2026-02-10",
            "Test Card Streaming Credit",
            10.0,
        ),
        txn("fee", "2026-03-01", "Annual Membership Fee", -450.0),
        txn("tv", "2026-08-15", "Example TV", -9.99),
        txn("credit", "2026-08-18", "Test Card Streaming Credit", 9.99),
        txn("dining-buy", "2026-08-20", "Example Tables Bistro", -60.0),
        txn(
            "dining-credit",
            "2026-08-23",
            "Example Tables Dining Credit",
            50.0,
        ),
        txn(
            "dining-reversal",
            "2026-08-25",
            "Example Tables Dining Credit",
            -20.0,
        ),
        txn("generic-refund", "2026-08-26", "Example Shop Refund", 500.0),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let activity = activity(&card, &transactions, "2026-09-16");
    let recovery = |refs: &[&Transaction]| {
        card.fee_recovery(refs, &[], &activity, &[], day("2026-09-16"))
            .unwrap()
    };
    // Only issuer credits after the fee count, and hidden benefits still count.
    assert_eq!(
        recovery(&refs),
        FeeRecovery {
            annual_fee: 450.0,
            recovered: 39.99,
            offsets: 0.0,
            covered: false,
            gap: 410.01,
            for_others: 0.0,
            since: day("2026-03-01"),
            basis: "fee-charge",
            through: day("2027-02-28"),
            expected: 0.0,
        }
    );
    // A bonus filed as repaying this year's fee counts; one repaying an older fee, or still
    // pending, doesn't.
    let mut bonus = txn("bonus", "2026-04-15", "Engagement Bonus", 450.0);
    bonus.refund_of = Some("fee".into());
    let mut pending = txn("pending-bonus", "2026-09-10", "Engagement Bonus", 450.0);
    pending.refund_of = Some("fee".into());
    pending.pending = true;
    let mut stale = txn("old-bonus", "2025-04-15", "Engagement Bonus", 450.0);
    stale.refund_of = Some("old-fee".into());
    let offset = card
        .fee_recovery(
            &refs,
            &[bonus, pending, stale],
            &activity,
            &[],
            day("2026-09-16"),
        )
        .unwrap();
    assert_eq!(
        (offset.offsets, offset.gap, offset.covered),
        (450.0, 39.99, true)
    );
    // A credit on someone else's purchase is counted apart.
    let mut shared = transactions.clone();
    shared
        .iter_mut()
        .filter(|t| t.id == "credit")
        .for_each(|t| t.other = true);
    let shared_refs = shared.iter().collect::<Vec<_>>();
    let shared_recovery = recovery(&shared_refs);
    assert_eq!(
        (shared_recovery.recovered, shared_recovery.for_others),
        (30.0, 9.99)
    );
    // Without a fee charge the year starts in January, or where the history starts if later.
    let without_fee = refs
        .iter()
        .copied()
        .filter(|t| t.id != "fee")
        .collect::<Vec<_>>();
    assert_eq!(
        recovery(&without_fee),
        FeeRecovery {
            annual_fee: 450.0,
            recovered: 49.99,
            offsets: 0.0,
            covered: false,
            gap: 400.01,
            for_others: 0.0,
            since: day("2026-02-10"),
            basis: "card-history",
            through: day("2026-12-31"),
            expected: 0.0,
        }
    );
    let december = txn("december", "2025-12-20", "Coffee", -4.0);
    let full_year = without_fee
        .iter()
        .copied()
        .chain([&december])
        .collect::<Vec<_>>();
    assert_eq!(
        (recovery(&full_year).since, recovery(&full_year).basis),
        (day("2026-01-01"), "calendar-year")
    );
}

#[test]
fn the_fee_year_expects_the_credits_its_periods_plan_before_it_ends() {
    let card = card_with(json!([]));
    let fee = txn("fee", "2026-03-01", "Annual Membership Fee", -450.0);
    let mut theirs = txn("theirs", "2026-09-10", "Example Tables Bistro", -40.0);
    theirs.other = true;
    let transactions = [fee, theirs];
    let refs = transactions.iter().collect::<Vec<_>>();
    let credit =
        |title: &str, date: &str, amount: f64, transaction_id: Option<&str>| PlannedCredit {
            title: title.into(),
            date: day(date),
            amount,
            source: if transaction_id.is_some() {
                PlannedSource::Purchase
            } else {
                PlannedSource::Recurring
            },
            transaction_id: transaction_id.map(Into::into),
        };
    let period = |planned: Vec<PlannedCredit>| BenefitPeriod {
        start: day("2026-10-01"),
        end: day("2026-12-31"),
        label: "Q4".into(),
        cap: 50.0,
        credited: 0.0,
        state: PeriodState::Open,
        unused: None,
        left: None,
        activity_ids: Vec::new(),
        planned_total: 0.0,
        planned,
    };
    let history = vec![BenefitHistory {
        id: "dining".into(),
        name: "Dining".into(),
        logo_domain: None,
        logo_url: None,
        logo_brand: None,
        category: None,
        recurring: false,
        external: false,
        retired: false,
        periods: vec![
            period(vec![
                credit("Example streaming", "2026-10-05", 25.0, None),
                // Someone else's purchase earns them the credit.
                credit("Example Tables Bistro", "2026-09-10", 40.0, Some("theirs")),
            ]),
            // After the fee year ends.
            period(vec![credit("Example streaming", "2027-03-05", 25.0, None)]),
        ],
        years: Vec::new(),
    }];
    let recovery = card
        .fee_recovery(&refs, &[], &[], &history, day("2026-09-16"))
        .unwrap();
    assert_eq!(
        (recovery.through, recovery.expected),
        (day("2027-02-28"), 25.0)
    );
}

#[test]
fn an_issuer_credit_is_read_from_the_banks_own_descriptor() {
    // Plaid may rename an issuer credit and file it as income; the bank's descriptor still
    // names it, even after the benefit retired.
    let cards = Catalog::new(&[test_rules(json!([store()]))]).unwrap();
    let card = cards.only().unwrap();
    let mut credit = txn("store-credit", "2026-07-01", "Goodwill", 40.0);
    credit.description = Some("Goodwill".into());
    credit.original_description = Some("EXAMPLE STORE BENEFIT GOODWILL".into());
    credit.category = "Income".into();
    let purchase = txn("store-purchase", "2026-06-28", "Example Store", -60.0);
    let transactions = vec![purchase, credit.clone()];
    let items = activity(card, &transactions, "2026-07-10");
    let found = item(&items, "store-credit");
    assert_eq!(
        (found.benefit_id.as_str(), found.kind),
        ("store", ActivityKind::Credit)
    );
    assert_eq!(found.paired_id.as_deref(), Some("store-purchase"));
    let classified = crate::transaction_policy::classify(&credit, &cards);
    assert_eq!(
        (classified.kind, classified.credit),
        (crate::transaction_policy::Kind::Reimbursement, true)
    );
    // A refund from an unrelated merchant isn't the benefit's credit.
    let thrift = txn("thrift", "2026-07-02", "Goodwill", 50.0);
    assert!(activity(card, &[thrift], "2026-07-10").is_empty());
}

#[test]
fn a_credit_that_posts_before_its_purchase_suggests_the_benefits_own_merchant() {
    let card = card_with(json!([store()]));
    let mut credit = txn("store-credit", "2026-07-01", "Goodwill", 40.0);
    credit.original_description = Some("EXAMPLE STORE BENEFIT GOODWILL".into());
    let mut transactions = vec![
        txn("history", "2026-01-02", "Coffee", -4.0),
        // Larger and earlier, but not the benefit's merchant.
        txn("insurance", "2026-06-25", "Example Insurance", -900.0),
        txn("store-purchase", "2026-07-03", "Example Store", -60.0),
        credit,
    ];
    assert_eq!(
        review(&card, &transactions, "2026-07-10"),
        [(
            ReviewKind::NoPurchase,
            "store-credit".into(),
            Some("store-purchase".into())
        )]
    );
    // Linking pairs them, despite the order and the retirement.
    transactions[2].benefit_id = Some("store".into());
    assert_eq!(review(&card, &transactions, "2026-07-10"), []);
    let items = activity(&card, &transactions, "2026-07-10");
    assert_eq!(
        item(&items, "store-credit").paired_id.as_deref(),
        Some("store-purchase")
    );
    assert_eq!(
        item(&items, "store-purchase").credit,
        Some(PurchaseCredit::Credited { amount: 40.0 })
    );
}

#[test]
fn credits_to_use_first_lead_with_the_soonest_deadline_and_the_most_left() {
    let base = window(&card_with(json!([dining()])), &[], "2026-09-14", "dining");
    let with = |id: &str, cadence: Cadence, end: &str, high: Option<f64>| BenefitWindow {
        id: id.into(),
        name: id.into(),
        cadence,
        window_end: day(end),
        remaining_range: high.map(|high| AmountRange { low: 0.0, high }),
        ..base.clone()
    };
    // Credit spent outside the card is listed until a card charge suggests it ran out.
    let outside = |id: &str, estimated: f64| BenefitWindow {
        cap: 10.0,
        estimated_credit_amount: Some(estimated),
        ..with(id, Cadence::Monthly, "2026-09-30", None)
    };
    let windows = [
        with("hotel", Cadence::Annual, "2026-12-31", Some(300.0)),
        with("renewal", Cadence::Renewal, "2026-09-30", Some(50.0)),
        with("store", Cadence::Monthly, "2026-09-30", Some(15.0)),
        with("used", Cadence::Monthly, "2026-09-30", Some(0.0)),
        with("untracked", Cadence::Monthly, "2026-09-30", None),
        with("dining", Cadence::Monthly, "2026-09-30", Some(25.0)),
        BenefitWindow {
            recurring: true,
            ..with("streaming", Cadence::Monthly, "2026-09-30", Some(25.0))
        },
        outside("partner-unused", 0.0),
        outside("partner-used", 10.0),
    ];
    assert_eq!(
        use_first(&windows),
        ["dining", "store", "partner-unused", "hotel", "renewal"]
    );
}

#[test]
fn a_change_of_cadence_ends_the_old_allowance_the_day_before() {
    let card = card_with(json!([hotel()]));
    let transactions = [txn("history", "2025-01-05", "Coffee", -4.0)];
    let hotel = history(&card, &transactions, "2026-02-10")
        .into_iter()
        .find(|benefit| benefit.id == "hotel")
        .unwrap();
    let periods = hotel
        .periods
        .iter()
        .map(|period| (period.label.as_str(), period.cap, period.state))
        .collect::<Vec<_>>();
    assert_eq!(
        periods,
        [
            ("Jan 1 – Aug 31", 150.0, PeriodState::BeforeHistory),
            ("Sep 1 – Dec 31", 200.0, PeriodState::Missed),
            ("Jan – Jun", 200.0, PeriodState::Open),
            ("Jul – Dec", 200.0, PeriodState::Future),
        ]
    );
    assert_eq!(hotel.periods[2].left, range(200.0, 200.0));
}

#[test]
fn matches_that_need_a_decision_are_queued_until_one_is_saved() {
    let card = card_with(json!([streaming(), dining(), hotel()]));
    let mut transactions = vec![
        txn("history", "2026-01-02", "Coffee", -4.0),
        txn("store", "2026-05-20", "Example TV Shop", -42.18),
        txn("dinner", "2026-08-12", "Example Bistro", -50.0),
        txn(
            "dining-credit",
            "2026-08-14",
            "Example Tables Dining Credit",
            50.0,
        ),
        txn(
            "issuer-credit",
            "2026-09-03",
            "TEST CARD STATEMENT CREDIT",
            50.0,
        ),
        txn("return", "2026-09-10", "Example Hotels", 32.0),
    ];
    let some = |id: &str| Some(id.to_owned());
    assert_eq!(
        review(&card, &transactions, "2026-09-20"),
        [
            (ReviewKind::UnverifiedCredit, "return".into(), None),
            (ReviewKind::UnassignedCredit, "issuer-credit".into(), None),
            (
                ReviewKind::NoPurchase,
                "dining-credit".into(),
                some("dinner")
            ),
            (ReviewKind::NotEligible, "store".into(), None),
        ]
    );
    let decide = |transactions: &mut Vec<Transaction>, id: &str, f: &dyn Fn(&mut Transaction)| {
        transactions.iter_mut().filter(|t| t.id == id).for_each(f)
    };
    // Each decision is a saved annotation: confirm, assign, link by assigning, or reject.
    decide(&mut transactions, "return", &|t| {
        t.benefit_confirmed = Some(true)
    });
    decide(&mut transactions, "issuer-credit", &|t| {
        t.benefit_id = some("dining")
    });
    decide(&mut transactions, "dinner", &|t| {
        t.benefit_id = some("dining")
    });
    decide(&mut transactions, "store", &|t| {
        t.benefit_confirmed = Some(false)
    });
    assert_eq!(review(&card, &transactions, "2026-09-20"), []);
    let items = activity(&card, &transactions, "2026-09-20");
    assert_eq!(
        (
            item(&items, "return").kind,
            item(&items, "return").benefit_id.as_str()
        ),
        (ActivityKind::Credit, "hotel")
    );
    assert_eq!(item(&items, "issuer-credit").kind, ActivityKind::Credit);
    assert_eq!(item(&items, "dining-credit").paired_id, some("dinner"));
    assert!(items.iter().all(|item| item.id != "store"));
}

#[test]
fn another_merchants_charge_is_suggested_only_at_the_exact_amount_in_the_benefits_category() {
    let card = card_with(json!([entertainment()]));
    let mut transactions = vec![
        categorized(
            "market",
            "2026-10-30",
            "Example Market",
            -15.99,
            "Groceries",
        ),
        categorized(
            "tickets",
            "2026-10-29",
            "Example Tickets",
            -129.9,
            "Entertainment",
        ),
        txn("credit", "2026-11-01", "Entertainment Credit", 15.99),
    ];
    assert_eq!(review(&card, &transactions, "2026-11-05"), []);
    transactions.push(categorized(
        "show",
        "2026-10-31",
        "Example Stream",
        -15.99,
        "Entertainment",
    ));
    assert_eq!(
        review(&card, &transactions, "2026-11-05"),
        [(ReviewKind::NoPurchase, "credit".into(), Some("show".into()))]
    );
}

#[test]
fn purchases_unlike_every_credited_one_expect_no_credit_and_ask_nothing() {
    let card = card_with(json!([rule(
        "membership",
        "annual",
        120.0,
        "example rides",
        json!({ "creditMatches": "membership credit" })
    )]));
    let mut transactions = (6..=8)
        .flat_map(|month| {
            [
                categorized(
                    &format!("fee-{month}"),
                    &format!("2026-{month:02}-08"),
                    "Example Rides",
                    -10.0,
                    "Services",
                ),
                txn(
                    &format!("credit-{month}"),
                    &format!("2026-{month:02}-08"),
                    "Membership Credit",
                    10.0,
                ),
            ]
        })
        .collect::<Vec<_>>();
    transactions.extend([
        // Another amount at the same merchant, and another category.
        categorized("ride", "2026-07-01", "Example Rides", -25.0, "Services"),
        categorized("meal", "2026-07-02", "Example Rides Eats", -30.0, "Dining"),
        // Like every credited purchase, yet no credit came.
        categorized("fee-5", "2026-05-08", "Example Rides", -10.0, "Services"),
        // Linked by hand, so it still counts.
        Transaction {
            benefit_id: Some("membership".into()),
            ..categorized("linked", "2026-07-03", "Example Rides", -40.0, "Transport")
        },
    ]);
    let items = activity(&card, &transactions, "2026-09-20");
    for id in ["ride", "meal"] {
        assert_eq!(
            item(&items, id).credit,
            Some(PurchaseCredit::Unlikely),
            "{id}"
        );
    }
    assert_eq!(
        item(&items, "fee-5").credit,
        Some(PurchaseCredit::NotCredited)
    );
    assert_ne!(
        item(&items, "linked").credit,
        Some(PurchaseCredit::Unlikely)
    );
    assert_eq!(
        review(&card, &transactions, "2026-09-20"),
        [(ReviewKind::NotEligible, "fee-5".into(), None),]
    );
}

#[test]
fn a_benefit_learns_a_merchant_from_its_credits_or_a_link() {
    let card = card_with(json!([entertainment()]));
    let paired = |transactions: &[Transaction]| {
        let items = activity(&card, transactions, "2026-11-05");
        transactions
            .iter()
            .filter(|t| t.amount > 0.0)
            .map(|t| item(&items, &t.id).paired_id.clone())
            .collect::<Vec<_>>()
    };
    let some = |id: &str| Some(id.to_owned());
    // The same charge precedes a credit of its amount month after month.
    let monthly = [
        txn("aug", "2026-08-31", "Example Stream", -15.99),
        txn("aug-credit", "2026-09-01", "Entertainment Credit", 15.99),
        txn("sep", "2026-10-01", "Example Stream", -15.99),
        txn("sep-credit", "2026-10-02", "Entertainment Credit", 15.99),
        txn("oct", "2026-10-31", "Example Stream", -15.99),
        txn("oct-credit", "2026-11-01", "Entertainment Credit", 15.99),
    ];
    assert_eq!(paired(&monthly), [some("aug"), some("sep"), some("oct")]);
    assert_eq!(review(&card, &monthly, "2026-11-05"), []);
    // Once is a coincidence, and so is a charge that usually comes without a credit.
    assert_eq!(paired(&monthly[..2]), [None]);
    let weekly = (1..=9)
        .map(|week| {
            let date = format!("2026-{:02}-{:02}", 8 + (week * 7) / 31, 1 + (week * 7) % 31);
            txn(&format!("w{week}"), &date, "Example Market", -15.99)
        })
        .chain([
            txn("c1", "2026-08-09", "Entertainment Credit", 15.99),
            txn("c2", "2026-08-30", "Entertainment Credit", 15.99),
        ])
        .collect::<Vec<_>>();
    assert_eq!(paired(&weekly), [None, None]);
    // A charge linked by hand teaches its merchant at its amount.
    let mut linked = vec![
        txn("aug", "2026-08-31", "Example Stream", -15.99),
        txn("next", "2026-09-30", "Example Stream", -15.99),
        txn("next-credit", "2026-10-01", "Entertainment Credit", 15.99),
    ];
    assert_eq!(paired(&linked), [None]);
    linked[0].benefit_id = some("entertainment");
    assert_eq!(paired(&linked), [some("next")]);
}

#[test]
fn confirming_an_uncredited_purchase_keeps_it_a_purchase() {
    let card = card_with(json!([streaming()]));
    let mut store = txn("store", "2026-05-20", "Example TV Shop", -42.18);
    store.benefit_confirmed = Some(true);
    let transactions = [txn("history", "2026-01-02", "Coffee", -4.0), store];
    let items = activity(&card, &transactions, "2026-09-20");
    assert_eq!(item(&items, "store").kind, ActivityKind::Purchase);
    assert_eq!(
        item(&items, "store").credit,
        Some(PurchaseCredit::NotCredited)
    );
    assert_eq!(review(&card, &transactions, "2026-09-20"), []);
}

#[test]
fn planned_credits_fill_open_and_future_periods_from_waiting_purchases_then_subscriptions() {
    let card = card_with(json!([streaming()]));
    let transactions = [
        txn("history", "2026-01-02", "Coffee", -4.0),
        txn("tv", "2026-10-02", "Example TV", -7.99),
        txn("credit", "2026-10-03", "Streaming credit", 7.99),
        txn("news", "2026-10-12", "Example News", -4.0),
    ];
    let music = txn("music", "2026-09-20", "Example Music Premium", -5.99);
    assert_eq!(card.purchase_benefit(&music).as_deref(), Some("streaming"));
    let charges = ["2026-10-20", "2026-11-20"]
        .map(|date| PlannedCharge {
            benefit_id: "streaming".into(),
            date: day(date),
            amount: -5.99,
            title: "Example Music Premium".into(),
        })
        .to_vec();
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = card.activity(&refs, &noon("2026-10-13"));
    let streaming = card
        .history(
            &matched,
            Some(day("2026-01-02")),
            day("2026-10-13"),
            &charges,
        )
        .into_iter()
        .find(|benefit| benefit.id == "streaming")
        .unwrap();
    let month = |start: &str| {
        streaming
            .periods
            .iter()
            .find(|period| period.start == day(start))
            .unwrap()
            .planned
            .clone()
    };
    // A waiting purchase's credit comes first; the subscription gets what's left.
    assert_eq!(
        month("2026-10-01"),
        [
            PlannedCredit {
                title: "Example News".into(),
                date: day("2026-10-12"),
                amount: 4.0,
                source: PlannedSource::Purchase,
                transaction_id: Some("news".into()),
            },
            PlannedCredit {
                title: "Example Music Premium".into(),
                date: day("2026-10-20"),
                amount: 3.01,
                source: PlannedSource::Recurring,
                transaction_id: None,
            },
        ]
    );
    assert_eq!(month("2026-11-01")[0].amount, 5.99);
    // A closed month plans nothing.
    assert!(month("2026-09-01").is_empty());
}

#[test]
fn each_year_sums_the_benefits_shown_and_what_went_to_others() {
    use super::history::{card_years, BenefitYear, CardBenefitYear};
    let period = |start: &str, credited: f64, ids: &[&str]| BenefitPeriod {
        start: day(start),
        end: day(start),
        label: start.into(),
        cap: 100.0,
        credited,
        state: PeriodState::Partial,
        unused: Some(AmountRange {
            low: 10.0,
            high: 20.0,
        }),
        left: None,
        activity_ids: ids.iter().map(|id| (*id).into()).collect(),
        planned: Vec::new(),
        planned_total: 2.5,
    };
    let benefit =
        |id: &str, external: bool, retired: bool, periods: Vec<BenefitPeriod>| BenefitHistory {
            id: id.into(),
            name: id.into(),
            logo_domain: None,
            logo_url: None,
            logo_brand: None,
            category: None,
            recurring: false,
            external,
            retired,
            periods,
            years: Vec::new(),
        };
    let history = [
        benefit(
            "dining",
            false,
            false,
            vec![period(
                "2026-07-01",
                50.0,
                &["theirs", "mine", "undone", "waiting"],
            )],
        ),
        // Estimated from charges, so it adds nothing.
        benefit(
            "partner",
            true,
            false,
            vec![period("2026-07-01", 10.0, &[])],
        ),
        benefit(
            "hidden",
            false,
            false,
            vec![period("2026-07-01", 9.99, &[])],
        ),
        // Retired: shown in a year with credits, not in one without.
        benefit(
            "retired",
            false,
            true,
            vec![
                period("2025-07-01", 30.0, &[]),
                period("2026-01-01", 0.0, &[]),
            ],
        ),
    ];
    let credit = |id: &str, amount: f64, pending: bool| BenefitActivity {
        id: id.into(),
        merchant: "Example Tables".into(),
        description: None,
        amount,
        pending,
        account_id: None,
        date: day("2026-07-02"),
        posted: day("2026-07-02"),
        dated: false,
        benefit_id: "dining".into(),
        benefit_name: "Dining".into(),
        kind: ActivityKind::Credit,
        paired_id: None,
        credit_window: None,
        alternate_credit_window: None,
        credit: None,
    };
    let activity = [
        credit("theirs", 30.0, false),
        credit("mine", 20.0, false),
        credit("waiting", 5.0, true),
        BenefitActivity {
            kind: ActivityKind::Reversal,
            ..credit("undone", -10.0, false)
        },
    ];
    let others = ["theirs", "undone", "waiting"].into_iter().collect();
    let years = card_years(
        &history,
        &activity,
        &others,
        &["hidden".to_string()],
        day("2026-09-16"),
    );
    let missed = AmountRange {
        low: 10.0,
        high: 20.0,
    };
    let none = AmountRange {
        low: 0.0,
        high: 0.0,
    };
    assert_eq!(
        years,
        [
            CardBenefitYear {
                totals: BenefitYear {
                    year: 2025,
                    captured: 30.0,
                    planned: 2.5,
                    missed,
                    left: none,
                },
                for_others: 0.0,
            },
            CardBenefitYear {
                totals: BenefitYear {
                    year: 2026,
                    captured: 50.0,
                    planned: 2.5,
                    missed,
                    left: none,
                },
                // Net of the reversal; a pending credit isn't counted yet.
                for_others: 20.0,
            },
        ]
    );
}

#[test]
fn official_names_recognize_the_product_and_not_the_ones_it_excludes() {
    let example = Catalog::example();
    for name in ["The Example Card®", "Example Card", "example card"] {
        assert_eq!(
            example
                .named_product(Some(name))
                .map(|rules| rules.product.as_str()),
            Some("example-card"),
            "{name}"
        );
    }
    for name in ["Other Bank Rewards Card", "Everyday Checking"] {
        assert!(example.named_product(Some(name)).is_none(), "{name}");
    }
    assert!(example.named_product(None).is_none());
    let test = Catalog::new(&[test_rules(json!([]))]).unwrap();
    assert!(test.named_product(Some("Test Card")).is_some());
    assert!(test.named_product(Some("Test Card Business")).is_none());
    // One product is the fallback; none tracks nothing.
    assert_eq!(
        example.only().map(|rules| rules.product.as_str()),
        Some("example-card")
    );
    assert!(Catalog::default().only().is_none());
}

fn anniversary_lounge() -> Value {
    rule(
        "lounge",
        "annual",
        100.0,
        "example lounge",
        json!({ "alignment": "anniversary" }),
    )
}

#[test]
fn an_anniversary_benefit_runs_from_the_day_its_annual_fee_posted() {
    let spa = rule(
        "spa",
        "semiannual",
        50.0,
        "example spa",
        json!({ "alignment": "anniversary" }),
    );
    let transactions = [
        txn("fee", "2026-03-15", "Annual Membership Fee", -450.0),
        txn("earlier", "2026-02-10", "Example Lounge", -100.0),
        txn(
            "earlier-credit",
            "2026-02-15",
            "Example Lounge credit",
            100.0,
        ),
        txn("visit", "2026-04-10", "Example Lounge", -60.0),
        txn("visit-credit", "2026-04-12", "Lounge credit", 60.0),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let card = card_with(json!([anniversary_lounge(), spa])).for_card(&refs);
    let lounge = window(&card, &transactions, "2026-06-01", "lounge");
    assert_eq!(
        (lounge.window_start, lounge.window_end),
        (day("2026-03-15"), day("2027-03-14"))
    );
    // February's credit belonged to the cardmember year before.
    assert_eq!(lounge.remaining_amount, Some(40.0));
    assert!(!lounge.anniversary_unknown);
    let spa = window(&card, &transactions, "2026-06-01", "spa");
    assert_eq!(
        (spa.window_start, spa.window_end),
        (day("2026-03-15"), day("2026-09-14"))
    );
}

#[test]
fn an_anniversary_benefit_follows_the_calendar_until_a_fee_charge_dates_its_year() {
    let transactions = [txn("visit", "2026-04-10", "Example Lounge", -60.0)];
    let refs = transactions.iter().collect::<Vec<_>>();
    let card = card_with(json!([anniversary_lounge()])).for_card(&refs);
    let lounge = window(&card, &transactions, "2026-06-01", "lounge");
    assert_eq!(
        (lounge.window_start, lounge.window_end),
        (day("2026-01-01"), day("2026-12-31"))
    );
    assert!(lounge.anniversary_unknown);
}

#[test]
fn a_calendar_benefit_ignores_the_anniversary() {
    let transactions = [txn("fee", "2026-03-15", "Annual Membership Fee", -450.0)];
    let refs = transactions.iter().collect::<Vec<_>>();
    let card = card_with(json!([travel_fee()])).for_card(&refs);
    let travel = window(&card, &transactions, "2026-06-01", "travel-fee");
    assert_eq!(
        (travel.window_start, travel.window_end),
        (day("2026-01-01"), day("2026-12-31"))
    );
    assert!(!travel.anniversary_unknown);
}

#[test]
fn cardmember_years_split_into_halves_and_quarters_from_the_anniversary() {
    use super::windows::anniversary_window;
    assert_eq!(
        anniversary_window(day("2026-02-01"), Cadence::Annual, (3, 15)),
        (day("2025-03-15"), day("2026-03-14"))
    );
    assert_eq!(
        anniversary_window(day("2026-12-20"), Cadence::Quarterly, (3, 15)),
        (day("2026-12-15"), day("2027-03-14"))
    );
    // A Feb 29 anniversary starts on Feb 28 in other years.
    assert_eq!(
        anniversary_window(day("2027-03-01"), Cadence::Annual, (2, 29)),
        (day("2027-02-28"), day("2028-02-28"))
    );
}

#[test]
fn a_benefit_logo_is_the_rules_domain_then_where_it_was_last_used_then_its_brand() {
    let card = card_with(json!([
        rule(
            "streaming",
            "monthly",
            15.0,
            "example tv",
            json!({ "logoDomain": "tv.example.com" })
        ),
        dining(),
        rule("travel", "annual", 100.0, "example air", json!({})),
        rule("shop", "annual", 100.0, r"\b(?:examplebrand)\b", json!({})),
        rule(
            "perk",
            "annual",
            100.0,
            "dining",
            json!({ "name": "Dining credit" })
        ),
    ]));
    let with_site = |id: &str, date: &str, merchant: &str, site: &str| {
        let mut transaction = txn(id, date, merchant, -20.0);
        transaction.website = Some(site.into());
        transaction
    };
    let transactions = [
        with_site("tv", "2026-03-02", "Example TV", "https://shop.example.org"),
        with_site(
            "old",
            "2026-02-10",
            "Example Tables",
            "https://old.example.com",
        ),
        with_site(
            "new",
            "2026-03-10",
            "Example Tables",
            "https://www.tables.example.com/book",
        ),
        txn("air", "2026-03-11", "Example Air", -80.0),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let activity = card.activity(&refs, &noon("2026-03-15"));
    let logos = card.logos(&refs, &activity);
    let domain = |id: &str| logos.get(id).and_then(|logo| logo.domain.as_deref());
    assert_eq!(domain("streaming"), Some("tv.example.com"));
    assert_eq!(domain("dining"), Some("tables.example.com"));
    // With nothing else known, the brand its pattern names is left to look a logo up by.
    assert_eq!(
        logos.get("travel"),
        Some(&BenefitLogo {
            brand: Some("example air".into()),
            ..BenefitLogo::default()
        })
    );
    assert_eq!(logos.get("perk"), None);

    // Without a site, the logo the bank attached to the purchase stands in; only over https.
    let with_logo = |id: &str, url: &str| {
        let mut transaction = txn(id, "2026-03-11", "Example Air", -80.0);
        transaction.logo_url = Some(url.into());
        transaction
    };
    for (url, expected) in [
        (
            "https://logos.example/air.png",
            Some("https://logos.example/air.png"),
        ),
        ("http://logos.example/air.png", None),
    ] {
        let transactions = [with_logo("air", url)];
        let refs = transactions.iter().collect::<Vec<_>>();
        let activity = card.activity(&refs, &noon("2026-03-15"));
        let logos = card.logos(&refs, &activity);
        assert_eq!(
            logos.get("travel").and_then(|logo| logo.url.as_deref()),
            expected
        );
    }
}
