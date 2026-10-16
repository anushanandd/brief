//! Accounts the user shapes by hand, applied when a committed snapshot is read: a closed
//! account leaves today's accounts, holdings, and totals while its history and activity stay,
//! and a manual account (one no provider reports) joins them, valued by its dated entries.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::{decimal_f64, money, scaled};
use crate::finance_contract::{
    Account, MarketProjection, Performance, PerformancePoint, Point, Snapshot, StatementPosition,
    Trade,
};
use crate::settings::{FinanceSettings, ManualAccount, ManualEntry};
use rust_decimal::Decimal;

/// What the renderer still needs to know about an account the user closed.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClosedAccount {
    pub id: String,
    pub name: String,
    pub institution: String,
    pub r#type: String,
    pub closed_on: String,
}

/// Daily history is bounded; a manual account entered further back starts here.
const MAX_HISTORY_DAYS: i64 = 366 * 15;

/// Applies closed and manual accounts to a snapshot as it is read, returning the accounts
/// that were closed. Net worth and the combined row are summed again from what remains.
pub fn adjust_snapshot(snapshot: &mut Snapshot, settings: &FinanceSettings) -> Vec<ClosedAccount> {
    let closed = remove_closed(&mut snapshot.accounts, settings);
    let closed_ids = closed
        .iter()
        .map(|account| account.id.as_str())
        .collect::<BTreeSet<_>>();
    snapshot
        .holdings
        .retain(|holding| !closed_ids.contains(holding.account_id.as_str()));
    snapshot
        .brokerage_performance
        .retain(|performance| !closed_ids.contains(performance.account_id.as_str()));
    snapshot
        .account_balance_history
        .retain(|performance| !closed_ids.contains(performance.account_id.as_str()));
    snapshot.possible_duplicate_accounts.retain(|candidate| {
        !closed_ids.contains(candidate.plaid_account_id.as_str())
            && !closed_ids.contains(candidate.snaptrade_account_id.as_str())
    });
    let today = snapshot
        .calendar_date
        .clone()
        .or_else(|| snapshot.updated_at.get(..10).map(str::to_owned))
        .and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok());
    // Statement rows for a manual account the user removed have nowhere to go.
    let known = |id: &str| {
        !id.starts_with("manual:")
            || settings
                .manual_accounts
                .iter()
                .any(|manual| manual.id == id)
    };
    snapshot
        .transactions
        .retain(|transaction| transaction.account_id.as_deref().is_none_or(known));
    snapshot.trades.retain(|trade| known(&trade.account_id));
    for manual in &settings.manual_accounts {
        for transaction in &mut snapshot.transactions {
            if transaction.account_id.as_deref() == Some(manual.id.as_str()) {
                transaction.account.clone_from(&manual.name);
            }
        }
        for trade in &mut snapshot.trades {
            if trade.account_id == manual.id {
                trade.account.clone_from(&manual.name);
            }
        }
    }
    if let Some(today) = today {
        let today_text = today.to_string();
        for valued in &valued(settings, snapshot) {
            let manual = &valued.account;
            let Some(value) = manual.value_on(&today_text) else {
                // Entered without a balance: listed, so a statement can be imported into it.
                if manual.entries.is_empty() {
                    snapshot.accounts.push(manual_account(
                        valued,
                        None,
                        &today_text,
                        &snapshot.updated_at,
                    ));
                }
                continue;
            };
            snapshot.accounts.push(manual_account(
                valued,
                Some(value),
                &today_text,
                &snapshot.updated_at,
            ));
            let history = manual_history(manual, today);
            for point in &mut snapshot.net_worth_history {
                if let Some(value) = manual.value_on(&point.date) {
                    point.value = money(
                        decimal_f64(point.value).unwrap_or_default()
                            + decimal_f64(value).unwrap_or_default(),
                    );
                }
            }
            snapshot.account_balance_history.push(Performance {
                account_id: manual.id.clone(),
                name: manual.name.clone(),
                institution: "Manual".into(),
                current_value: value,
                history_start: history.first().map(|point| point.date.clone()),
                performance_method: None,
                history_source: Some(
                    if valued.estimated_on(&today_text) {
                        "estimated"
                    } else {
                        "manual"
                    }
                    .into(),
                ),
                points: history,
            });
        }
    }
    snapshot.accounts[1..].sort_by(|left, right| left.id.cmp(&right.id));
    let (net_worth, incomplete) = totals(&mut snapshot.accounts);
    snapshot.net_worth = net_worth;
    snapshot.net_worth_incomplete = incomplete;
    closed
}

