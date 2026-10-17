//! What moved through each account over each chart period, for its details: posted money in and
//! out, interest, and dividends, and for an investment account the new money and the market's
//! movement. The combined account counts every account; a brokerage counts the bank-feed
//! accounts linked to it.
use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::calendar;
use super::changes::{Window, WINDOWS};
use crate::finance_contract::{Performance, Snapshot};
use crate::numeric::{from_minor, minor_units};
use crate::transaction_policy::Mark;

/// The account that stands for every account.
const COMBINED: &str = "all";

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountFlows {
    pub account_id: String,
    pub window: Window,
    /// None for all history.
    pub start: Option<NaiveDate>,
    pub end: NaiveDate,
    pub money_in: f64,
    pub money_out: f64,
    pub net_flow: f64,
    pub interest: f64,
    pub dividends: f64,
    /// New money into an investment account; none without deposit history.
    pub contributions: Option<f64>,
    /// The market's movement in an investment account; none without it in the history.
    pub market_change: Option<f64>,
}

/// A day's or a month's spending and income across every account, for the calendar.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// `YYYY-MM-DD` for a day, `YYYY-MM` for a month.
    pub period: String,
    pub spent: f64,
    pub income: f64,
    pub net: f64,
}

/// Each day's and each month's spending and income, by the date each transaction carries.
/// Transfers and credits are neither; income, interest, and dividends are income; what native
/// policy counts as spending is spent.
pub fn calendar_totals(snapshot: &Snapshot) -> (Vec<Totals>, Vec<Totals>) {
    let mut days = std::collections::BTreeMap::<NaiveDate, (i64, i64)>::new();
    for transaction in &snapshot.transactions {
        let (Some(classification), Some(date)) = (
            transaction.classification.as_ref(),
            calendar::transaction_date(&transaction.date),
        ) else {
            continue;
        };
        if classification.mark == Mark::Transfer || classification.credit {
            continue;
        }
        let amount = minor_units(transaction.amount);
        let income = matches!(
            classification.mark,
            Mark::Income | Mark::Interest | Mark::Dividend
        );
        let entry = days.entry(date).or_default();
        if income {
            if amount > 0 {
                entry.1 += amount;
            }
        } else if classification.spending && amount < 0 {
            entry.0 -= amount;
        }
    }
    let totals = |period: String, (spent, income): (i64, i64)| Totals {
        period,
        spent: from_minor(spent),
        income: from_minor(income),
        net: from_minor(income - spent),
    };
    let mut months = std::collections::BTreeMap::<String, (i64, i64)>::new();
    for (date, (spent, income)) in &days {
        let month = months.entry(date.format("%Y-%m").to_string()).or_default();
        month.0 += spent;
        month.1 += income;
    }
    (
        days.into_iter()
            .filter(|(_, sums)| *sums != (0, 0))
            .map(|(date, sums)| totals(date.to_string(), sums))
            .collect(),
        months
            .into_iter()
            .filter(|(_, sums)| *sums != (0, 0))
            .map(|(month, sums)| totals(month, sums))
            .collect(),
    )
}

/// New money and market movement across `[start, end]` for investment series. The running
/// deposit total starts at the opening value, so contributions count only flows after the
/// period opens, or after tracking starts when that is later.
fn investment_flows(
    series: &[&Performance],
    start: Option<NaiveDate>,
    end: NaiveDate,
) -> (Option<f64>, Option<f64>) {
    let mut contributions: Option<i64> = None;
    let mut market: Option<i64> = None;
    for performance in series {
        if performance.performance_method.as_deref() == Some("value-only") {
            continue;
        }
        let dated = performance
            .points
            .iter()
            .filter_map(|point| Some((calendar::transaction_date(&point.date)?, point)))
            .collect::<Vec<_>>();
        let deposits = dated
            .iter()
            .filter_map(|(date, point)| Some((*date, point.net_deposits?)))
            .filter(|(date, _)| *date <= end)
            .collect::<Vec<_>>();
        let base = start
            .and_then(|start| deposits.iter().rev().find(|(date, _)| *date < start))
            .or(deposits.first());
        if let (Some((_, last)), Some((_, base))) = (deposits.last(), base) {
            *contributions.get_or_insert(0) += minor_units(*last) - minor_units(*base);
        }
        for (date, point) in &dated {
            let Some(change) = point.market_change else {
                continue;
            };
            if *date <= end && start.is_none_or(|start| *date >= start) {
                *market.get_or_insert(0) += minor_units(change);
            }
        }
    }
    (contributions.map(from_minor), market.map(from_minor))
}

