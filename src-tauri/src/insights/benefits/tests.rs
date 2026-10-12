use chrono::NaiveDate;
use serde_json::json;

use super::*;
use crate::insights::calendar::NamedZone;

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn txn(id: &str, date: &str, merchant: &str, amount: f64) -> Transaction {
    serde_json::from_value(json!({
        "id": id,
        "merchant": merchant,
        "category": "Shopping",
        "date": date,
        "amount": amount,
        "account": "Platinum Card",
        "pending": false,
        "description": null,
    }))
    .unwrap()
}

fn noon(date: &str) -> String {
    format!("{date}T12:00:00Z")
}

fn window(transactions: &[Transaction], today: &str, id: &str) -> BenefitWindow {
    let refs = transactions.iter().collect::<Vec<_>>();
    PLATINUM
        .tracker(&refs, &noon(today), &noon(today), &NamedZone::Utc)
        .into_iter()
        .find(|window| window.id == id)
        .unwrap()
}

fn outcomes(
    transactions: &[Transaction],
    first: &str,
    today: &str,
    id: &str,
) -> Vec<WindowOutcome> {
    let refs = transactions.iter().collect::<Vec<_>>();
    PLATINUM
        .window_outcomes(&refs, day(first), &noon(today), &NamedZone::Utc)
        .into_iter()
        .filter(|outcome| outcome.benefit_id == id)
        .collect()
}

fn activity(transactions: &[Transaction], today: &str) -> Vec<BenefitActivity> {
    let refs = transactions.iter().collect::<Vec<_>>();
    PLATINUM.activity(&refs, &noon(today), &NamedZone::Utc)
}

fn item<'a>(activity: &'a [BenefitActivity], id: &str) -> &'a BenefitActivity {
    activity.iter().find(|item| item.id == id).unwrap()
}

fn range(low: f64, high: f64) -> Option<AmountRange> {
    Some(AmountRange { low, high })
}

#[test]
fn a_monthly_credit_paired_with_its_purchase_leaves_an_exact_remainder() {
    let transactions = [
        txn("disney", "2026-08-15", "Disney Plus", -13.99),
        txn(
            "credit",
            "2026-08-17",
            "Digital entertainment credit",
            13.99,
        ),
    ];
    let current = window(&transactions, "2026-08-20", "digital-entertainment");
    assert!(!current.period_uncertain);
    assert_eq!(current.credited_amount, 13.99);
    assert_eq!(current.remaining_amount, Some(11.01));
    assert_eq!(current.remaining_range, range(11.01, 11.01));
    let items = activity(&transactions, "2026-08-20");
    assert_eq!(item(&items, "credit").paired_id.as_deref(), Some("disney"));
    assert_eq!(
        item(&items, "disney").credit,
        Some(PurchaseCredit::Credited { amount: 13.99 })
    );
}

#[test]
fn a_credit_posted_after_its_window_closes_counts_toward_the_purchase_window() {
    let transactions = [
        txn("history", "2025-12-30", "Coffee", -4.0),
        txn("dinner", "2026-03-29", "Resy", -180.4),
        txn("credit", "2026-04-02", "Resy credit", 100.0),
    ];
    let current = window(&transactions, "2026-06-15", "resy");
    assert_eq!(current.window_start, day("2026-04-01"));
    assert_eq!(current.remaining_amount, Some(100.0));
    assert!(current.activity.is_empty());
    let closed = outcomes(&transactions, "2026-01-01", "2026-06-15", "resy");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].state, OutcomeState::Settled);
    assert_eq!(
        (closed[0].unused, closed[0].unused_high),
        (Some(0.0), Some(0.0))
    );
}

#[test]
fn an_unpaired_credit_soon_after_a_reset_bounds_both_windows() {
    // The restaurant's own name, not Resy's, appears on the purchase.
    let transactions = [txn("credit", "2026-04-05", "Resy credit", 50.0)];
    let current = window(&transactions, "2026-04-20", "resy");
    assert!(current.period_uncertain);
    assert_eq!(current.credited_amount, 50.0);
    assert_eq!(current.uncertain_credit_amount, 50.0);
    assert_eq!(current.remaining_amount, None);
    assert_eq!(current.remaining_range, range(50.0, 100.0));
    let items = activity(&transactions, "2026-04-20");
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
    let transactions = [
        txn("dinner", "2026-03-10", "Resy", -120.0),
        txn("march", "2026-03-12", "Resy credit", 100.0),
        txn("credit", "2026-04-05", "Resy credit", 50.0),
    ];
    let current = window(&transactions, "2026-04-20", "resy");
    assert!(!current.period_uncertain);
    assert_eq!(current.remaining_amount, Some(50.0));
}