/// The same adjustment for a live valuation of `snapshot`, so the overlay agrees with the view
/// it covers.
pub fn adjust_projection(
    projection: &mut MarketProjection,
    snapshot: &Snapshot,
    settings: &FinanceSettings,
) {
    let closed = remove_closed(&mut projection.accounts, settings);
    let closed_ids = closed
        .iter()
        .map(|account| account.id.as_str())
        .collect::<BTreeSet<_>>();
    projection
        .holdings
        .retain(|holding| !closed_ids.contains(holding.account_id.as_str()));
    projection
        .brokerage_performance
        .retain(|performance| !closed_ids.contains(performance.account_id.as_str()));
    let today = chrono::Local::now().date_naive().to_string();
    for valued in &valued(settings, snapshot) {
        let value = valued.account.value_on(&today);
        if value.is_some() || valued.account.entries.is_empty() {
            projection.accounts.push(manual_account(
                valued,
                value,
                &today,
                &projection.updated_at,
            ));
        }
    }
    projection.accounts[1..].sort_by(|left, right| left.id.cmp(&right.id));
    let (net_worth, incomplete) = totals(&mut projection.accounts);
    projection.net_worth = net_worth;
    projection.net_worth_incomplete = incomplete;
}

/// Units held and the latest price, by fund.
type Positions<'a> = BTreeMap<&'a str, (Decimal, Decimal)>;

/// Fund units an imported statement shows a manual account holding on each day it prices
/// them, each at the latest price the statement records for it.
fn statement_positions<'a>(account_id: &str, trades: &'a [Trade]) -> Vec<(&'a str, Positions<'a>)> {
    let mut trades = trades
        .iter()
        .filter(|trade| trade.account_id == account_id)
        .filter_map(|trade| {
            let units = decimal_f64(trade.units?)?;
            let price = decimal_f64(trade.price?)?;
            Some((
                trade.date.as_str(),
                trade.ticker.as_deref()?,
                trade,
                units,
                price,
            ))
        })
        .collect::<Vec<_>>();
    trades.sort_by_key(|(date, ..)| *date);
    let mut held = BTreeMap::<&str, (Decimal, Decimal)>::new();
    let mut days = Vec::new();
    for (date, ticker, trade, units, price) in trades {
        let position = held.entry(ticker).or_default();
        position.0 += if trade.r#type == "SELL" {
            -units
        } else {
            units
        };
        position.1 = price;
        match days.last_mut() {
            Some((day, positions)) if *day == date => *positions = held.clone(),
            _ => days.push((date, held.clone())),
        }
    }
    days
}

/// A manual account as the view values it.
struct Valued {
    /// Its dated values: the user's entries, the statement's, and any estimates after them.
    account: ManualAccount,
    /// The last day a value was observed rather than estimated.
    observed_through: Option<String>,
    /// Every day a value was observed: an entry or a statement's price.
    observed_on: Vec<String>,
    positions: Vec<StatementPosition>,
}

impl Valued {
    /// Whether the value on `date` is an estimate, carried past the last observed day.
    fn estimated_on(&self, date: &str) -> bool {
        self.account.tracks_sp500
            && self
                .observed_through
                .as_deref()
                .is_some_and(|observed| date > observed)
            && self.account.value_on(date).is_some()
    }
}

/// The S&P 500 (dividends reinvested) on `date`: the last saved close on or before it.
fn sp500_on(benchmark: &[Point], date: &str) -> Option<Decimal> {
    benchmark
        .iter()
        .take_while(|point| point.date.as_str() <= date)
        .last()
        .and_then(|point| decimal_f64(point.value))
        .filter(|value| value.is_sign_positive() && !value.is_zero())
}

