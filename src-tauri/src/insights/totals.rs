//! The "All accounts" total and each account group's line. Money presents the spending cards
//! on their own, so the Accounts total and its history leave their balances out, and the cards
//! form their own group. Net worth is unchanged.
use std::collections::{BTreeMap, BTreeSet};

use super::rollup::{self, Rollup};
use crate::finance_contract::{Account, Performance, PerformancePoint, Point, Snapshot};
use crate::numeric::{from_minor, minor_units, round2};
use crate::settings::FinanceSettings;
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

/// The group the spending cards form, apart from the account types.
pub const SPENDING: &str = "spending";

/// One account group's line on Accounts: a type of the counted accounts, or the spending cards
/// set apart (`spending`), which no other group includes.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GroupHistory {
    #[serde(rename = "type")]
    pub account_type: String,
    /// The accounts it sums.
    pub account_ids: Vec<String>,
    /// Its value now, as the rollup totals it; none while an account in it has no value.
    pub value: Option<f64>,
    /// Each date's total of the histories its accounts' charts draw, each account counting
    /// from its first point and carrying its last value across dates it skips. Empty while an
    /// account has no value or no history beyond today's value, as net worth history is.
    pub points: Vec<Point>,
}

/// The id of an account group's line in chart changes and the market stream.
pub fn group_series_id(account_type: &str) -> String {
    format!("group:{account_type}")
}

