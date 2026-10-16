use chrono::NaiveDate;
use serde_json::json;

use super::history::{BenefitPeriod, PeriodState, PlannedCredit, PlannedSource, ReviewKind};
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

fn window(transactions: &[Transaction], today: &str, id: &str) -> BenefitWindow {
    let refs = transactions.iter().collect::<Vec<_>>();
    PLATINUM
        .tracker(&refs, &noon(today), &noon(today), &NamedZone::Utc)
        .into_iter()
        .find(|window| window.id == id)
        .unwrap()
}

fn history(transactions: &[Transaction], today: &str) -> Vec<BenefitHistory> {
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = PLATINUM.activity(&refs, &noon(today));
    let start = refs
        .iter()
        .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
        .min();
    PLATINUM.history(&matched, start, day(today), &[])
}

/// A benefit's periods that started on or after `from` and ended before `today`.
fn closed(transactions: &[Transaction], from: &str, today: &str, id: &str) -> Vec<BenefitPeriod> {
    history(transactions, today)
        .into_iter()
        .find(|benefit| benefit.id == id)
        .unwrap()
        .periods
        .into_iter()
        .filter(|period| period.start >= day(from) && period.end < day(today))
        .collect()
}

fn activity(transactions: &[Transaction], today: &str) -> Vec<BenefitActivity> {
    let refs = transactions.iter().collect::<Vec<_>>();
    PLATINUM.activity(&refs, &noon(today))
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
    let ended = closed(&transactions, "2026-01-01", "2026-06-15", "resy");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].state, PeriodState::Full);
    assert_eq!(ended[0].unused, range(0.0, 0.0));
    assert_eq!(ended[0].activity_ids, ["credit", "dinner"]);
}

#[test]
fn a_purchase_counts_toward_the_window_of_its_transaction_date_not_its_posting() {
    // Processed on the last day of a window, posted in the next, as the issuer counts it.
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        dated(
            "booking",
            "2026-06-30",
            "2026-07-01",
            "AMEX Fine Hotels and AmexTravel",
            -557.87,
        ),
        dated(
            "hotel",
            "2026-06-30",
            "2026-07-02",
            "Platinum Hotel Credit",
            300.0,
        ),
        dated("leggings", "2026-09-29", "2026-10-01", "lululemon", -107.56),
        dated(
            "lulu",
            "2026-09-29",
            "2026-10-02",
            "Platinum lululemon Credit",
            75.0,
        ),
    ];
    let periods = |id: &str| {
        history(&transactions, "2026-10-14")
            .into_iter()
            .find(|benefit| benefit.id == id)
            .unwrap()
            .periods
    };
    let period = |id: &str, start: &str| {
        periods(id)
            .into_iter()
            .find(|period| period.start == day(start))
            .unwrap()
    };
    assert_eq!(period("hotel", "2026-01-01").credited, 300.0);
    assert_eq!(period("hotel", "2026-07-01").credited, 0.0);
    assert_eq!(period("lululemon", "2026-07-01").credited, 75.0);
    assert_eq!(period("lululemon", "2026-07-01").state, PeriodState::Full);
    // The next quarter keeps its whole allowance.
    let current = window(&transactions, "2026-10-14", "lululemon");
    assert_eq!(current.window_start, day("2026-10-01"));
    assert_eq!(current.remaining_amount, Some(75.0));
}

#[test]
fn a_quarter_the_history_reaches_partway_is_judged_by_what_it_shows() {
    // The history starts in May, inside the second quarter; its credit leaves twenty cents.
    let transactions = [
        txn("history", "2026-05-09", "Coffee", -4.0),
        txn("lulu", "2026-05-11", "Platinum lululemon Credit", 74.8),
    ];
    let quarter = closed(&transactions, "2026-04-01", "2026-08-20", "lululemon")
        .into_iter()
        .find(|period| period.start == day("2026-04-01"))
        .unwrap();
    assert_eq!(quarter.state, PeriodState::Full);
    // Exactly what the credits it shows leave, not a range.
    assert_eq!(quarter.unused, range(0.2, 0.2));
}