/// Manual accounts as the view values them: the user's dated entries, joined by the value an
/// imported statement shows on each day it prices the account's funds; an entry wins its day.
/// One that tracks the S&P 500 is estimated on every other market day by moving the last
/// observed value with the index, until a newer entry or statement replaces the estimate.
fn valued(settings: &FinanceSettings, snapshot: &Snapshot) -> Vec<Valued> {
    settings
        .manual_accounts
        .iter()
        .map(|manual| {
            let mut entries = statement_positions(&manual.id, &snapshot.trades)
                .into_iter()
                .map(|(date, positions)| {
                    let value = positions
                        .values()
                        .map(|(units, price)| units * price)
                        .sum::<Decimal>();
                    (date.to_owned(), money(value))
                })
                .collect::<BTreeMap<_, _>>();
            for entry in &manual.entries {
                entries.insert(entry.date.clone(), entry.value);
            }
            let observed = entries
                .iter()
                .next_back()
                .map(|(date, value)| (date.clone(), *value));
            let observed_on = entries.keys().cloned().collect::<Vec<_>>();
            if manual.tracks_sp500 {
                // Each market day without an observation moves with the index from the last
                // observation before it, so the history has the shape of the market.
                let estimates = snapshot
                    .benchmark_history
                    .iter()
                    .filter(|point| !entries.contains_key(&point.date))
                    .filter_map(|point| {
                        let (date, value) = entries.range(..point.date.clone()).next_back()?;
                        let base = sp500_on(&snapshot.benchmark_history, date)?;
                        let index = decimal_f64(point.value)?;
                        Some((
                            point.date.clone(),
                            money(decimal_f64(*value)? * index / base),
                        ))
                    })
                    .collect::<Vec<_>>();
                entries.extend(estimates);
            }
            Valued {
                account: ManualAccount {
                    entries: entries
                        .into_iter()
                        .map(|(date, value)| ManualEntry { date, value })
                        .collect(),
                    ..manual.clone()
                },
                observed_through: observed.map(|(date, _)| date),
                observed_on,
                positions: statement_holdings(&manual.id, &snapshot.trades),
            }
        })
        .collect()
}

/// The fund units the latest statement shows a manual account holding, at its prices.
fn statement_holdings(account_id: &str, trades: &[Trade]) -> Vec<StatementPosition> {
    let Some((date, positions)) = statement_positions(account_id, trades).pop() else {
        return Vec::new();
    };
    positions
        .into_iter()
        .filter(|(_, (units, _))| units.is_sign_positive() && !units.is_zero())
        .map(|(name, (units, price))| StatementPosition {
            name: name.to_owned(),
            units: scaled(units, 8),
            price: scaled(price, 8),
            value: money(units * price),
            priced_on: date.to_owned(),
        })
        .collect()
}

fn remove_closed(accounts: &mut Vec<Account>, settings: &FinanceSettings) -> Vec<ClosedAccount> {
    let mut closed = Vec::new();
    accounts.retain(|account| {
        if account.id == "all" {
            return true;
        }
        match settings.account_closed_dates.get(&account.id) {
            Some(date) => {
                closed.push(ClosedAccount {
                    id: account.id.clone(),
                    name: account.name.clone(),
                    institution: account.institution.clone(),
                    r#type: account.r#type.clone(),
                    closed_on: date.clone(),
                });
                false
            }
            None => true,
        }
    });
    closed
}

/// A manual account as the view lists it; `value` is `None` until it has a balance.
/// `balance_as_of` is the last observed day; an estimate after it says so in its source.
fn manual_account(valued: &Valued, value: Option<f64>, today: &str, as_of: &str) -> Account {
    let manual = &valued.account;
    let estimated = valued.estimated_on(today);
    Account {
        id: manual.id.clone(),
        name: manual.name.clone(),
        institution: "Manual".into(),
        r#type: manual.kind.clone(),
        value,
        currency: Some("USD".into()),
        balance_as_of: valued.observed_through.clone(),
        observed_on: valued.observed_on.clone(),
        statement_positions: valued.positions.clone(),
        balance_fetched_at: Some(as_of.to_owned()),
        balance_source: Some(
            match (value, estimated) {
                (None, _) => "unavailable",
                (Some(_), true) => "estimated",
                (Some(_), false) => "reported",
            }
            .into(),
        ),
        reported_balance: if estimated { None } else { value },
        ..Account::default()
    }
}

