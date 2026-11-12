//! The sums under Activity's ledger while it is narrowed: for the rows in view (the renderer
//! filters and selects them, then sends their IDs), how many, and what came in, went out, and
//! net, per currency. Amounts are summed in cents from the view as read, signed as the ledger
//! shows them: a positive amount came in, a negative one went out.
//!
//! The ledger lists two kinds of row, and both are summed: a transaction (`spending:<id>`) and a
//! trade (`trade:<id>`), each in its account's currency. Any other ID (an expected item or a
//! statement row the renderer might list one day, or a row gone since the view was read) is
//! counted as unmatched and summed nowhere, never guessed at.
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use ts_rs::TS;

use super::View;
use crate::numeric::{from_minor, minor_units};

/// The most IDs one request may name, and the longest ID.
pub const MAX_IDS: usize = 100_000;
pub const MAX_ID_LENGTH: usize = 300;

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActivityTotals {
    /// Rows found, with or without an amount.
    pub count: usize,
    /// Dollars first, then the other currencies by code.
    pub currencies: Vec<CurrencyTotals>,
    /// Pending rows in the sums.
    pub pending: usize,
    /// Rows left out of the sums because their amount is unknown.
    pub without_amount: usize,
    /// IDs that name no row of the view, left out of everything.
    pub unmatched: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyTotals {
    pub currency: String,
    /// Rows in this currency with an amount.
    pub count: usize,
    pub money_in: f64,
    /// Zero or negative: what went out, signed as the ledger shows it.
    pub money_out: f64,
    pub net: f64,
}

#[derive(Default)]
struct Sum {
    count: usize,
    money_in: i64,
    money_out: i64,
}

/// The totals of the rows `ids` names, each ID counted once.
pub fn activity_totals(view: &View, ids: &[String]) -> ActivityTotals {
    let snapshot = &view.snapshot;
    // An account that names no currency, or a row without an account, is in dollars: the
    // view keeps only US-dollar activity otherwise.
    let currency = |account_id: Option<&str>| {
        snapshot
            .accounts
            .iter()
            .find(|account| Some(account.id.as_str()) == account_id)
            .and_then(|account| account.currency.clone())
            .unwrap_or_else(|| "USD".into())
    };
    let transactions: BTreeMap<&str, _> = snapshot
        .transactions
        .iter()
        .map(|transaction| (transaction.id.as_str(), transaction))
        .collect();
    let trades: BTreeMap<&str, _> = snapshot
        .trades
        .iter()
        .map(|trade| (trade.id.as_str(), trade))
        .collect();
    let mut totals = ActivityTotals::default();
    let mut sums = BTreeMap::<String, Sum>::new();
    let unique: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    for id in unique {
        let row = if let Some(id) = id.strip_prefix("spending:") {
            transactions.get(id).map(|transaction| {
                (
                    transaction.amount,
                    transaction.pending,
                    currency(transaction.account_id.as_deref()),
                )
            })
        } else if let Some(id) = id.strip_prefix("trade:") {
            trades
                .get(id)
                .map(|trade| (trade.amount, false, currency(Some(&trade.account_id))))
        } else {
            None
        };
        let Some((amount, pending, currency)) = row else {
            totals.unmatched += 1;
            continue;
        };
        totals.count += 1;
        if !amount.is_finite() {
            totals.without_amount += 1;
            continue;
        }
        totals.pending += usize::from(pending);
        let sum = sums.entry(currency).or_default();
        let cents = minor_units(amount);
        sum.count += 1;
        if cents > 0 {
            sum.money_in += cents;
        } else {
            sum.money_out += cents;
        }
    }
    totals.currencies = sums
        .into_iter()
        .map(|(currency, sum)| CurrencyTotals {
            currency,
            count: sum.count,
            money_in: from_minor(sum.money_in),
            money_out: from_minor(sum.money_out),
            net: from_minor(sum.money_in + sum.money_out),
        })
        .collect();
    // Dollars lead; the map already ordered the rest by code.
    totals.currencies.sort_by_key(|set| set.currency != "USD");
    totals
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The synthetic seed's snapshot, with synthetic rows of its own in a card and a CAD account.
    fn view() -> View {
        let mut snapshot: crate::finance_contract::Snapshot =
            serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap();
        snapshot.accounts = serde_json::from_value(json!([
            { "id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": null },
            { "id": "cad", "name": "Loonie", "institution": "Bank", "type": "cash", "value": null,
              "currency": "CAD" },
        ]))
        .unwrap();
        let row = |id: &str, amount: f64, account: &str, pending: bool| {
            crate::finance_contract::Transaction {
                id: id.into(),
                amount,
                account_id: Some(account.into()),
                pending,
                ..Default::default()
            }
        };
        snapshot.transactions = vec![
            row("pay", 1200.0, "card", false),
            row("coffee", -45.1, "card", true),
            row("dime", -0.2, "card", false),
            row("loonie", -10.0, "cad", false),
        ];
        snapshot.trades = vec![crate::finance_contract::Trade {
            id: "buy".into(),
            r#type: "BUY".into(),
            amount: -100.0,
            account_id: "card".into(),
            ..Default::default()
        }];
        View {
            snapshot,
            insights: None,
            account_names: BTreeMap::new(),
            spending_account_id: None,
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn mixed_signs_sum_in_cents_with_pending_counted_and_trades_included() {
        let totals = activity_totals(
            &view(),
            &ids(&[
                "spending:pay",
                "spending:coffee",
                "spending:dime",
                "trade:buy",
            ]),
        );
        assert_eq!((totals.count, totals.pending), (4, 1));
        assert_eq!(
            totals.currencies,
            [CurrencyTotals {
                currency: "USD".into(),
                count: 4,
                money_in: 1200.0,
                money_out: -145.3,
                net: 1054.7,
            }]
        );
    }

    #[test]
    fn currencies_stay_apart_dollars_first() {
        let totals = activity_totals(&view(), &ids(&["spending:loonie", "spending:dime"]));
        assert_eq!(
            totals
                .currencies
                .iter()
                .map(|set| (set.currency.as_str(), set.net))
                .collect::<Vec<_>>(),
            [("USD", -0.2), ("CAD", -10.0)]
        );
    }

    #[test]
    fn a_missing_amount_is_counted_apart_and_an_unknown_id_is_ignored() {
        let mut view = view();
        view.snapshot.transactions[0].amount = f64::NAN;
        let totals = activity_totals(
            &view,
            &ids(&[
                "spending:pay",
                "spending:dime",
                "spending:gone",
                "expected:bill",
                "spending:dime",
            ]),
        );
        // The repeated ID counts once; the unknown ones count nowhere.
        assert_eq!(
            (totals.count, totals.without_amount, totals.unmatched),
            (2, 1, 2)
        );
        assert_eq!(totals.currencies[0].count, 1);
        assert_eq!(totals.currencies[0].net, -0.2);
        assert_eq!(totals.currencies[0].money_in, 0.0);
    }

    #[test]
    fn no_rows_have_no_sums() {
        assert_eq!(activity_totals(&view(), &[]), ActivityTotals::default());
    }
}