/// Each account's flows, and the combined account's, over each chart period ending on the
/// snapshot's day.
pub fn flows(snapshot: &Snapshot) -> Vec<AccountFlows> {
    let Some(end) = calendar::transaction_date(&snapshot.updated_at) else {
        return Vec::new();
    };
    let posted = snapshot
        .transactions
        .iter()
        .filter(|transaction| !transaction.pending)
        .filter_map(|transaction| {
            let date = calendar::transaction_date(transaction.posted_date())?;
            (date <= end).then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    let investment = |kind: &str| matches!(kind, "brokerage" | "retirement");
    let mut result = Vec::new();
    for account in &snapshot.accounts {
        let combined = account.id == COMBINED;
        // A brokerage also counts the bank-feed accounts linked to it.
        let related = |id: &str| {
            if combined {
                snapshot
                    .accounts
                    .iter()
                    .any(|other| other.id == id && other.id != COMBINED)
            } else {
                id == account.id
                    || snapshot
                        .account_links
                        .get(id)
                        .is_some_and(|linked| *linked == account.id)
            }
        };
        let series = snapshot
            .brokerage_performance
            .iter()
            .filter(|performance| {
                snapshot.accounts.iter().any(|other| {
                    other.id == performance.account_id
                        && investment(&other.r#type)
                        && (combined || other.id == account.id)
                })
            })
            .collect::<Vec<_>>();
        for (window, days) in WINDOWS {
            let start = days.map(|days| end - Duration::days(days));
            let (mut money_in, mut money_out, mut interest, mut dividends) = (0, 0, 0, 0);
            for (date, transaction) in &posted {
                if start.is_some_and(|start| *date < start)
                    || !transaction.account_id.as_deref().is_some_and(&related)
                {
                    continue;
                }
                let amount = minor_units(transaction.amount);
                if amount > 0 {
                    money_in += amount;
                    match transaction.classification.as_ref().map(|c| &c.mark) {
                        Some(Mark::Interest) => interest += amount,
                        Some(Mark::Dividend) => dividends += amount,
                        _ => {}
                    }
                } else {
                    money_out -= amount;
                }
            }
            let (contributions, market_change) = investment_flows(&series, start, end);
            result.push(AccountFlows {
                account_id: account.id.clone(),
                window,
                start,
                end,
                money_in: from_minor(money_in),
                money_out: from_minor(money_out),
                net_flow: from_minor(money_in - money_out),
                interest: from_minor(interest),
                dividends: from_minor(dividends),
                contributions,
                market_change,
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn each_period_counts_posted_flows_and_new_money_after_it_opens() {
        let snapshot: Snapshot = serde_json::from_value({
            let mut value: serde_json::Value =
                serde_json::from_str(include_str!("../../../src/data/empty.json")).unwrap();
            value["updatedAt"] = json!("2026-09-16T12:00:00Z");
            value["accounts"] = json!([
                { "id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 0.0 },
                { "id": "cash", "name": "Cash", "institution": "Bank", "type": "cash", "value": 0.0 },
                { "id": "broker", "name": "Broker", "institution": "Bank", "type": "brokerage", "value": 0.0 },
            ]);
            let txn = |id: &str, date: &str, amount: f64, mark: &str, account: &str| {
                json!({
                    "id": id, "merchant": id, "category": "Other", "date": date, "amount": amount,
                    "account": account, "accountId": account, "pending": false, "description": null,
                    "classification": { "mark": mark, "kind": "income", "spending": false, "zelle": false },
                })
            };
            value["transactions"] = json!([
                txn("pay", "2026-09-15", 100.0, "income", "cash"),
                txn("interest", "2026-09-14", 0.5, "interest", "cash"),
                txn("rent", "2026-09-10", -40.0, "initial", "cash"),
                txn("old", "2026-01-02", 1_000.0, "income", "cash"),
                txn("dividend", "2026-09-12", 2.0, "dividend", "broker"),
            ]);
            value["brokeragePerformance"] = json!([{
                "accountId": "broker", "name": "Broker", "institution": "Bank", "currentValue": 0.0,
                "points": [
                    { "date": "2026-08-01", "value": 100.0, "netDeposits": 100.0, "sp500": null, "marketChange": 1.0 },
                    { "date": "2026-09-10", "value": 160.0, "netDeposits": 150.0, "sp500": null, "marketChange": 10.0 },
                    { "date": "2026-09-16", "value": 170.0, "netDeposits": 155.0, "sp500": null, "marketChange": 5.0 },
                ],
            }]);
            value
        })
        .unwrap();
        let result = flows(&snapshot);
        let find = |id: &str, window: Window| {
            result
                .iter()
                .find(|flows| flows.account_id == id && flows.window == window)
                .unwrap()
        };
        let week = find("cash", Window::Week);
        assert_eq!(
            week.start,
            Some(NaiveDate::from_ymd_opt(2026, 9, 9).unwrap())
        );
        assert_eq!(
            (week.money_in, week.money_out, week.net_flow, week.interest),
            (100.5, 40.0, 60.5, 0.5)
        );
        assert_eq!((week.contributions, week.market_change), (None, None));
        // The brokerage's new money after the week opened, and the market's movement in it.
        let broker = find("broker", Window::Week);
        assert_eq!(
            (broker.dividends, broker.contributions, broker.market_change),
            (2.0, Some(55.0), Some(15.0))
        );
        // All history counts from the first deposit, and the combined account counts everything.
        let all = find("all", Window::All);
        assert_eq!((all.money_in, all.start), (1_102.5, None));
        assert_eq!(
            (all.contributions, all.market_change),
            (Some(55.0), Some(16.0))
        );
        // The calendar's days and months: income is income, and a non-spending outflow is
        // neither.
        let (days, months) = calendar_totals(&snapshot);
        let day = days.iter().find(|day| day.period == "2026-09-15").unwrap();
        assert_eq!((day.spent, day.income, day.net), (0.0, 100.0, 100.0));
        assert!(days.iter().all(|day| day.period != "2026-09-10"));
        let september = months
            .iter()
            .find(|month| month.period == "2026-09")
            .unwrap();
        assert_eq!(september.income, 102.5);
    }

    #[test]
    fn the_calendar_counts_spending_and_income_but_never_transfers_or_credits() {
        let snapshot: Snapshot = serde_json::from_value({
            let mut value: serde_json::Value =
                serde_json::from_str(include_str!("../../../src/data/empty.json")).unwrap();
            value["updatedAt"] = json!("2026-09-16T12:00:00Z");
            let txn = |id: &str, amount: f64, mark: &str, spending: bool, credit: bool| {
                json!({
                    "id": id, "merchant": id, "category": "Other", "date": "2026-09-02",
                    "postedOn": "2026-09-04", "amount": amount, "account": "Card",
                    "accountId": "card", "pending": false, "description": null,
                    "classification": {
                        "mark": mark, "kind": "expense", "spending": spending,
                        "credit": credit, "zelle": false,
                    },
                })
            };
            value["transactions"] = json!([
                txn("dinner", -30.0, "initial", true, false),
                txn("credit", 10.0, "refund", false, true),
                txn("move", -500.0, "transfer", false, false),
                txn("pay", 200.0, "income", false, false),
            ]);
            value
        })
        .unwrap();
        let (days, _) = calendar_totals(&snapshot);
        // Counted on the date the transaction carries, not its posting date.
        assert_eq!(
            days,
            [Totals {
                period: "2026-09-02".into(),
                spent: 30.0,
                income: 200.0,
                net: 170.0,
            }]
        );
    }
}
