//! Active subscriptions: recurring series classified as subscriptions whose next charge falls
//! within a year, so annual plans count alongside monthly ones. A series with a missed charge
//! drops out, as it does from the forecast. Monthly cost is normalized from the cadence and
//! summed in whole cents. A subscription whose latest charge differs from the price it held
//! before says so: these detections are retrospective, so they report a change already paid.
use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use super::calendar::{self, Zone};
use super::forecast::{recurring_key, EventKind, EventState, ExpectedEvent};
use crate::finance_contract::Transaction;
use crate::numeric::minor_units;

/// Long enough for a yearly charge to be next.
pub const HORIZON_DAYS: i64 = 366;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub recurring_id: String,
    pub title: String,
    /// Typical charge, negative, or the pending charge when one has matched.
    pub amount: f64,
    pub monthly_amount: f64,
    pub frequency: &'static str,
    pub next_date: NaiveDate,
    pub pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub source_transaction_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_change: Option<PriceChange>,
}

/// The latest posted charge against the price the subscription held before it.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceChange {
    /// Both negative, like the charges.
    pub previous: f64,
    pub latest: f64,
    pub changed_on: NaiveDate,
    pub transaction_id: String,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subscriptions {
    /// Sum of monthly costs, negative.
    pub monthly: f64,
    /// Largest monthly cost first.
    pub items: Vec<Subscription>,
}

/// A change below both 50 cents and 1% is rounding, tax, or currency noise, not a new price.
fn changed(previous: i64, latest: i64) -> bool {
    let difference = (latest - previous).abs();
    difference >= 50 && difference * 100 >= previous.abs()
}

/// Price changes per series, from posted charges: the latest differs from the one before,
/// which itself matched the charge before it when there was one, so a series whose amount
/// always varies never reads as repriced.
fn price_changes(
    transactions: &[Transaction],
    reference: &str,
    zone: &dyn Zone,
) -> BTreeMap<String, PriceChange> {
    let mut series = BTreeMap::<String, Vec<(NaiveDate, &Transaction)>>::new();
    for transaction in transactions
        .iter()
        .filter(|transaction| !transaction.pending && transaction.amount < 0.0)
    {
        if let Some(date) = calendar::transaction_date(transaction.posted_date(), reference, zone) {
            series
                .entry(recurring_key(transaction, zone))
                .or_default()
                .push((date, transaction));
        }
    }
    series
        .into_iter()
        .filter_map(|(key, mut charges)| {
            charges.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));
            let amount = |index: usize| minor_units(charges[index].1.amount);
            let count = charges.len();
            if count < 2 {
                return None;
            }
            let (previous, latest) = (amount(count - 2), amount(count - 1));
            let steady = count < 3 || amount(count - 3) == previous;
            let (changed_on, transaction) = charges[count - 1];
            (steady && changed(previous, latest)).then(|| {
                (
                    key,
                    PriceChange {
                        previous: previous as f64 / 100.0,
                        latest: latest as f64 / 100.0,
                        changed_on,
                        transaction_id: transaction.id.clone(),
                    },
                )
            })
        })
        .collect()
}

pub fn subscriptions(
    events: &[ExpectedEvent],
    transactions: &[Transaction],
    reference: &str,
    zone: &dyn Zone,
) -> Subscriptions {
    let changes = price_changes(transactions, reference, zone);
    let mut items = events
        .iter()
        .filter(|event| event.kind == EventKind::Subscription && event.amount < 0.0)
        .map(|event| Subscription {
            recurring_id: event.recurring_id.clone(),
            title: event.title.clone(),
            amount: event.amount,
            monthly_amount: minor_units(event.amount * f64::from(event.per_year) / 12.0) as f64
                / 100.0,
            frequency: event.frequency,
            next_date: event.date,
            pending: event.state == EventState::Pending,
            account_id: event.account_id.clone(),
            source_transaction_id: event.source_transaction_id.clone(),
            price_change: changes.get(&event.recurring_id).cloned(),
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.monthly_amount
            .total_cmp(&right.monthly_amount)
            .then_with(|| left.title.cmp(&right.title))
    });
    Subscriptions {
        monthly: items
            .iter()
            .map(|item| minor_units(item.monthly_amount))
            .sum::<i64>() as f64
            / 100.0,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn event(id: &str, kind: EventKind, amount: f64, per_year: u32) -> ExpectedEvent {
        ExpectedEvent {
            id: id.into(),
            recurring_id: id.into(),
            title: id.into(),
            kind,
            date: day("2026-10-01"),
            amount,
            amount_low: amount.abs(),
            amount_high: amount.abs(),
            frequency: "Monthly",
            observations: 3,
            last_seen: day("2026-09-01"),
            state: EventState::Expected,
            account_id: Some("plaid:card".into()),
            matched_transaction_id: None,
            source_transaction_id: id.into(),
            later: Vec::new(),
            per_year,
        }
    }

    #[test]
    fn normalizes_subscriptions_to_monthly_cost_and_skips_other_series() {
        let result = subscriptions(
            &[
                event("music", EventKind::Subscription, -10.99, 12),
                event("storage", EventKind::Subscription, -120.0, 1),
                event("weekly", EventKind::Subscription, -3.0, 52),
                event("rent", EventKind::Payment, -1_500.0, 12),
                event("pay", EventKind::Income, 2_000.0, 24),
            ],
            &[],
            "2026-09-16T12:00:00Z",
            &calendar::NamedZone::Utc,
        );
        let monthly = result
            .items
            .iter()
            .map(|item| (item.title.as_str(), item.monthly_amount))
            .collect::<Vec<_>>();
        assert_eq!(
            monthly,
            [("weekly", -13.0), ("music", -10.99), ("storage", -10.0)]
        );
        assert_eq!(result.monthly, -33.99);
    }

    fn charge(id: &str, date: &str, amount: f64) -> Transaction {
        serde_json::from_value(serde_json::json!({
            "id": id, "merchant": "Stream", "category": "Entertainment", "date": date,
            "amount": amount, "account": "Card", "accountId": "plaid:card", "pending": false,
            "categorySource": "provider", "description": null,
            "classification": { "mark": "initial", "kind": "expense", "spending": true, "credit": false, "zelle": false }
        }))
        .unwrap()
    }

    #[test]
    fn reports_a_new_price_only_after_a_steady_one() {
        let zone = calendar::NamedZone::Utc;
        let reference = "2026-09-16T12:00:00Z";
        let raised = [
            charge("june", "2026-06-05", -11.99),
            charge("july", "2026-07-05", -11.99),
            charge("august", "2026-08-05", -13.99),
        ];
        let changes = price_changes(&raised, reference, &zone);
        let change = changes.values().next().unwrap();
        assert_eq!((change.previous, change.latest), (-11.99, -13.99));
        assert_eq!(change.changed_on, day("2026-08-05"));
        assert_eq!(change.transaction_id, "august");

        // A bill that always varies, and a few cents of tax drift, are not new prices.
        let variable = [
            charge("june", "2026-06-05", -80.10),
            charge("july", "2026-07-05", -95.40),
            charge("august", "2026-08-05", -70.25),
        ];
        assert!(price_changes(&variable, reference, &zone).is_empty());
        let drift = [
            charge("july", "2026-07-05", -11.99),
            charge("august", "2026-08-05", -12.04),
        ];
        assert!(price_changes(&drift, reference, &zone).is_empty());
    }
}