#[test]
fn an_allowance_that_excludes_tax_never_settles_an_open_credit() {
    let transactions = [txn("credit", "2026-09-01", "AMEX Walmart+ Credit", 13.73)];
    let current = window(&transactions, "2026-09-03", "walmart-plus");
    assert!(current.period_uncertain);
    assert_eq!(current.remaining_range, None);
}

#[test]
fn a_reversal_undoes_its_own_credit_in_that_credit_window() {
    let transactions = [
        txn("disney", "2026-08-15", "Disney Plus", -13.99),
        txn(
            "credit",
            "2026-08-17",
            "Digital entertainment credit",
            13.99,
        ),
        txn(
            "reversal",
            "2026-08-25",
            "Digital entertainment credit reversal",
            -13.99,
        ),
    ];
    let current = window(&transactions, "2026-08-28", "digital-entertainment");
    assert!(!current.period_uncertain);
    assert_eq!(current.credited_amount, 0.0);
    assert_eq!(current.remaining_amount, Some(25.0));
}

#[test]
fn a_remainder_below_the_smallest_qualifying_charge_is_used_up() {
    let transactions = (1..=12)
        .flat_map(|month| {
            let date = format!("2026-{month:02}-05");
            let credited = format!("2026-{month:02}-07");
            [
                txn(&format!("fee-{month}"), &date, "Uber One", -9.99),
                txn(
                    &format!("credit-{month}"),
                    &credited,
                    "Uber One credit",
                    9.99,
                ),
            ]
        })
        .collect::<Vec<_>>();
    let current = window(&transactions, "2026-12-20", "uber-one");
    assert_eq!(current.credited_amount, 119.88);
    assert_eq!(current.remaining_amount, Some(0.0));
    assert_eq!(current.unusable_remainder, Some(0.12));
}

#[test]
fn uber_cash_without_a_card_charge_is_not_visible_rather_than_missed() {
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        txn("ride", "2026-07-09", "Uber", -24.1),
    ];
    let months = outcomes(&transactions, "2026-06-01", "2026-08-20", "uber-cash");
    let states = months
        .iter()
        .map(|outcome| (outcome.window_end, outcome.state, outcome.unused))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            (day("2026-06-30"), OutcomeState::NotVisible, None),
            (day("2026-07-31"), OutcomeState::Settled, Some(0.0)),
        ]
    );
}

#[test]
fn only_settled_windows_that_the_history_covers_can_be_missed() {
    let transactions = [
        txn("history", "2026-02-10", "Coffee", -4.0),
        txn("disney", "2026-02-15", "Disney Plus", -13.99),
        txn(
            "credit",
            "2026-02-17",
            "Digital entertainment credit",
            13.99,
        ),
    ];
    let months = outcomes(
        &transactions,
        "2026-01-01",
        "2026-04-10",
        "digital-entertainment",
    );
    let states = months
        .iter()
        .map(|outcome| (outcome.window_end, outcome.state, outcome.unused))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            (day("2026-01-31"), OutcomeState::PartialHistory, None),
            (day("2026-02-28"), OutcomeState::PartialHistory, None),
            (day("2026-03-31"), OutcomeState::Settling, None),
        ]
    );
    assert!(months.iter().all(|outcome| outcome.engaged));
    let later = outcomes(
        &transactions,
        "2026-03-01",
        "2026-06-01",
        "digital-entertainment",
    );
    assert_eq!(later[0].state, OutcomeState::Settled);
    assert_eq!(later[0].unused, Some(25.0));
}

