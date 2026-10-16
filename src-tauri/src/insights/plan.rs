//! A light, deterministic month view: category spending with pace, bill coverage over the
//! next 30 days, and cash runway from recent spending. Amounts are summed in whole cents.
use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use super::calendar;
use super::forecast::{EventState, ExpectedEvent};
use crate::finance_contract::Account;
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units};
use crate::transaction_policy::Kind;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryPlan {
    pub category: String,
    pub spent: f64,
    /// Month-end spending if the month continues at its current daily pace.
    pub projected: f64,
    /// Average monthly spending over the last three complete months.
    pub typical: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Known USD cash available in checking and savings accounts: the available balance
    /// where the bank reports one, so pending holds count.
    pub cash: f64,
    pub cash_incomplete: bool,
    pub expected_outflows: f64,
    pub expected_inflows: f64,
    /// Cash after the next 30 days of expected outflows, before expected income.
    pub after_outflows: f64,
    pub horizon_days: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub month_start: NaiveDate,
    pub days_elapsed: i64,
    pub days_in_month: i64,
    pub spent: f64,
    /// Month-end spending if the month continues at its current daily pace.
    pub projected: f64,
    pub categories: Vec<CategoryPlan>,
    pub coverage: Coverage,
    /// Average monthly personal spending over the last three complete months.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_spending: Option<f64>,
    /// Months of that spending known cash would cover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runway_months: Option<f64>,
}

/// Expense and reimbursement amounts per category in each calendar month, in cents.
fn monthly_spending(transactions: &[Transaction]) -> BTreeMap<NaiveDate, BTreeMap<String, i64>> {
    let mut months = BTreeMap::<NaiveDate, BTreeMap<String, i64>>::new();
    for transaction in transactions
        .iter()
        .filter(|transaction| !transaction.pending)
    {
        let Some(date) = calendar::transaction_date(transaction.posted_date()) else {
            continue;
        };
        match transaction.kind() {
            Some(Kind::Expense) if transaction.amount < 0.0 => {}
            // Reimbursements reduce the spending they repay, never below zero in total.
            Some(Kind::Reimbursement) if transaction.amount > 0.0 => {}
            _ => continue,
        }
        let month = months
            .entry(calendar::add_months_first(date, 0))
            .or_default();
        for (category, amount) in transaction.parts() {
            *month.entry(category.to_owned()).or_default() -= minor_units(amount);
        }
    }
    months
}

pub fn plan(
    transactions: &[Transaction],
    accounts: &[Account],
    expected: &[ExpectedEvent],
    today: NaiveDate,
) -> Plan {
    let month_start = calendar::add_months_first(today, 0);
    let month_end = calendar::month_end(today.year(), today.month());
    let days_in_month = month_end.day() as i64;
    let days_elapsed = today.day() as i64;
    let months = monthly_spending(transactions);
    let current = months.get(&month_start).cloned().unwrap_or_default();
    let complete = (1..=3)
        .map(|offset| calendar::add_months_first(month_start, -offset))
        .collect::<Vec<_>>();
    let history_months = complete
        .iter()
        .filter(|month| months.keys().next().is_some_and(|first| first <= *month))
        .count() as i64;
    let typical = |category: &str| {
        if history_months == 0 {
            return 0;
        }
        complete
            .iter()
            .filter_map(|month| months.get(month)?.get(category))
            .sum::<i64>()
            / history_months
    };
    let mut categories = current
        .iter()
        .map(|(category, spent)| {
            let spent = (*spent).max(0);
            CategoryPlan {
                category: category.clone(),
                spent: from_minor(spent),
                projected: from_minor(spent * days_in_month / days_elapsed.max(1)),
                typical: from_minor(typical(category).max(0)),
            }
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        right
            .spent
            .total_cmp(&left.spent)
            .then_with(|| left.category.cmp(&right.category))
    });
    let spent = current.values().sum::<i64>().max(0);

    let cash_accounts = accounts
        .iter()
        .filter(|account| account.r#type == "cash")
        .collect::<Vec<_>>();
    let cash = cash_accounts
        .iter()
        .filter(|account| account.currency.as_deref().is_none_or(|code| code == "USD"))
        .filter_map(|account| account.available_balance.or(account.value))
        .map(minor_units)
        .sum::<i64>();
    let cash_incomplete = cash_accounts.iter().any(|account| {
        account.value.is_none()
            || account
                .currency
                .as_deref()
                .is_some_and(|code| code != "USD")
    });
    let horizon = today + Duration::days(30);
    // A pending bill already left an available balance; counting it again would double it.
    let held = |event: &&ExpectedEvent| {
        event.state == EventState::Pending
            && cash_accounts.iter().any(|account| {
                Some(account.id.as_str()) == event.account_id.as_deref()
                    && account.available_balance.is_some()
            })
    };
    let upcoming = expected.iter().filter(|event| {
        event.date >= today - Duration::days(5) && event.date <= horizon && !held(event)
    });
    let (mut inflows, mut outflows) = (0, 0);
    for event in upcoming {
        let amount = minor_units(event.amount);
        if amount < 0 {
            outflows -= amount;
        } else {
            inflows += amount;
        }
    }
    let monthly = (history_months >= 2).then(|| {
        complete
            .iter()
            .filter_map(|month| months.get(month))
            .map(|month| month.values().sum::<i64>().max(0))
            .sum::<i64>()
            / history_months
    });
    Plan {
        month_start,
        days_elapsed,
        days_in_month,
        spent: from_minor(spent),
        projected: from_minor(spent * days_in_month / days_elapsed.max(1)),
        categories,
        coverage: Coverage {
            cash: from_minor(cash),
            cash_incomplete,
            expected_outflows: from_minor(outflows),
            expected_inflows: from_minor(inflows),
            after_outflows: from_minor(cash - outflows),
            horizon_days: 30,
        },
        monthly_spending: monthly.map(from_minor),
        runway_months: monthly
            .filter(|monthly| *monthly > 0 && cash > 0 && !cash_incomplete)
            .map(|monthly| (cash as f64 / monthly as f64 * 10.0).floor() / 10.0),
    }
}