#[test]
fn a_window_that_leaves_only_cents_was_used_in_full_and_still_counts_them() {
    let month = |id: &str, date: &str| {
        [
            txn(&format!("{id}-charge"), date, "Disney Plus", -24.98),
            txn(
                &format!("{id}-credit"),
                date,
                "Platinum Digital Entertainment Credit",
                24.98,
            ),
        ]
    };
    let transactions = [
        vec![txn("history", "2026-07-01", "Coffee", -4.0)],
        month("august", "2026-08-03").to_vec(),
        month("september", "2026-09-03").to_vec(),
    ]
    .concat();
    // A settled month: full, though its two cents still count as missed.
    let august = closed(
        &transactions,
        "2026-08-01",
        "2026-09-20",
        "digital-entertainment",
    );
    assert_eq!(august[0].state, PeriodState::Full);
    assert_eq!(august[0].unused, range(0.02, 0.02));
    // The open month has nothing to spend, so it isn't a credit to use first.
    let current = window(&transactions, "2026-09-20", "digital-entertainment");
    assert_eq!(current.remaining_range, range(0.0, 0.0));
    assert_eq!(current.unusable_remainder, Some(0.02));
    assert!(!use_first(&[current]).contains(&"digital-entertainment".to_owned()));
}

#[test]
fn an_unpaired_credit_carries_its_window_in_its_transaction_date() {
    // Dated in March though it posted in April, so it can only be March's.
    let transactions = [dated(
        "credit",
        "2026-03-31",
        "2026-04-02",
        "Resy credit",
        50.0,
    )];
    let items = activity(&transactions, "2026-04-20");
    assert_eq!(
        item(&items, "credit").credit_window,
        Some(day("2026-01-01"))
    );
    assert_eq!(item(&items, "credit").alternate_credit_window, None);
    let current = window(&transactions, "2026-04-20", "resy");
    assert!(!current.period_uncertain);
}

#[test]
fn a_credit_is_due_a_delay_after_its_purchase_posts() {
    let transactions = [
        txn("history", "2026-06-01", "Coffee", -4.0),
        dated("leggings", "2026-09-29", "2026-10-01", "lululemon", -107.56),
    ];
    let items = activity(&transactions, "2026-10-03");
    // Seven days from posting, not from the transaction date.
    assert_eq!(
        item(&items, "leggings").credit,
        Some(PurchaseCredit::Expected {
            amount: 75.0,
            by: day("2026-10-08"),
        })
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
    let months = closed(&transactions, "2026-06-01", "2026-08-20", "uber-cash");
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
    // March's credits may still post within the usual week after it ends.
    let months = closed(
        &transactions,
        "2026-01-01",
        "2026-04-05",
        "digital-entertainment",
    );
    let states = months
        .iter()
        .map(|period| (period.end, period.state, period.unused))
        .collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            // The history starts after January, with nothing of it to judge.
            (day("2026-01-31"), PeriodState::BeforeHistory, None),
            // It reaches into February, which is judged by the credits it shows.
            (day("2026-02-28"), PeriodState::Partial, range(11.01, 11.01)),
            (day("2026-03-31"), PeriodState::Settling, None),
        ]
    );
    assert_eq!(months[1].credited, 13.99);
    let later = closed(
        &transactions,
        "2026-03-01",
        "2026-06-01",
        "digital-entertainment",
    );
    assert_eq!(later[0].state, PeriodState::Missed);
    assert_eq!(later[0].unused, range(25.0, 25.0));
}

#[test]
fn an_unused_benefit_is_missed_whether_or_not_the_card_ever_used_it() {
    // Hiding a benefit, which the renderer applies, is what says its allowance doesn't matter.
    let transactions = [txn("history", "2026-01-01", "Coffee", -4.0)];
    let quarters = closed(&transactions, "2026-01-01", "2026-06-15", "lululemon");
    assert_eq!(quarters.len(), 1);
    assert_eq!(quarters[0].state, PeriodState::Missed);
    assert_eq!(quarters[0].unused, range(75.0, 75.0));
    let all = history(&transactions, "2026-06-15");
    let lululemon = all
        .iter()
        .filter(|b| b.id == "lululemon")
        .cloned()
        .collect::<Vec<_>>();
    let (_, missed) = credit_summary(&[], &lululemon, day("2026-01-01"), day("2026-06-30"));
    assert_eq!(missed, 75.0);
}