#[test]
fn an_unused_benefit_is_unclaimed_rather_than_missed() {
    let transactions = [txn("history", "2026-01-01", "Coffee", -4.0)];
    let quarters = outcomes(&transactions, "2026-01-01", "2026-06-15", "lululemon");
    assert_eq!(quarters.len(), 1);
    assert_eq!(quarters[0].state, OutcomeState::Settled);
    assert_eq!(quarters[0].unused, Some(75.0));
    assert!(!quarters[0].engaged);
    let (_, missed) = credit_summary(&[], &quarters, day("2026-01-01"), day("2026-06-30"));
    assert_eq!(missed, 0.0);
}

#[test]
fn a_learned_credit_delay_tracks_late_and_missing_credits() {
    let mut transactions = (6..=8)
        .flat_map(|month| {
            [
                txn(
                    &format!("fee-{month}"),
                    &format!("2026-{month:02}-01"),
                    "Peacock",
                    -10.0,
                ),
                txn(
                    &format!("credit-{month}"),
                    &format!("2026-{month:02}-03"),
                    "Digital entertainment credit",
                    10.0,
                ),
            ]
        })
        .collect::<Vec<_>>();
    transactions.extend([
        txn("old", "2026-07-10", "Disney Plus", -13.99),
        txn("late", "2026-09-01", "Disney Plus", -13.99),
        txn("fresh", "2026-09-08", "Hulu", -9.99),
    ]);
    let items = activity(&transactions, "2026-09-10");
    assert_eq!(
        item(&items, "old").credit,
        Some(PurchaseCredit::NotCredited)
    );
    assert_eq!(
        item(&items, "late").credit,
        Some(PurchaseCredit::Late {
            amount: 13.99,
            by: day("2026-10-27")
        })
    );
    assert_eq!(
        item(&items, "fresh").credit,
        Some(PurchaseCredit::Expected {
            amount: 9.99,
            by: day("2026-09-13")
        })
    );
    let current = window(&transactions, "2026-09-10", "digital-entertainment");
    assert!(current.credit_lag_learned);
    assert_eq!(current.credit_lag_days, 5);
    assert_eq!(current.awaiting_credit_amount, 23.98);
}

#[test]
fn a_purchase_after_the_allowance_is_used_expects_no_credit() {
    let transactions = [
        txn("first", "2026-09-01", "Disney Plus", -25.0),
        txn("credit", "2026-09-02", "Digital entertainment credit", 25.0),
        txn("second", "2026-09-05", "Hulu", -9.99),
    ];
    let items = activity(&transactions, "2026-09-06");
    assert_eq!(
        item(&items, "second").credit,
        Some(PurchaseCredit::OverAllowance)
    );
}

