//! The "All accounts" total. Spending presents the saved spending card on its own, so the
//! Accounts total and its history leave that card's balance out. Net worth is unchanged.
use std::collections::BTreeMap;

use crate::finance_contract::{Account, Point, Snapshot};
use crate::numeric::minor_units;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTotal {
    pub excluded_account_id: String,
    /// The excluded card balance. It is not market-driven, so live totals subtract it as is.
    pub excluded_value: f64,
    pub value: f64,
    /// Net worth history less the card's balance on each date; empty when the card has no
    /// balance history to subtract.
    pub history: Vec<Point>,
}

/// The total without the spending card, and net worth history less the card's balance.
pub fn accounts_total(snapshot: &Snapshot, card: &Account) -> Option<AccountsTotal> {
    let total = snapshot
        .accounts
        .iter()
        .find(|account| account.id == "all")?
        .value?;
    let card_value = card.value?;
    let card_history: BTreeMap<&str, f64> = snapshot
        .account_balance_history
        .iter()
        .find(|history| history.account_id == card.id)
        .map(|history| {
            history
                .points
                .iter()
                .map(|point| (point.date.as_str(), point.value))
                .collect()
        })
        .unwrap_or_default();
    let history = if card_history.is_empty() {
        Vec::new()
    } else {
        snapshot
            .net_worth_history
            .iter()
            .map(|point| {
                // The card's last known balance on or before the date. Net worth counts an
                // account from its first point, so before the card's history there is nothing
                // to take out.
                let balance = card_history
                    .range(..=point.date.as_str())
                    .next_back()
                    .map_or(0.0, |(_, balance)| *balance);
                Point {
                    value: (minor_units(point.value) - minor_units(balance)) as f64 / 100.0,
                    date: point.date.clone(),
                }
            })
            .collect()
    };
    Some(AccountsTotal {
        excluded_account_id: card.id.clone(),
        excluded_value: card_value,
        value: (minor_units(total) - minor_units(card_value)) as f64 / 100.0,
        history,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A snapshot with these accounts and histories, and nothing else.
    fn snapshot(accounts: &[Account], histories: serde_json::Value) -> Snapshot {
        let mut value = json!({
            "updatedAt": "2026-09-03T12:00:00Z", "netWorth": 0, "accounts": accounts,
            "holdings": [], "transactions": [], "netWorthHistory": [],
            "spending": { "monthTotal": 0, "categories": [] },
        });
        for (key, history) in histories.as_object().unwrap() {
            value[key] = history.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    fn account(id: &str, kind: &str, value: f64) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": kind, "value": value
        }))
        .unwrap()
    }

    #[test]
    fn leaves_the_spending_card_out_of_the_total_and_its_history() {
        let accounts = [
            account("all", "combined", 900.0),
            account("plaid:checking", "cash", 1_200.0),
            account("plaid:card", "credit", -300.0),
        ];
        let histories = json!({
            "netWorthHistory": [
                { "date": "2026-09-01", "value": 800.0 },
                { "date": "2026-09-02", "value": 850.0 },
                { "date": "2026-09-03", "value": 900.0 },
            ],
            "accountBalanceHistory": [{
                "accountId": "plaid:card", "name": "Card", "institution": "Bank", "currentValue": -300.0,
                "points": [
                    { "date": "2026-09-02", "value": -100.0 },
                    { "date": "2026-09-03", "value": -300.0 },
                ],
            }],
        });
        let total = accounts_total(&snapshot(&accounts, histories.clone()), &accounts[2]).unwrap();
        assert_eq!(total.value, 1_200.0);
        assert_eq!(total.excluded_value, -300.0);
        assert_eq!(
            total
                .history
                .iter()
                .map(|point| (point.date.as_str(), point.value))
                .collect::<Vec<_>>(),
            [
                ("2026-09-01", 800.0),
                ("2026-09-02", 950.0),
                ("2026-09-03", 1_200.0)
            ]
        );

        let without_history = json!({ "netWorthHistory": histories["netWorthHistory"] });
        let total = accounts_total(&snapshot(&accounts, without_history), &accounts[2]).unwrap();
        assert!(
            total.history.is_empty(),
            "history is never estimated from today's balance"
        );
    }
}