#[test]
fn an_unpaired_credit_is_placed_by_the_usual_delay_not_the_terms_limit() {
    // Posted three weeks into October: under the terms' eight weeks it could be September's,
    // but credits usually post within a week, so it is October's.
    let transactions = [
        txn("history", "2026-08-01", "Coffee", -4.0),
        txn("credit", "2026-10-22", "Platinum Resy Credit", 60.0),
    ];
    let current = window(&transactions, "2026-10-25", "resy");
    assert!(!current.period_uncertain);
    assert!(!current.credit_lag_learned);
    assert_eq!(current.credit_lag_days, 7);
    assert_eq!(current.remaining_amount, Some(40.0));
}

#[test]
fn a_window_that_began_before_the_card_history_is_judged_by_what_it_shows() {
    let transactions = [
        txn("history", "2026-05-09", "Coffee", -4.0),
        txn("credit", "2026-06-10", "Platinum Airline Fee Credit", 50.0),
    ];
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = PLATINUM.activity(&refs, &noon("2026-10-13"));
    let windows = PLATINUM.tracker_for(
        &matched,
        &noon("2026-10-13"),
        &noon("2026-10-13"),
        Some(day("2026-05-09")),
        &NamedZone::Utc,
    );
    let airline = windows.iter().find(|w| w.id == "airline-fee").unwrap();
    // Exactly what its credits leave; the date says where the history starts.
    assert_eq!(airline.history_from, Some(day("2026-05-09")));
    assert_eq!(airline.remaining_range, range(150.0, 150.0));
    assert_eq!(airline.remaining_amount, Some(150.0));
    // A window the history covers is unaffected.
    let resy = windows.iter().find(|w| w.id == "resy").unwrap();
    assert_eq!(resy.history_from, None);
    assert_eq!(resy.remaining_amount, Some(100.0));
}