#[test]
fn credits_count_against_the_fee_from_its_posting_or_from_january_without_one() {
    let transactions = vec![
        txn(
            "old-credit",
            "2026-02-10",
            "Platinum Digital Entertainment Credit",
            20.0,
        ),
        txn("fee", "2026-03-01", "Annual Membership Fee", -895.0),
        txn("disney", "2026-08-15", "Disney Plus", -13.99),
        txn(
            "credit",
            "2026-08-18",
            "Platinum Digital Entertainment Credit",
            13.99,
        ),
        txn("resy-buy", "2026-08-20", "Resy Example Bistro", -80.0),
        txn("resy-credit", "2026-08-23", "Platinum Resy Credit", 80.0),
        txn("resy-reversal", "2026-08-25", "Platinum Resy Credit", -30.0),
        txn(
            "generic-refund",
            "2026-08-26",
            "Example Store Refund",
            500.0,
        ),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let activity = activity(&transactions, "2026-09-16");
    let recovery = |refs: &[&Transaction], hidden: &[String]| {
        PLATINUM
            .fee_recovery(
                refs,
                &activity,
                hidden,
                day("2026-09-16"),
                &noon("2026-09-16"),
                &NamedZone::Utc,
            )
            .unwrap()
    };
    // Only issuer credits after the fee posted count: 13.99 + 80 - 30.
    assert_eq!(
        recovery(&refs, &[]),
        FeeRecovery {
            annual_fee: 895.0,
            recovered: 63.99,
            for_others: 0.0,
            since: day("2026-03-01"),
            basis: "fee-charge"
        }
    );
    assert_eq!(recovery(&refs, &["resy".into()]).recovered, 13.99);
    // A credit on someone else's purchase used the allowance, but saved them the money.
    let mut shared = transactions.clone();
    shared
        .iter_mut()
        .filter(|t| t.id == "credit")
        .for_each(|t| t.other = true);
    let shared_refs = shared.iter().collect::<Vec<_>>();
    let shared_recovery = recovery(&shared_refs, &[]);
    assert_eq!(
        (shared_recovery.recovered, shared_recovery.for_others),
        (50.0, 13.99)
    );
    // Without a fee charge in the history the year is the calendar year, and says so.
    let without_fee = refs
        .iter()
        .copied()
        .filter(|t| t.id != "fee")
        .collect::<Vec<_>>();
    assert_eq!(
        recovery(&without_fee, &[]),
        FeeRecovery {
            annual_fee: 895.0,
            recovered: 83.99,
            for_others: 0.0,
            since: day("2026-01-01"),
            basis: "calendar-year"
        }
    );
}

#[test]
fn an_issuer_credit_is_read_from_the_banks_own_descriptor() {
    // Plaid can clean an issuer credit into a different name and file it as income; the bank's
    // descriptor still says what it is, even after the benefit retired.
    let mut credit = txn("saks-credit", "2026-07-01", "Goodwill", 50.0);
    credit.description = Some("Goodwill".into());
    credit.original_description = Some("SAKS BENEFIT GOODWILL".into());
    credit.category = "Income".into();
    let purchase = txn("saks", "2026-06-28", "Saks Fifth Avenue", -60.0);
    let transactions = vec![purchase, credit.clone()];
    let items = activity(&transactions, "2026-07-10");
    let found = item(&items, "saks-credit");
    assert_eq!(
        (found.benefit_id.as_str(), found.kind),
        ("saks", ActivityKind::Credit)
    );
    assert_eq!(found.paired_id.as_deref(), Some("saks"));
    let classified = crate::transaction_policy::classify(&credit);
    assert_eq!(
        (classified.kind, classified.credit),
        (crate::transaction_policy::Kind::Reimbursement, true)
    );
    // A thrift store refund is not a Saks credit.
    let thrift = txn("thrift", "2026-07-02", "Goodwill", 50.0);
    assert!(activity(&[thrift], "2026-07-10").is_empty());
}

#[test]
fn credits_to_use_first_lead_with_the_soonest_deadline_and_the_most_left() {
    let base = window(&[], "2026-09-14", "resy");
    let with = |id: &str, cadence: Cadence, end: &str, high: Option<f64>| BenefitWindow {
        id: id.into(),
        name: id.into(),
        cadence,
        window_end: day(end),
        remaining_range: high.map(|high| AmountRange { low: 0.0, high }),
        ..base.clone()
    };
    let windows = [
        with("hotel", Cadence::Annual, "2026-12-31", Some(300.0)),
        with("renewal", Cadence::Renewal, "2026-09-30", Some(50.0)),
        with("uber", Cadence::Monthly, "2026-09-30", Some(15.0)),
        with("used", Cadence::Monthly, "2026-09-30", Some(0.0)),
        with("untracked", Cadence::Monthly, "2026-09-30", None),
        with("streaming", Cadence::Monthly, "2026-09-30", Some(25.0)),
    ];
    assert_eq!(
        use_first(&windows),
        ["streaming", "uber", "hotel", "renewal"]
    );
}

#[test]
fn unused_allowance_is_missed_only_when_the_card_used_the_benefit() {
    assert_eq!(
        Verdict::of(OutcomeState::Settled, true),
        Some(Verdict::Missed)
    );
    assert_eq!(
        Verdict::of(OutcomeState::Settled, false),
        Some(Verdict::Unclaimed)
    );
    assert_eq!(
        Verdict::of(OutcomeState::Settling, false),
        Some(Verdict::Settling)
    );
    assert_eq!(
        Verdict::of(OutcomeState::PartialHistory, true),
        Some(Verdict::Unknown)
    );
    assert_eq!(Verdict::of(OutcomeState::NotVisible, false), None);
    assert_eq!(Verdict::of(OutcomeState::NotTracked, true), None);
}