/// Each group `rollup` lists, then the spending cards, with its daily history.
pub fn group_histories(
    snapshot: &Snapshot,
    settings: &FinanceSettings,
    rollup: &Rollup,
) -> Vec<GroupHistory> {
    let card_ids = rollup::spending_cards(&snapshot.accounts, settings)
        .into_iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    rollup
        .listed
        .iter()
        .chain(&rollup.cards)
        .map(|group| {
            let members = snapshot
                .accounts
                .iter()
                .filter(|account| account.id != "all")
                .filter(|account| {
                    let card = card_ids.contains(&account.id.as_str());
                    if group.account_type == SPENDING {
                        card
                    } else {
                        !card && account.r#type == group.account_type
                    }
                })
                // As the rollup sums them.
                .filter(|account| account.counted())
                .collect::<Vec<_>>();
            GroupHistory {
                account_type: group.account_type.clone(),
                account_ids: members.iter().map(|account| account.id.clone()).collect(),
                value: group.value,
                points: if group.value.is_some() {
                    group_points(snapshot, &members)
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

/// The accounts' chart histories summed by date: an investment account's performance, else
/// its balance history. A history of today's value alone says nothing about the past, so
/// then there is none.
fn group_points(snapshot: &Snapshot, accounts: &[&Account]) -> Vec<Point> {
    let histories = accounts
        .iter()
        .map(|account| {
            let of_account = |history: &&Performance| history.account_id == account.id;
            let history = matches!(account.r#type.as_str(), "brokerage" | "retirement")
                .then(|| snapshot.brokerage_performance.iter().find(of_account))
                .flatten()
                .or_else(|| snapshot.account_balance_history.iter().find(of_account))
                .filter(|history| history.history_source.as_deref() != Some("unavailable"))?;
            let points = history
                .points
                .iter()
                .filter(|point| point.value.is_finite() && point.date.len() == 10)
                .map(|point| (point.date.as_str(), minor_units(point.value)))
                .collect::<BTreeMap<_, _>>();
            (points.len() > 1).then_some(points)
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    let dates = histories
        .iter()
        .flat_map(|history| history.keys().copied())
        .collect::<BTreeSet<_>>();
    dates
        .into_iter()
        .map(|date| Point {
            date: date.to_owned(),
            value: from_minor(
                histories
                    .iter()
                    .filter_map(|history| history.range(..=date).next_back())
                    .map(|(_, value)| *value)
                    .sum(),
            ),
        })
        .collect()
}

/// One day of the total Home draws, split into cash, securities, and the rest, with how far it
/// stood below its best so far.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetWorthPart {
    pub date: String,
    /// Bank balances, and the cash in brokerages whose history records it.
    pub cash: f64,
    /// Securities in those brokerages.
    pub invested: f64,
    /// The rest of the total: accounts whose history gives only a total (a manual account, a
    /// brokerage known by the provider's totals), debts, and money between accounts.
    pub other: f64,
    /// The highest the total had reached by this day.
    pub high_water: f64,
    /// How far below that it stood, in percent (zero at a new high); none while the high isn't
    /// above zero.
    pub drawdown_pct: Option<f64>,
}

/// Each day of `history`, the total Home draws, split as `NetWorthPart` says. Cash accounts and
/// investment accounts count from their first point and carry their last across days they
/// skip, as the total counts them, and the rest is what they leave of the total. Drawdown is
/// the plain fall from the total's high: spending and withdrawals lower net worth, so they
/// count. Empty without a history.
pub fn net_worth_parts(snapshot: &Snapshot, history: &[Point]) -> Vec<NetWorthPart> {
    type Days<'a> = BTreeMap<&'a str, i64>;
    let mut cash: Vec<Days> = Vec::new();
    let mut invested: Vec<Days> = Vec::new();
    for account in snapshot
        .accounts
        .iter()
        .filter(|account| account.id != "all" && account.counted())
    {
        let of_account = |history: &&Performance| history.account_id == account.id;
        match account.r#type.as_str() {
            "cash" => {
                if let Some(history) = snapshot.account_balance_history.iter().find(of_account) {
                    cash.push(days(history, |point| Some(point.value)));
                }
            }
            "brokerage" | "retirement" => {
                if let Some(history) = snapshot.brokerage_performance.iter().find(of_account) {
                    cash.push(days(history, |point| point.cash));
                    invested.push(days(history, |point| point.invested));
                }
            }
            _ => {}
        }
    }
    let on = |histories: &[Days], date: &str| -> i64 {
        histories
            .iter()
            .filter_map(|history| history.range(..=date).next_back())
            .map(|(_, value)| *value)
            .sum()
    };
    let mut high = i64::MIN;
    history
        .iter()
        .filter(|point| point.value.is_finite())
        .map(|point| {
            let total = minor_units(point.value);
            let (held, securities) = (on(&cash, &point.date), on(&invested, &point.date));
            high = high.max(total);
            NetWorthPart {
                date: point.date.clone(),
                cash: from_minor(held),
                invested: from_minor(securities),
                other: from_minor(total - held - securities),
                high_water: from_minor(high),
                drawdown_pct: (high > 0)
                    .then(|| round2((total - high) as f64 / high as f64 * 100.0)),
            }
        })
        .collect()
}

/// A history's figure by date, in cents, for the days it has one.
fn days<'a>(
    history: &'a Performance,
    figure: impl Fn(&PerformancePoint) -> Option<f64>,
) -> BTreeMap<&'a str, i64> {
    history
        .points
        .iter()
        .filter(|point| point.date.len() == 10)
        .filter_map(|point| {
            let value = figure(point).filter(|value| value.is_finite())?;
            Some((point.date.as_str(), minor_units(value)))
        })
        .collect()
}

/// The total without the spending cards, and net worth history less their balances.
pub fn accounts_total(snapshot: &Snapshot, cards: &[&Account]) -> Option<AccountsTotal> {
    // A spending-only card has no balance to take out.
    let cards = cards
        .iter()
        .copied()
        .filter(|card| card.counted())
        .collect::<Vec<_>>();
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
    fn net_worth_splits_into_cash_securities_and_the_rest_with_its_drawdown() {
        let accounts = [
            account("checking", "cash", 300.0),
            account("brokerage", "brokerage", 1_000.0),
            account("plan", "retirement", 500.0),
        ];
        let performance = |id: &str, points: serde_json::Value| {
            json!({ "accountId": id, "name": id, "institution": "Bank", "currentValue": 0,
                "points": points })
        };
        let snapshot = snapshot(
            &accounts,
            json!({
                "accountBalanceHistory": [performance("checking", json!([
                    { "date": "2026-09-01", "value": 400, "netDeposits": null, "sp500": null },
                    { "date": "2026-09-03", "value": 300, "netDeposits": null, "sp500": null },
                ]))],
                // The brokerage knows its cash; the plan has only totals, so it is the rest.
                "brokeragePerformance": [
                    performance("brokerage", json!([
                        { "date": "2026-09-01", "value": 1_100, "netDeposits": 1_000,
                          "sp500": null, "cash": 100, "invested": 1_000 },
                        { "date": "2026-09-02", "value": 900, "netDeposits": 1_000,
                          "sp500": null, "cash": 100, "invested": 800 },
                        { "date": "2026-09-03", "value": 1_000, "netDeposits": 1_000,
                          "sp500": null, "cash": 100, "invested": 900 },
                    ])),
                    performance("plan", json!([
                        { "date": "2026-09-01", "value": 500, "netDeposits": null, "sp500": null },
                    ])),
                ],
            }),
        );
        let history = [
            ("2026-09-01", 2_000.0),
            ("2026-09-02", 1_800.0),
            ("2026-09-03", 1_800.0),
        ]
        .map(|(date, value)| Point {
            date: date.into(),
            value,
        });
        let parts = net_worth_parts(&snapshot, &history)
            .into_iter()
            .map(|part| {
                (
                    part.cash,
                    part.invested,
                    part.other,
                    part.high_water,
                    part.drawdown_pct,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parts,
            [
                (500.0, 1_000.0, 500.0, 2_000.0, Some(0.0)),
                // The checking balance carries across the day its history skips.
                (500.0, 800.0, 500.0, 2_000.0, Some(-10.0)),
                (400.0, 900.0, 500.0, 2_000.0, Some(-10.0)),
            ]
        );
        assert!(net_worth_parts(&snapshot, &[]).is_empty());
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

    fn history(id: &str, points: &[(&str, f64)]) -> serde_json::Value {
        json!({
            "accountId": id, "name": id, "institution": "Bank", "currentValue": 0,
            "points": points.iter().map(|(date, value)| json!({
                "date": date, "value": value, "netDeposits": null, "sp500": null,
            })).collect::<Vec<_>>(),
        })
    }

    fn lines(groups: &[GroupHistory]) -> Vec<(&str, Option<f64>, Vec<(&str, f64)>)> {
        groups
            .iter()
            .map(|group| {
                (
                    group.account_type.as_str(),
                    group.value,
                    group
                        .points
                        .iter()
                        .map(|point| (point.date.as_str(), point.value))
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn each_group_sums_its_accounts_chart_histories_with_the_spending_cards_apart() {
        let accounts = [
            account("all", "combined", 2_400.0),
            account("plaid:checking", "cash", 1_200.0),
            account("plaid:savings", "cash", 300.0),
            account("plaid:card", "credit", -300.0),
            account("plaid:other", "credit", -50.0),
            account("snaptrade:broker", "brokerage", 1_250.0),
        ];
        let histories = json!({
            "accountBalanceHistory": [
                history("plaid:checking", &[("2026-09-01", 1_000.0), ("2026-09-03", 1_200.0)]),
                // Savings starts later and skips a day: it counts from its first point and
                // carries its last value.
                history("plaid:savings", &[("2026-09-02", 250.0), ("2026-09-04", 300.0)]),
                history("plaid:card", &[("2026-09-01", -100.0), ("2026-09-04", -300.0)]),
                history("plaid:other", &[("2026-09-01", -10.0), ("2026-09-04", -50.0)]),
                // An investment account's chart draws its performance, not this.
                history("snaptrade:broker", &[("2026-09-01", 1.0), ("2026-09-04", 1.0)]),
            ],
            "brokeragePerformance": [
                history("snaptrade:broker", &[("2026-09-01", 1_000.0), ("2026-09-04", 1_250.0)]),
            ],
        });
        let snapshot = snapshot(&accounts, histories);
        let settings = FinanceSettings {
            spending_account_id: Some("plaid:card".into()),
            ..FinanceSettings::default()
        };
        let rollup = rollup::rollup(&accounts, &[], 2_400.0, &settings);
        let groups = group_histories(&snapshot, &settings, &rollup);
        assert_eq!(
            lines(&groups),
            [
                (
                    "cash",
                    Some(1_500.0),
                    vec![
                        ("2026-09-01", 1_000.0),
                        ("2026-09-02", 1_250.0),
                        ("2026-09-03", 1_450.0),
                        ("2026-09-04", 1_500.0),
                    ]
                ),
                (
                    "brokerage",
                    Some(1_250.0),
                    vec![("2026-09-01", 1_000.0), ("2026-09-04", 1_250.0)]
                ),
                (
                    "credit",
                    Some(-50.0),
                    vec![("2026-09-01", -10.0), ("2026-09-04", -50.0)]
                ),
                (
                    "spending",
                    Some(-300.0),
                    vec![("2026-09-01", -100.0), ("2026-09-04", -300.0)]
                ),
            ]
        );
        assert_eq!(groups[3].account_ids, ["plaid:card"]);
        assert_eq!(groups[2].account_ids, ["plaid:other"]);
    }

    #[test]
    fn a_group_has_no_history_while_an_account_in_it_cannot_complete_one() {
        let mut unknown = account("plaid:unknown", "brokerage", 0.0);
        unknown.value = None;
        let accounts = [
            account("all", "combined", 1_500.0),
            account("plaid:checking", "cash", 1_200.0),
            // Today's value alone says nothing about the past.
            account("plaid:new", "cash", 300.0),
            account("snaptrade:broker", "brokerage", 1_000.0),
            unknown,
        ];
        let histories = json!({
            "accountBalanceHistory": [
                history("plaid:checking", &[("2026-09-01", 1_000.0), ("2026-09-03", 1_200.0)]),
                history("plaid:new", &[("2026-09-03", 300.0)]),
            ],
            "brokeragePerformance": [
                history("snaptrade:broker", &[("2026-09-01", 900.0), ("2026-09-03", 1_000.0)]),
            ],
        });
        let settings = FinanceSettings::default();
        let rollup = rollup::rollup(&accounts, &[], 1_500.0, &settings);
        let groups = group_histories(&snapshot(&accounts, histories), &settings, &rollup);
        let group = |kind: &str| {
            groups
                .iter()
                .find(|group| group.account_type == kind)
                .unwrap()
        };
        assert_eq!(group("cash").value, Some(1_500.0));
        assert!(group("cash").points.is_empty());
        assert_eq!(group("brokerage").value, None);
        assert!(group("brokerage").points.is_empty());
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