#[test]
fn global_entry_renews_after_four_years_and_tsa_precheck_after_four_and_a_half() {
    let renewal = |amount: f64| {
        let transactions = [txn(
            "credit",
            "2026-03-31",
            "Platinum Global Entry Credit",
            amount,
        )];
        window(&transactions, "2026-04-02", "global-entry").renewal_check
    };
    assert_eq!(renewal(120.0).as_deref(), Some("Mar 31, 2030"));
    assert_eq!(renewal(78.0).as_deref(), Some("Oct 1, 2030"));
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
    let recovery = |refs: &[&Transaction]| {
        PLATINUM
            .fee_recovery(refs, &activity, &[], day("2026-09-16"))
            .unwrap()
    };
    // Only issuer credits after the fee posted count: 13.99 + 80 - 30. Hiding a benefit stops
    // its reminders, not the money it returned, so there is no visibility input.
    assert_eq!(
        recovery(&refs),
        FeeRecovery {
            annual_fee: 895.0,
            recovered: 63.99,
            for_others: 0.0,
            since: day("2026-03-01"),
            basis: "fee-charge",
            // A year after the fee posted.
            through: day("2027-02-28"),
            expected: 0.0,
        }
    );
    // A credit on someone else's purchase used the allowance, but saved them the money.
    let mut shared = transactions.clone();
    shared
        .iter_mut()
        .filter(|t| t.id == "credit")
        .for_each(|t| t.other = true);
    let shared_refs = shared.iter().collect::<Vec<_>>();
    let shared_recovery = recovery(&shared_refs);
    assert_eq!(
        (shared_recovery.recovered, shared_recovery.for_others),
        (50.0, 13.99)
    );
    // Without a fee charge, the year starts in January, or where the history starts when
    // that is later, and says which.
    let without_fee = refs
        .iter()
        .copied()
        .filter(|t| t.id != "fee")
        .collect::<Vec<_>>();
    assert_eq!(
        recovery(&without_fee),
        FeeRecovery {
            annual_fee: 895.0,
            recovered: 83.99,
            for_others: 0.0,
            since: day("2026-02-10"),
            basis: "card-history",
            // Without a fee charge the anniversary is unknown, so the year ends December 31.
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
    let fee = txn("fee", "2026-03-01", "Annual Membership Fee", -895.0);
    let mut theirs = txn("theirs", "2026-09-10", "Resy Example Bistro", -40.0);
    theirs.other = true;
    let transactions = vec![fee, theirs];
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
        cap: 100.0,
        credited: 0.0,
        state: PeriodState::Open,
        unused: None,
        left: None,
        activity_ids: Vec::new(),
        planned,
    };
    let history = vec![BenefitHistory {
        id: "resy".into(),
        name: "Resy".into(),
        recurring: false,
        external: false,
        retired: false,
        periods: vec![
            period(vec![
                credit("Example streaming", "2026-10-05", 25.0, None),
                // Someone else's purchase earns them the credit.
                credit("Resy Example Bistro", "2026-09-10", 40.0, Some("theirs")),
            ]),
            // After the fee year ends, so it belongs to the next one.
            period(vec![credit("Example streaming", "2027-03-05", 25.0, None)]),
        ],
    }];
    let recovery = PLATINUM
        .fee_recovery(&refs, &[], &history, day("2026-09-16"))
        .unwrap();
    assert_eq!(
        (recovery.through, recovery.expected),
        (day("2027-02-28"), 25.0)
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
fn a_credit_that_posts_before_its_purchase_suggests_the_benefits_own_merchant() {
    // The credit posted off the authorization, before a purchase that posted after the
    // benefit retired. A larger, earlier charge elsewhere is not the likely purchase.
    let mut credit = txn("saks-credit", "2026-07-01", "Goodwill", 50.0);
    credit.original_description = Some("SAKS BENEFIT GOODWILL".into());
    let mut transactions = vec![
        txn("history", "2026-01-02", "Coffee", -4.0),
        txn("insurance", "2026-06-25", "Example Insurance", -900.0),
        txn("saks", "2026-07-03", "Saks Fifth Avenue", -60.0),
        credit,
    ];
    assert_eq!(
        review(&transactions, "2026-07-10"),
        [(
            ReviewKind::NoPurchase,
            "saks-credit".into(),
            Some("saks".into())
        )]
    );
    // Linking it pairs them, despite the order and the retirement.
    transactions[2].benefit_id = Some("saks".into());
    assert_eq!(review(&transactions, "2026-07-10"), []);
    let items = activity(&transactions, "2026-07-10");
    assert_eq!(
        item(&items, "saks-credit").paired_id.as_deref(),
        Some("saks")
    );
    assert_eq!(
        item(&items, "saks").credit,
        Some(PurchaseCredit::Credited { amount: 50.0 })
    );
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
    // Uber Cash has no tracked remainder: it is listed until a card charge suggests it ran out.
    let uber_cash = |id: &str, estimated: f64| BenefitWindow {
        cap: 15.0,
        estimated_credit_amount: Some(estimated),
        ..with(id, Cadence::Monthly, "2026-09-30", None)
    };
    let windows = [
        with("hotel", Cadence::Annual, "2026-12-31", Some(300.0)),
        with("renewal", Cadence::Renewal, "2026-09-30", Some(50.0)),
        with("lululemon", Cadence::Monthly, "2026-09-30", Some(15.0)),
        with("used", Cadence::Monthly, "2026-09-30", Some(0.0)),
        with("untracked", Cadence::Monthly, "2026-09-30", None),
        with("resy", Cadence::Monthly, "2026-09-30", Some(25.0)),
        BenefitWindow {
            recurring: true,
            ..with("streaming", Cadence::Monthly, "2026-09-30", Some(25.0))
        },
        uber_cash("uber-unused", 0.0),
        uber_cash("uber-used", 15.0),
    ];
    assert_eq!(
        use_first(&windows),
        ["resy", "lululemon", "uber-unused", "hotel", "renewal"]
    );
}

#[test]
fn a_change_of_cadence_ends_the_old_allowance_the_day_before() {
    // The hotel credit was $200 a year until Sep 18, 2025, then $300 a half-year.
    let transactions = [txn("history", "2025-01-05", "Coffee", -4.0)];
    let hotel = history(&transactions, "2026-02-10")
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
            ("Jan 1 – Sep 17", 200.0, PeriodState::BeforeHistory),
            ("Sep 18 – Dec 31", 300.0, PeriodState::Missed),
            ("Jan – Jun", 300.0, PeriodState::Open),
            ("Jul – Dec", 300.0, PeriodState::Future),
        ]
    );
    assert_eq!(hotel.periods[2].left, range(300.0, 300.0));
}

fn review(transactions: &[Transaction], today: &str) -> Vec<(ReviewKind, String, Option<String>)> {
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = PLATINUM.activity(&refs, &noon(today));
    PLATINUM
        .review(&refs, &matched)
        .into_iter()
        .map(|item| (item.kind, item.transaction_id, item.suggestion_id))
        .collect()
}

#[test]
fn matches_that_need_a_decision_are_queued_until_one_is_saved() {
    let mut transactions = vec![
        txn("history", "2026-01-02", "Coffee", -4.0),
        txn("store", "2026-05-20", "Disney Store", -42.18),
        txn("dinner", "2026-08-12", "Carbone", -186.4),
        txn("resy-credit", "2026-08-14", "Platinum Resy Credit", 100.0),
        txn(
            "amex-credit",
            "2026-09-03",
            "AMEX PLATINUM STATEMENT CREDIT",
            50.0,
        ),
        txn("return", "2026-09-10", "lululemon", 32.0),
    ];
    let some = |id: &str| Some(id.to_owned());
    assert_eq!(
        review(&transactions, "2026-09-20"),
        [
            (ReviewKind::UnverifiedCredit, "return".into(), None),
            (ReviewKind::UnassignedCredit, "amex-credit".into(), None),
            (ReviewKind::NoPurchase, "resy-credit".into(), some("dinner")),
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
    decide(&mut transactions, "amex-credit", &|t| {
        t.benefit_id = some("resy")
    });
    decide(&mut transactions, "dinner", &|t| {
        t.benefit_id = some("resy")
    });
    decide(&mut transactions, "store", &|t| {
        t.benefit_confirmed = Some(false)
    });
    assert_eq!(review(&transactions, "2026-09-20"), []);
    let items = activity(&transactions, "2026-09-20");
    assert_eq!(
        (
            item(&items, "return").kind,
            item(&items, "return").benefit_id.as_str()
        ),
        (ActivityKind::Credit, "lululemon")
    );
    assert_eq!(item(&items, "amex-credit").kind, ActivityKind::Credit);
    assert_eq!(item(&items, "resy-credit").paired_id, some("dinner"));
    assert!(items.iter().all(|item| item.id != "store"));
}

#[test]
fn confirming_an_uncredited_purchase_keeps_it_a_purchase() {
    let mut store = txn("store", "2026-05-20", "Disney Store", -42.18);
    store.benefit_confirmed = Some(true);
    let transactions = [txn("history", "2026-01-02", "Coffee", -4.0), store];
    let items = activity(&transactions, "2026-09-20");
    assert_eq!(item(&items, "store").kind, ActivityKind::Purchase);
    assert_eq!(
        item(&items, "store").credit,
        Some(PurchaseCredit::NotCredited)
    );
    assert_eq!(review(&transactions, "2026-09-20"), []);
}

#[test]
fn planned_credits_fill_open_and_future_periods_from_waiting_purchases_then_subscriptions() {
    use super::history::{PlannedCredit, PlannedSource};
    let transactions = [
        txn("history", "2026-01-02", "Coffee", -4.0),
        txn("disney", "2026-10-02", "Disney Plus", -15.99),
        txn(
            "credit",
            "2026-10-03",
            "Digital entertainment credit",
            15.99,
        ),
        txn("hulu", "2026-10-12", "Hulu", -6.0),
    ];
    let youtube = txn("youtube", "2026-09-20", "YouTube Premium", -13.99);
    assert_eq!(
        PLATINUM.purchase_benefit(&youtube).as_deref(),
        Some("digital-entertainment")
    );
    let charges = ["2026-10-20", "2026-11-20"]
        .map(|date| PlannedCharge {
            benefit_id: "digital-entertainment".into(),
            date: day(date),
            amount: -13.99,
            title: "YouTube Premium".into(),
        })
        .to_vec();
    let refs = transactions.iter().collect::<Vec<_>>();
    let matched = PLATINUM.activity(&refs, &noon("2026-10-13"));
    let streaming = PLATINUM
        .history(
            &matched,
            Some(day("2026-01-02")),
            day("2026-10-13"),
            &charges,
        )
        .into_iter()
        .find(|benefit| benefit.id == "digital-entertainment")
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
    // October has $9.01 left: Hulu's waiting credit first, then part of YouTube.
    assert_eq!(
        month("2026-10-01"),
        [
            PlannedCredit {
                title: "Hulu".into(),
                date: day("2026-10-12"),
                amount: 6.0,
                source: PlannedSource::Purchase,
                transaction_id: Some("hulu".into()),
            },
            PlannedCredit {
                title: "YouTube Premium".into(),
                date: day("2026-10-20"),
                amount: 3.01,
                source: PlannedSource::Recurring,
                transaction_id: None,
            },
        ]
    );
    assert_eq!(month("2026-11-01")[0].amount, 13.99);
    // A closed month plans nothing.
    assert!(month("2026-09-01").is_empty());
}
