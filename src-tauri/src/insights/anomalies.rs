//! Charges worth a second look: a possible duplicate (two identical posted charges from one
//! merchant on one day) and an unusual amount (far above what a merchant steadily charges).
//! Both are deliberately strict, because a false alarm costs trust: repeated small fares,
//! three identical tickets, and pending holds are left alone, and an amount is unusual only
//! where nearly every earlier charge was the same, such as a bill, a membership, or a fixed
//! fare. A meal, a grocery run, or a shopping trip costs what you chose to buy that day, so
//! those merchants are never flagged, however alike their past charges. Marking a transaction
//! reviewed dismisses it. Amounts compare in whole cents.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::calendar;
use crate::finance_contract::Transaction;
use crate::numeric::{median_i64, minor_units};
use crate::transaction_policy::Kind;

/// Old enough that a dispute is still timely.
pub const RECENT_DAYS: i64 = 60;
/// Smaller identical charges (transit fares, vending) repeat legitimately.
const DUPLICATE_MINIMUM: i64 = 1_000;
/// A usual amount needs this many earlier charges.
const HISTORY_MINIMUM: usize = 5;
/// How far back the usual amount looks.
const HISTORY_DAYS: i64 = 365;
/// Above the usual amount by at least this much, in cents.
const UNUSUAL_MINIMUM: i64 = 2_000;
/// A merchant is steady when at least four in five earlier charges lie within this percent
/// of the usual amount, or within `STEADY_BAND_MINIMUM` cents of it if that is wider. Only
/// then is "usually" true of the amount, and a departure from it news.
const STEADY_BAND_PERCENT: i64 = 5;
const STEADY_BAND_MINIMUM: i64 = 100;
/// Categories where each visit's amount is whatever was bought that day.
const CHOSEN_BASKET: [&str; 3] = ["Dining", "Groceries", "Shopping"];

