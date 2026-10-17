//! The "All accounts" total. Money presents the spending cards on their own, so the Accounts
//! total and its history leave their balances out. Net worth is unchanged.
use std::collections::BTreeMap;

use crate::finance_contract::{Account, Point, Snapshot};
use crate::numeric::minor_units;
use serde::Serialize;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTotal {
    pub excluded_account_ids: Vec<String>,
    /// The excluded cards' balance. It is not market-driven, so live totals subtract it as is.
    pub excluded_value: f64,
    pub value: f64,
    /// Net worth history less the cards' balances on each date; empty when a card has no
    /// balance history to subtract.
    pub history: Vec<Point>,
}

/// The total without the spending cards, and net worth history less their balances.
pub fn accounts_total(snapshot: &Snapshot, cards: &[&Account]) -> Option<AccountsTotal> {
    let total = snapshot
        .accounts
        .iter()
        .find(|account| account.id == "all")?
        .value?;
    let excluded = cards
        .iter()
        .map(|card| card.value.map(minor_units))
        .sum::<Option<i64>>()?;
    let histories = cards
        .iter()
        .map(|card| {
            snapshot
                .account_balance_history
                .iter()
                .find(|history| history.account_id == card.id)
                .map(|history| {
                    history
                        .points
                        .iter()
                        .map(|point| (point.date.as_str(), point.value))
                        .collect::<BTreeMap<_, _>>()
                })
                .filter(|points| !points.is_empty())
        })
        .collect::<Option<Vec<_>>>();
    let history = match histories {
        Some(histories) if !cards.is_empty() => snapshot
            .net_worth_history
            .iter()
            .map(|point| {
                // Each card's last known balance on or before the date. Net worth counts an
                // account from its first point, so before a card's history there is nothing
                // to take out.
                let balance = histories
                    .iter()
                    .map(|history| {
                        history
                            .range(..=point.date.as_str())
                            .next_back()
                            .map_or(0, |(_, balance)| minor_units(*balance))
                    })
                    .sum::<i64>();
                Point {
                    value: (minor_units(point.value) - balance) as f64 / 100.0,
                    date: point.date.clone(),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(AccountsTotal {
        excluded_account_ids: cards.iter().map(|card| card.id.clone()).collect(),
        excluded_value: excluded as f64 / 100.0,
        value: (minor_units(total) - excluded) as f64 / 100.0,
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
        let total =
            accounts_total(&snapshot(&accounts, histories.clone()), &[&accounts[2]]).unwrap();
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
        let total = accounts_total(&snapshot(&accounts, without_history), &[&accounts[2]]).unwrap();
        assert!(
            total.history.is_empty(),
            "history is never estimated from today's balance"
        );
    }

    #[test]
    fn leaves_every_spending_card_out() {
        let accounts = [
            account("all", "combined", 700.0),
            account("plaid:checking", "cash", 1_200.0),
            account("plaid:card", "credit", -300.0),
            account("plaid:other", "credit", -200.0),
        ];
        let histories = json!({
            "netWorthHistory": [{ "date": "2026-09-03", "value": 700.0 }],
            "accountBalanceHistory": [
                { "accountId": "plaid:card", "name": "Card", "institution": "Bank",
                  "currentValue": -300.0, "points": [{ "date": "2026-09-03", "value": -300.0 }] },
                { "accountId": "plaid:other", "name": "Other", "institution": "Bank",
                  "currentValue": -200.0, "points": [{ "date": "2026-09-03", "value": -200.0 }] },
            ],
        });
        let total = accounts_total(
            &snapshot(&accounts, histories),
            &[&accounts[2], &accounts[3]],
        )
        .unwrap();
        assert_eq!(total.value, 1_200.0);
        assert_eq!(total.excluded_value, -500.0);
        assert_eq!(total.history[0].value, 1_200.0);
        assert_eq!(total.excluded_account_ids, ["plaid:card", "plaid:other"]);
    }
}
