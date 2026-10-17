//! How each account's balance moved over the chart periods (a day, a week, a month, a quarter,
//! a year, and all saved history), and how much activity posted in each. Balances come from the
//! committed daily histories; an account whose history does not reach back to a period's start
//! has no change for it, and the period says so.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::calendar;
use crate::finance_contract::Transaction;
use crate::finance_contract::{Account, Performance};
use crate::numeric::{from_minor, minor_units};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Window {
    Day,
    Week,
    Month,
    Quarter,
    Year,
    All,
}

/// Each chart period and its length in days; `All` is every saved day.
pub const WINDOWS: [(Window, Option<i64>); 6] = [
    (Window::Day, Some(1)),
    (Window::Week, Some(7)),
    (Window::Month, Some(30)),
    (Window::Quarter, Some(90)),
    (Window::Year, Some(365)),
    (Window::All, None),
];

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountChange {
    pub account_id: String,
    pub change: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodChange {
    pub window: Window,
    /// The first day counted; for `all`, the first day of saved activity or history.
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Accounts whose balance changed, largest change first.
    pub accounts: Vec<AccountChange>,
    /// An account's history starts after `start`, so its change is not listed.
    pub incomplete: bool,
    /// Posted transactions dated in the period.
    pub transactions: usize,
    /// Each account type's change across every account but the combined one, and across the
    /// accounts the totals count, with the spending cards' apart. A type with an account whose
    /// history doesn't reach the period has none; an unchanged account adds nothing.
    pub groups: Vec<TypeChange>,
    pub listed: Vec<TypeChange>,
    pub cards: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeChange {
    #[serde(rename = "type")]
    pub account_type: String,
    pub change: Option<f64>,
}

/// Each type's change over `accounts`, from the changes known in whole cents.
fn type_changes<'a>(
    accounts: impl IntoIterator<Item = &'a Account>,
    known: &BTreeMap<&str, i64>,
) -> Vec<TypeChange> {
    let mut by_type: Vec<(String, Option<i64>)> = Vec::new();
    for account in accounts {
        let change = known.get(account.id.as_str()).copied();
        match by_type.iter_mut().find(|(kind, _)| *kind == account.r#type) {
            Some((_, total)) => *total = total.zip(change).map(|(total, change)| total + change),
            None => by_type.push((account.r#type.clone(), change)),
        }
    }
    by_type
        .into_iter()
        .map(|(account_type, change)| TypeChange {
            account_type,
            change: change.map(from_minor),
        })
        .collect()
}

/// Daily balances per account from the balance and brokerage histories; the balance history
/// wins when both describe an account. Aggregate series are left out.
fn histories(
    balances: &[Performance],
    brokerages: &[Performance],
) -> BTreeMap<String, Vec<(NaiveDate, i64)>> {
    let mut by_account = BTreeMap::new();
    for history in brokerages.iter().chain(balances) {
        if matches!(history.account_id.as_str(), "all" | "total" | "net-worth") {
            continue;
        }
        let mut points = history
            .points
            .iter()
            .filter_map(|point| {
                Some((
                    NaiveDate::parse_from_str(point.date.get(..10)?, "%Y-%m-%d").ok()?,
                    minor_units(point.value),
                ))
            })
            .collect::<Vec<_>>();
        points.sort_by_key(|(date, _)| *date);
        if points.len() >= 2 {
            by_account.insert(history.account_id.clone(), points);
        }
    }
    by_account
}

/// Each period's changes. `accounts` give the types to total by, and `cards` the spending cards
/// the totals leave out.
pub fn changes(
    balances: &[Performance],
    brokerages: &[Performance],
    transactions: &[Transaction],
    accounts: &[Account],
    cards: &[&str],
    today: NaiveDate,
) -> Vec<PeriodChange> {
    let histories = histories(balances, brokerages);
    let posted = transactions
        .iter()
        .filter(|transaction| !transaction.pending)
        .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
        .filter(|date| *date <= today)
        .collect::<Vec<_>>();
    let earliest = histories
        .values()
        .filter_map(|points| points.first().map(|(date, _)| *date))
        .chain(posted.iter().copied())
        .min();
    WINDOWS
        .iter()
        .filter_map(|(window, days)| {
            let start = match days {
                Some(days) => today - Duration::days(days - 1),
                None => earliest?,
            };
            let mut incomplete = false;
            // Each account's change in cents, unchanged ones included, when its history covers
            // the period.
            let known = histories
                .iter()
                .filter_map(|(account_id, points)| {
                    // The balance at the close of the day before the period, and the latest.
                    let opening = points
                        .iter()
                        .rev()
                        .find(|(date, _)| *date < start)
                        .or_else(|| days.is_none().then(|| points.first()).flatten());
                    let Some((_, opening)) = opening else {
                        incomplete = true;
                        return None;
                    };
                    let (_, latest) = points.last()?;
                    Some((account_id.as_str(), latest - opening))
                })
                .collect::<BTreeMap<_, _>>();
            let mut changed = known
                .iter()
                .filter(|(_, change)| **change != 0)
                .map(|(account_id, change)| AccountChange {
                    account_id: (*account_id).to_owned(),
                    change: from_minor(*change),
                })
                .collect::<Vec<_>>();
            changed.sort_by(|left, right| {
                right
                    .change
                    .abs()
                    .total_cmp(&left.change.abs())
                    .then_with(|| left.account_id.cmp(&right.account_id))
            });
            let every = accounts.iter().filter(|account| account.id != "all");
            let is_card = |account: &&Account| cards.contains(&account.id.as_str());
            let card_changes = every
                .clone()
                .filter(is_card)
                .map(|card| known.get(card.id.as_str()).copied())
                .collect::<Vec<_>>();
            Some(PeriodChange {
                window: *window,
                start,
                end: today,
                accounts: changed,
                incomplete,
                transactions: posted.iter().filter(|date| **date >= start).count(),
                groups: type_changes(every.clone(), &known),
                listed: type_changes(every.filter(|account| !is_card(account)), &known),
                cards: (!card_changes.is_empty())
                    .then(|| {
                        card_changes
                            .into_iter()
                            .sum::<Option<i64>>()
                            .map(from_minor)
                    })
                    .flatten(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn history(account: &str, points: &[(&str, f64)]) -> Performance {
        serde_json::from_value(json!({
            "accountId": account, "name": account, "institution": "Bank",
            "currentValue": points.last().unwrap().1,
            "points": points.iter().map(|(date, value)| json!({
                "date": date, "value": value, "netDeposits": null, "sp500": null,
            })).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn txn(id: &str, date: &str, pending: bool) -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": "Example", "category": "Shopping", "date": date,
            "amount": -10.0, "account": "Card", "pending": pending,
        }))
        .unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn each_period_measures_from_the_balance_before_it_began() {
        let balances = [
            history(
                "checking",
                &[
                    ("2026-06-01", 1_000.0),
                    ("2026-09-08", 1_500.0),
                    ("2026-09-12", 1_200.0),
                    ("2026-09-16", 1_250.5),
                ],
            ),
            // Opened this month: it has a balance a week ago but none three months ago.
            history("card", &[("2026-09-01", -100.0), ("2026-09-16", -400.0)]),
            history("total", &[("2026-06-01", 0.0), ("2026-09-16", 9.0)]),
            history("flat", &[("2026-06-01", 50.0), ("2026-09-16", 50.0)]),
        ];
        let brokerage = [history(
            "broker",
            &[("2026-09-01", 900.0), ("2026-09-16", 1_000.0)],
        )];
        let transactions = [
            txn("a", "2026-09-15", false),
            txn("b", "2026-09-01", false),
            txn("c", "2026-09-16", true),
            txn("d", "2026-05-20", false),
        ];
        let account = |id: &str, kind: &str| -> Account {
            serde_json::from_value(json!({
                "id": id, "name": id, "institution": "Bank", "type": kind, "value": 0.0,
            }))
            .unwrap()
        };
        let accounts = [
            account("all", "combined"),
            account("checking", "cash"),
            account("flat", "cash"),
            account("card", "credit"),
            account("broker", "brokerage"),
        ];
        let result = changes(
            &balances,
            &brokerage,
            &transactions,
            &accounts,
            &["card"],
            day("2026-09-16"),
        );
        let window = |window: Window| result.iter().find(|item| item.window == window).unwrap();
        let types = |changes: &[TypeChange]| {
            changes
                .iter()
                .map(|change| (change.account_type.clone(), change.change))
                .collect::<Vec<_>>()
        };
        let summary = |window: Window| {
            result
                .iter()
                .find(|item| item.window == window)
                .unwrap()
                .accounts
                .iter()
                .map(|account| (account.account_id.clone(), account.change))
                .collect::<Vec<_>>()
        };

        // The week is Sep 10–16: checking opened at 1,500 (Sep 8), card at -100 (Sep 1).
        let week = window(Window::Week);
        assert_eq!(week.start, day("2026-09-10"));
        assert_eq!(
            summary(Window::Week),
            vec![
                ("card".to_owned(), -300.0),
                ("checking".to_owned(), -249.5),
                ("broker".to_owned(), 100.0),
            ]
        );
        assert!(!week.incomplete);
        assert_eq!(
            week.transactions, 1,
            "pending and older activity are not counted"
        );
        // Each type totals its accounts, an unchanged one adding nothing; the spending card is
        // left out of what the totals count, and totaled apart.
        let owned = |kind: &str, change: Option<f64>| (kind.to_owned(), change);
        assert_eq!(
            types(&week.groups),
            [
                owned("cash", Some(-249.5)),
                owned("credit", Some(-300.0)),
                owned("brokerage", Some(100.0)),
            ]
        );
        assert_eq!(
            types(&week.listed),
            [owned("cash", Some(-249.5)), owned("brokerage", Some(100.0))]
        );
        assert_eq!(week.cards, Some(-300.0));

        // A quarter reaches before the card and brokerage histories begin.
        let quarter = window(Window::Quarter);
        assert_eq!(
            summary(Window::Quarter),
            vec![("checking".to_owned(), 250.5)]
        );
        assert!(quarter.incomplete);
        assert_eq!(quarter.transactions, 2);
        // A type whose account's history starts later has no change for the period.
        assert_eq!(
            types(&quarter.groups),
            [
                owned("cash", Some(250.5)),
                owned("credit", None),
                owned("brokerage", None),
            ]
        );
        assert_eq!(quarter.cards, None);

        // All history starts each account at its first balance, from the earliest record.
        let all = window(Window::All);
        assert_eq!(all.start, day("2026-05-20"));
        assert!(!all.incomplete);
        assert_eq!(
            all.accounts.len(),
            3,
            "an unchanged account and aggregates are left out"
        );
        assert_eq!(all.transactions, 3);
    }

    #[test]
    fn without_history_or_activity_there_is_no_all_time_period() {
        let result = changes(&[], &[], &[], &[], &[], day("2026-09-16"));
        // Every period but all time, which has no first day to start from.
        assert_eq!(result.len(), WINDOWS.len() - 1);
        assert!(result
            .iter()
            .all(|period| period.accounts.is_empty() && period.transactions == 0));
    }
}