/// One point a day from the first entry through today, each the latest entry by then.
fn manual_history(manual: &ManualAccount, today: NaiveDate) -> Vec<PerformancePoint> {
    let Some(first) = manual
        .entries
        .first()
        .and_then(|entry| NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").ok())
    else {
        return Vec::new();
    };
    let start = first.max(today - Duration::days(MAX_HISTORY_DAYS));
    let mut points = Vec::new();
    let mut day = start;
    while day <= today {
        let date = day.to_string();
        if let Some(value) = manual.value_on(&date) {
            points.push(PerformancePoint {
                date,
                value,
                net_deposits: None,
                sp500: None,
                market_change: None,
                market_change_pct: None,
            });
        }
        day += Duration::days(1);
    }
    points
}

/// Net worth from every account but the combined row, which takes the result, and whether
/// an account without a value left it incomplete.
fn totals(accounts: &mut [Account]) -> (f64, bool) {
    let rows = accounts.iter().filter(|account| account.id != "all");
    // A manual account awaiting its first balance has nothing to count yet.
    let incomplete = rows
        .clone()
        .any(|account| account.value.is_none() && !account.id.starts_with("manual:"));
    let net_worth = rows
        .filter_map(|account| account.value.and_then(decimal_f64))
        .sum::<Decimal>();
    if let Some(all) = accounts.iter_mut().find(|account| account.id == "all") {
        all.value = Some(money(net_worth));
    }
    (money(net_worth), incomplete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ManualEntry;
    use serde_json::json;

    fn snapshot() -> Snapshot {
        serde_json::from_value(json!({
            "updatedAt": "2026-09-30T12:00:00Z",
            "calendarDate": "2026-09-30",
            "netWorth": 1_500.0,
            "accounts": [
                {"id": "all", "name": "All accounts", "institution": "Brief", "type": "combined", "value": 1_500.0},
                {"id": "plaid:card", "name": "Card", "institution": "Bank", "type": "credit", "value": -500.0},
                {"id": "plaid:checking", "name": "Checking", "institution": "Bank", "type": "cash", "value": 2_000.0},
            ],
            "holdings": [],
            "transactions": [],
            "spending": {"monthTotal": 0.0, "categories": []},
            "netWorthHistory": [
                {"date": "2026-09-28", "value": 1_400.0},
                {"date": "2026-09-29", "value": 1_450.0},
                {"date": "2026-09-30", "value": 1_500.0},
            ],
            "accountBalanceHistory": [
                {"accountId": "plaid:card", "name": "Card", "institution": "Bank", "currentValue": -500.0, "points": []}
            ],
        }))
        .unwrap()
    }

    #[test]
    fn closed_accounts_leave_todays_picture_and_manual_accounts_join_it() {
        let settings = FinanceSettings {
            account_closed_dates: [("plaid:card".to_owned(), "2026-09-01".to_owned())].into(),
            manual_accounts: vec![
                ManualAccount {
                    id: "manual:k".into(),
                    name: "Old 401(k)".into(),
                    kind: "retirement".into(),
                    tracks_sp500: false,
                    entries: vec![
                        ManualEntry {
                            date: "2026-09-29".into(),
                            value: 10_000.0,
                        },
                        ManualEntry {
                            date: "2026-09-30".into(),
                            value: 10_100.0,
                        },
                    ],
                },
                ManualAccount {
                    id: "manual:car".into(),
                    name: "Car loan".into(),
                    kind: "loan".into(),
                    tracks_sp500: false,
                    entries: vec![ManualEntry {
                        date: "2026-01-01".into(),
                        value: 3_000.0,
                    }],
                },
                ManualAccount {
                    id: "manual:future".into(),
                    name: "Not yet".into(),
                    kind: "cash".into(),
                    tracks_sp500: false,
                    entries: vec![ManualEntry {
                        date: "2026-10-01".into(),
                        value: 99.0,
                    }],
                },
            ],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        let mut snapshot = snapshot();
        let closed = adjust_snapshot(&mut snapshot, &settings);
        assert_eq!(closed.len(), 1);
        assert_eq!(
            (closed[0].id.as_str(), closed[0].closed_on.as_str()),
            ("plaid:card", "2026-09-01")
        );
        let ids = snapshot
            .accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["all", "manual:car", "manual:k", "plaid:checking"]);
        // 2,000 checking + 10,100 retirement − 3,000 owed.
        assert_eq!(snapshot.net_worth, 9_100.0);
        assert_eq!(snapshot.accounts[0].value, Some(9_100.0));
        assert!(!snapshot.net_worth_incomplete);
        let history = snapshot
            .net_worth_history
            .iter()
            .map(|point| point.value)
            .collect::<Vec<_>>();
        // The loan has been owed all along; the 401(k) joins on its first entry.
        assert_eq!(history, [-1_600.0, 8_450.0, 8_600.0]);
        assert!(snapshot
            .account_balance_history
            .iter()
            .all(|performance| performance.account_id != "plaid:card"));
        let retirement = snapshot
            .account_balance_history
            .iter()
            .find(|performance| performance.account_id == "manual:k")
            .unwrap();
        assert_eq!(retirement.history_start.as_deref(), Some("2026-09-29"));
        assert_eq!(retirement.points.len(), 2);
        assert_eq!(retirement.points[1].value, 10_100.0);
    }

    #[test]
    fn a_statement_values_a_manual_account_and_names_its_rows() {
        let buy = |id: &str, date: &str, units: f64, price: f64| Trade {
            id: id.into(),
            r#type: "BUY".into(),
            date: date.into(),
            amount: -(units * price),
            account: "Manual account".into(),
            account_id: "manual:k".into(),
            ticker: Some("Index Fund".into()),
            units: Some(units),
            price: Some(price),
            ..Trade::default()
        };
        let mut snapshot = snapshot();
        snapshot.trades = vec![
            buy("first", "2026-09-28", 2.0, 50.0),
            buy("second", "2026-09-29", 1.0, 60.0),
            Trade {
                account_id: "manual:removed".into(),
                ..buy("orphan", "2026-09-29", 1.0, 1.0)
            },
        ];
        let settings = FinanceSettings {
            manual_accounts: vec![ManualAccount {
                id: "manual:k".into(),
                name: "401(k)".into(),
                kind: "retirement".into(),
                tracks_sp500: false,
                // An entry wins its own day; the statement fills the others.
                entries: vec![ManualEntry {
                    date: "2026-09-30".into(),
                    value: 200.0,
                }],
            }],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        adjust_snapshot(&mut snapshot, &settings);
        let retirement = snapshot
            .account_balance_history
            .iter()
            .find(|history| history.account_id == "manual:k")
            .unwrap();
        let points = retirement
            .points
            .iter()
            .map(|point| (point.date.as_str(), point.value))
            .collect::<Vec<_>>();
        // 2 units at 50, then 3 units at 60, then the entry.
        assert_eq!(
            points,
            [
                ("2026-09-28", 100.0),
                ("2026-09-29", 180.0),
                ("2026-09-30", 200.0)
            ]
        );
        // Without a market price, the fund stays off the holdings list and on the account.
        assert!(snapshot.holdings.is_empty());
        let account = snapshot
            .accounts
            .iter()
            .find(|account| account.id == "manual:k")
            .unwrap();
        let [position] = account.statement_positions.as_slice() else {
            panic!("one statement position")
        };
        assert_eq!((position.units, position.value), (3.0, 180.0));
        assert_eq!(
            account.observed_on,
            ["2026-09-28", "2026-09-29", "2026-09-30"]
        );
        assert_eq!(snapshot.trades.len(), 2);
        assert!(snapshot
            .trades
            .iter()
            .all(|trade| trade.account == "401(k)"));
    }

    #[test]
    fn a_tracked_account_moves_with_the_sp500_after_its_last_statement() {
        let mut snapshot = snapshot();
        snapshot.trades = vec![Trade {
            id: "buy".into(),
            r#type: "BUY".into(),
            date: "2026-09-28".into(),
            amount: -100.0,
            account: "Manual account".into(),
            account_id: "manual:k".into(),
            ticker: Some("Index Fund".into()),
            units: Some(2.0),
            price: Some(50.0),
            ..Trade::default()
        }];
        let point = |date: &str, value: f64| Point {
            date: date.into(),
            value,
        };
        snapshot.benchmark_history = vec![
            point("2026-09-27", 90.0),
            point("2026-09-28", 100.0),
            point("2026-09-29", 110.0),
            point("2026-09-30", 121.0),
        ];
        let settings = FinanceSettings {
            manual_accounts: vec![ManualAccount {
                id: "manual:k".into(),
                name: "401(k)".into(),
                kind: "retirement".into(),
                entries: Vec::new(),
                tracks_sp500: true,
            }],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        adjust_snapshot(&mut snapshot, &settings);
        let account = snapshot
            .accounts
            .iter()
            .find(|account| account.id == "manual:k")
            .unwrap();
        // 100 on the statement's day, then up 10% and 21% with the index.
        assert_eq!(account.value, Some(121.0));
        assert_eq!(account.balance_source.as_deref(), Some("estimated"));
        assert_eq!(account.balance_as_of.as_deref(), Some("2026-09-28"));
        let history = snapshot
            .account_balance_history
            .iter()
            .find(|history| history.account_id == "manual:k")
            .unwrap();
        assert_eq!(history.history_source.as_deref(), Some("estimated"));
        assert_eq!(
            history
                .points
                .iter()
                .map(|point| point.value)
                .collect::<Vec<_>>(),
            [100.0, 110.0, 121.0]
        );
        // The position keeps the statement's own price; only the account is estimated.
        assert_eq!(account.statement_positions[0].price, 50.0);
        assert_eq!(account.observed_on, ["2026-09-28"]);
    }

    #[test]
    fn a_tracked_account_moves_with_the_index_between_its_statements() {
        let mut snapshot = snapshot();
        let buy = |id: &str, date: &str, units: f64, price: f64| Trade {
            id: id.into(),
            r#type: "BUY".into(),
            date: date.into(),
            amount: -(units * price),
            account: "Manual account".into(),
            account_id: "manual:k".into(),
            ticker: Some("Index Fund".into()),
            units: Some(units),
            price: Some(price),
            ..Trade::default()
        };
        snapshot.trades = vec![
            buy("first", "2026-09-28", 2.0, 50.0),
            buy("second", "2026-09-30", 1.0, 60.0),
        ];
        snapshot.benchmark_history = [
            ("2026-09-28", 100.0),
            ("2026-09-29", 105.0),
            ("2026-09-30", 120.0),
        ]
        .map(|(date, value)| Point {
            date: date.into(),
            value,
        })
        .to_vec();
        let settings = FinanceSettings {
            manual_accounts: vec![ManualAccount {
                id: "manual:k".into(),
                name: "401(k)".into(),
                kind: "retirement".into(),
                entries: Vec::new(),
                tracks_sp500: true,
            }],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        adjust_snapshot(&mut snapshot, &settings);
        let history = snapshot
            .account_balance_history
            .iter()
            .find(|history| history.account_id == "manual:k")
            .unwrap();
        // 100 observed, 105 estimated with the index, then the second statement's 180.
        assert_eq!(
            history
                .points
                .iter()
                .map(|point| point.value)
                .collect::<Vec<_>>(),
            [100.0, 105.0, 180.0]
        );
        let account = snapshot
            .accounts
            .iter()
            .find(|account| account.id == "manual:k")
            .unwrap();
        assert_eq!(account.observed_on, ["2026-09-28", "2026-09-30"]);
        // Observed today, so the account itself is not an estimate.
        assert_eq!(account.balance_source.as_deref(), Some("reported"));
    }

    #[test]
    fn a_manual_account_entered_without_a_balance_is_listed_and_counts_nothing() {
        let settings = FinanceSettings {
            manual_accounts: vec![ManualAccount {
                id: "manual:new".into(),
                name: "New 401(k)".into(),
                kind: "retirement".into(),
                entries: Vec::new(),
                tracks_sp500: false,
            }],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        let mut snapshot = snapshot();
        adjust_snapshot(&mut snapshot, &settings);
        let listed = snapshot
            .accounts
            .iter()
            .find(|account| account.id == "manual:new")
            .unwrap();
        assert_eq!(listed.value, None);
        assert_eq!(snapshot.net_worth, 1_500.0);
        assert!(!snapshot.net_worth_incomplete);
    }

    #[test]
    fn an_account_without_a_value_keeps_totals_incomplete() {
        let mut snapshot = snapshot();
        snapshot.accounts[2].value = None;
        adjust_snapshot(&mut snapshot, &FinanceSettings::default());
        assert!(snapshot.net_worth_incomplete);
        assert_eq!(snapshot.net_worth, -500.0);
    }
}