/// Whether nearly every earlier charge, in cents, was the usual amount.
fn steady(earlier: &[i64], usual: i64) -> bool {
    let band = (usual.abs() * STEADY_BAND_PERCENT / 100).max(STEADY_BAND_MINIMUM);
    let alike = earlier
        .iter()
        .filter(|amount| (*amount - usual).abs() <= band)
        .count();
    alike * 5 >= earlier.len() * 4
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DuplicateCharge")]
pub struct PossibleDuplicate {
    pub id: String,
    pub account_id: String,
    pub merchant: String,
    /// Negative, like each charge.
    pub amount: f64,
    pub date: NaiveDate,
    /// The earlier-listed charge first.
    pub transaction_ids: [String; 2],
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnusualCharge {
    pub transaction_id: String,
    pub merchant: String,
    pub amount: f64,
    /// The median of the merchant's earlier charges, negative.
    pub usual: f64,
    /// How many earlier charges the usual amount comes from.
    pub observations: usize,
    /// Those earlier charges, oldest first; not part of the contract.
    #[serde(skip)]
    pub earlier: Vec<String>,
}

impl UnusualCharge {
    /// For `n` earlier charges drawn alike, the chance another lands outside their range is
    /// 2 / (n + 1); this one lies well beyond it, so (n − 1) / (n + 1) is a floor on how sure
    /// Brief is that it differs.
    pub fn confidence(&self) -> f64 {
        let n = self.observations as f64;
        (n - 1.0) / (n + 1.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Anomalies {
    pub duplicates: Vec<PossibleDuplicate>,
    pub unusual: Vec<UnusualCharge>,
}

/// One identity per merchant: Plaid's entity when known, otherwise the name's letters and
/// digits.
fn merchant_key(transaction: &Transaction) -> String {
    transaction.merchant_entity_id.clone().unwrap_or_else(|| {
        transaction
            .merchant
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect()
    })
}

/// A usual amount comes only from charges on or after `since`, a confirmed chapter's start,
/// so a former life's prices don't set the new normal.
pub fn anomalies(
    transactions: &[Transaction],
    since: Option<NaiveDate>,
    today: NaiveDate,
) -> Anomalies {
    let recent_from = today - Duration::days(RECENT_DAYS);
    // Posted charges, with their day.
    let charges = transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction.amount < 0.0
                // A charge made for someone is still a charge on your card.
                && matches!(transaction.kind(), Some(Kind::Expense | Kind::Owed))
        })
        .filter_map(|transaction| {
            Some((calendar::transaction_date(&transaction.date)?, transaction))
        })
        .collect::<Vec<_>>();
    let unreviewed = |transaction: &Transaction| transaction.reviewed != Some(true);

    let mut groups = BTreeMap::<(String, String, i64, NaiveDate), Vec<&Transaction>>::new();
    for (date, transaction) in charges.iter().filter(|(date, _)| *date >= recent_from) {
        let amount = minor_units(transaction.amount);
        if -amount < DUPLICATE_MINIMUM {
            continue;
        }
        let account = transaction.account_id.clone().unwrap_or_default();
        groups
            .entry((account, merchant_key(transaction), amount, *date))
            .or_default()
            .push(transaction);
    }
    let mut duplicates = groups
        .into_iter()
        .filter_map(|((account_id, _, amount, date), mut pair)| {
            // Three or more identical charges are usually several of the same thing.
            if pair.len() != 2 || !pair.iter().all(|transaction| unreviewed(transaction)) {
                return None;
            }
            // A posted charge and the pending one it replaced are one charge.
            let replaced = |left: &Transaction, right: &Transaction| {
                left.pending_transaction_id.as_deref() == Some(right.id.as_str())
            };
            if replaced(pair[0], pair[1]) || replaced(pair[1], pair[0]) {
                return None;
            }
            pair.sort_by(|left, right| left.id.cmp(&right.id));
            Some(PossibleDuplicate {
                id: format!("duplicate:{}:{}", pair[0].id, pair[1].id),
                account_id,
                merchant: pair[0].merchant.clone(),
                amount: amount as f64 / 100.0,
                date,
                transaction_ids: [pair[0].id.clone(), pair[1].id.clone()],
            })
        })
        .collect::<Vec<_>>();
    duplicates.sort_by(|left, right| right.date.cmp(&left.date).then(left.id.cmp(&right.id)));

    let mut by_merchant = BTreeMap::<String, Vec<(NaiveDate, &Transaction)>>::new();
    for (date, transaction) in &charges {
        by_merchant
            .entry(merchant_key(transaction))
            .or_default()
            .push((*date, transaction));
    }
    let mut unusual = Vec::new();
    for history in by_merchant.values() {
        for (date, transaction) in history.iter().filter(|(date, transaction)| {
            *date >= recent_from
                && unreviewed(transaction)
                && !CHOSEN_BASKET.contains(&transaction.category.as_str())
        }) {
            let mut prior = history
                .iter()
                .filter(|(other, _)| {
                    other < date
                        && *other >= *date - Duration::days(HISTORY_DAYS)
                        && since.is_none_or(|since| *other >= since)
                })
                .collect::<Vec<_>>();
            prior.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));
            let mut earlier = prior
                .iter()
                .map(|(_, other)| -minor_units(other.amount))
                .collect::<Vec<_>>();
            if earlier.len() < HISTORY_MINIMUM {
                continue;
            }
            let observations = earlier.len();
            let Some(usual) = median_i64(&mut earlier) else {
                continue;
            };
            if !steady(&earlier, usual) {
                continue;
            }
            let mut deviations = earlier
                .iter()
                .map(|amount| (amount - usual).abs())
                .collect::<Vec<_>>();
            // A robust spread: the median deviation scaled to a normal standard deviation.
            let spread = median_i64(&mut deviations).unwrap_or(0) as f64 * 1.4826;
            let amount = -minor_units(transaction.amount);
            let margin = (4.0 * spread).max(usual as f64 * 0.5);
            if amount - usual >= UNUSUAL_MINIMUM && (amount - usual) as f64 > margin {
                unusual.push(UnusualCharge {
                    transaction_id: transaction.id.clone(),
                    merchant: transaction.merchant.clone(),
                    amount: -amount as f64 / 100.0,
                    usual: -usual as f64 / 100.0,
                    observations,
                    earlier: prior.iter().map(|(_, other)| other.id.clone()).collect(),
                });
            }
        }
    }
    unusual.sort_by(|left, right| left.transaction_id.cmp(&right.transaction_id));
    Anomalies {
        duplicates,
        unusual,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn charge(id: &str, merchant: &str, date: &str, amount: f64) -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": merchant, "category": "Services", "date": date,
            "amount": amount, "account": "Card", "accountId": "plaid:card", "pending": false,
            "description": null, "categorySource": "provider",
            "classification": { "mark": "initial", "kind": "expense", "spending": true, "credit": false, "zelle": false }
        }))
        .unwrap()
    }

    fn run(transactions: &[Transaction]) -> Anomalies {
        anomalies(transactions, None, day("2026-09-20"))
    }

    #[test]
    fn flags_only_an_exact_unreviewed_pair_of_posted_charges() {
        let mut found = run(&[
            charge("b", "Hardware Store", "2026-09-12", -84.10),
            charge("a", "Hardware Store", "2026-09-12", -84.10),
            charge("c", "Hardware Store", "2026-09-13", -84.10),
        ])
        .duplicates;
        assert_eq!(found.len(), 1);
        let pair = found.remove(0);
        assert_eq!(pair.transaction_ids, ["a".to_owned(), "b".to_owned()]);
        assert_eq!((pair.amount, pair.date), (-84.10, day("2026-09-12")));

        let fares = [
            charge("a", "Transit", "2026-09-12", -2.90),
            charge("b", "Transit", "2026-09-12", -2.90),
        ];
        let tickets = [
            charge("a", "Cinema", "2026-09-12", -18.0),
            charge("b", "Cinema", "2026-09-12", -18.0),
            charge("c", "Cinema", "2026-09-12", -18.0),
        ];
        let mut reviewed = [
            charge("a", "Cafe", "2026-09-12", -24.0),
            charge("b", "Cafe", "2026-09-12", -24.0),
        ];
        reviewed[1].reviewed = Some(true);
        let mut replaced = [
            charge("a", "Hotel", "2026-09-12", -240.0),
            charge("b", "Hotel", "2026-09-12", -240.0),
        ];
        replaced[1].pending_transaction_id = Some("a".into());
        let old = [
            charge("a", "Store", "2026-06-01", -50.0),
            charge("b", "Store", "2026-06-01", -50.0),
        ];
        for transactions in [&fares[..], &tickets, &reviewed, &replaced, &old] {
            assert!(run(transactions).duplicates.is_empty());
        }
    }

    #[test]
    fn flags_a_charge_far_above_a_steady_merchant_but_not_a_varied_one() {
        let mut steady = (1..=6)
            .map(|month| {
                charge(
                    &format!("gym-{month}"),
                    "Gym",
                    &format!("2026-0{month}-05"),
                    -40.0,
                )
            })
            .collect::<Vec<_>>();
        steady.push(charge("gym-big", "Gym", "2026-09-05", -140.0));
        let found = run(&steady).unusual;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].transaction_id, "gym-big");
        assert_eq!((found[0].usual, found[0].observations), (-40.0, 6));
        // After a confirmed chapter starts, earlier charges no longer set the usual amount.
        let fresh = anomalies(&steady, Some(day("2026-05-01")), day("2026-09-20")).unusual;
        assert!(fresh.is_empty());

        let mut varied = [-30.0, -120.0, -65.0, -200.0, -90.0, -150.0]
            .iter()
            .enumerate()
            .map(|(index, amount)| {
                charge(
                    &format!("grocer-{index}"),
                    "Grocer",
                    &format!("2026-0{}-10", index + 1),
                    *amount,
                )
            })
            .collect::<Vec<_>>();
        varied.push(charge("grocer-new", "Grocer", "2026-09-10", -260.0));
        assert!(run(&varied).unusual.is_empty());
    }

    /// Six earlier charges at `merchant`, one a month, then `latest` in September.
    fn history(merchant: &str, category: &str, amounts: [f64; 6], latest: f64) -> Vec<Transaction> {
        amounts
            .iter()
            .chain([latest].iter())
            .enumerate()
            .map(|(index, amount)| {
                let mut transaction = charge(
                    &format!("{merchant}-{index}"),
                    merchant,
                    &format!("2026-0{}-12", index + 3),
                    *amount,
                );
                transaction.category = category.into();
                transaction
            })
            .collect()
    }

    #[test]
    fn an_amount_is_unusual_only_where_earlier_charges_were_nearly_all_the_same() {
        // Meals that cluster near one price still vary by what was ordered.
        let meals = [-26.40, -28.10, -29.75, -27.20, -31.05, -24.90];
        assert!(run(&history("Sandwich Shop", "Services", meals, -60.13))
            .unusual
            .is_empty());
        // Rides vary with distance.
        let rides = [-14.20, -22.80, -18.50, -16.90, -25.10, -19.70];
        assert!(run(&history("Rides", "Transport", rides, -61.30))
            .unusual
            .is_empty());

        // A bill with one month's small adjustment is still steady, and "usually" is true.
        let bill = [-45.12, -45.12, -45.30, -45.12, -47.80, -45.12];
        let found = run(&history("Phone", "Utilities", bill, -95.12)).unusual;
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].usual, found[0].amount), (-45.12, -95.12));
    }

    #[test]
    fn a_visit_where_you_choose_what_to_buy_is_never_unusual() {
        // The same coffee every time, then a round for friends.
        let coffee = [-5.25; 6];
        assert!(run(&history("Cafe", "Dining", coffee, -40.0))
            .unusual
            .is_empty());
        for category in ["Groceries", "Shopping"] {
            assert!(run(&history("Store", category, [-30.0; 6], -90.0))
                .unusual
                .is_empty());
        }
        // The same steady history elsewhere is flagged.
        assert_eq!(
            run(&history("Club", "Personal Care", coffee, -40.0))
                .unusual
                .len(),
            1
        );
    }
}
