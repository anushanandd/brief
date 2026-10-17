//! What each cash account earns and how its money moves: the interest posted in each chart
//! period with its annualized rate on today's balance, and its usual monthly net flow over all
//! of its posted history.
use chrono::{Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::calendar::{self};
use super::changes::{Window, WINDOWS};
use crate::finance_contract::{Account, Transaction};
use crate::numeric::{from_minor, minor_units, round2};
use crate::transaction_policy::Mark;

/// A monthly average needs this much history; a few days would be stretched into a month.
const PACE_DAYS: i64 = 60;
const DAYS_PER_MONTH: f64 = 365.2425 / 12.0;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInterest {
    pub window: Window,
    pub earned: f64,
    /// Earned over the period's days, as a yearly percentage of today's balance to two places;
    /// absent without a positive balance.
    pub annual_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountInterest {
    pub account_id: String,
    /// Posted money in less money out per month, over all posted history once it spans two
    /// months.
    pub monthly_net_flow: Option<f64>,
    /// Days from the first posted transaction through today; absent without any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub history_days: Option<i64>,
    pub periods: Vec<PeriodInterest>,
}

/// Cash that earns little while another of the user's cash accounts pays more.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdleCash {
    pub account_id: String,
    pub balance: f64,
    /// The quarter's rate a year, when the account has posted a quarter of history.
    pub annual_rate: Option<f64>,
    pub better_account_id: String,
    pub better_rate: f64,
}

/// A balance worth moving.
const IDLE_MIN_BALANCE: f64 = 2_500.0;
/// At or below this yearly rate cash counts as idle.
const IDLE_MAX_RATE: f64 = 0.5;
/// A rate that makes another account worth moving to.
const BETTER_MIN_RATE: f64 = 1.0;
/// Rates are judged over a quarter, so a month without a posting isn't a verdict.
const RATE_HISTORY_DAYS: i64 = 90;

/// Cash accounts with at least `IDLE_MIN_BALANCE` earning `IDLE_MAX_RATE` or less over the
/// last quarter, each paired with the user's best-paying cash account, when one pays
/// `BETTER_MIN_RATE` or more. Both need a quarter of posted history to be judged.
pub fn idle_cash(accounts: &[Account], interest: &[AccountInterest]) -> Vec<IdleCash> {
    let quarter_rate = |item: &AccountInterest| {
        item.history_days
            .filter(|days| *days >= RATE_HISTORY_DAYS)
            .and_then(|_| {
                item.periods
                    .iter()
                    .find(|period| period.window == Window::Quarter)
                    .map(|period| period.annual_rate.unwrap_or(0.0))
            })
    };
    let Some((better_account_id, better_rate)) = interest
        .iter()
        .filter_map(|item| Some((item.account_id.as_str(), quarter_rate(item)?)))
        .filter(|(_, rate)| *rate >= BETTER_MIN_RATE)
        .max_by(|left, right| left.1.total_cmp(&right.1))
    else {
        return Vec::new();
    };
    interest
        .iter()
        .filter(|item| item.account_id != better_account_id)
        .filter_map(|item| {
            let rate = quarter_rate(item)?;
            let balance = accounts
                .iter()
                .find(|account| account.id == item.account_id)?
                .value?;
            (rate <= IDLE_MAX_RATE && balance >= IDLE_MIN_BALANCE).then(|| IdleCash {
                account_id: item.account_id.clone(),
                balance,
                annual_rate: Some(rate),
                better_account_id: better_account_id.to_owned(),
                better_rate,
            })
        })
        .collect()
}

pub fn interest(
    accounts: &[Account],
    transactions: &[Transaction],
    today: NaiveDate,
) -> Vec<AccountInterest> {
    accounts
        .iter()
        .filter(|account| account.r#type == "cash")
        .map(|account| {
            let posted = transactions
                .iter()
                .filter(|transaction| {
                    !transaction.pending
                        && transaction.account_id.as_deref() == Some(account.id.as_str())
                })
                .filter_map(|transaction| {
                    let date = calendar::transaction_date(transaction.posted_date())?;
                    (date <= today).then_some((date, transaction))
                })
                .collect::<Vec<_>>();
            let history_days = posted
                .iter()
                .map(|(date, _)| *date)
                .min()
                .map(|first| (today - first).num_days() + 1);
            let monthly_net_flow = history_days.filter(|days| *days >= PACE_DAYS).map(|days| {
                let net = posted
                    .iter()
                    .map(|(_, transaction)| minor_units(transaction.amount))
                    .sum::<i64>();
                from_minor(minor_units(from_minor(net) / days as f64 * DAYS_PER_MONTH))
            });
            let balance = account.value.filter(|value| *value > 0.0);
            let periods = WINDOWS
                .iter()
                .filter_map(|(window, days)| {
                    let days = days.or(history_days)?;
                    let start = today - Duration::days(days - 1);
                    let earned = from_minor(
                        posted
                            .iter()
                            .filter(|(date, transaction)| {
                                *date >= start
                                    && transaction.amount > 0.0
                                    && transaction
                                        .classification
                                        .as_ref()
                                        .is_some_and(|value| value.mark == Mark::Interest)
                            })
                            .map(|(_, transaction)| minor_units(transaction.amount))
                            .sum(),
                    );
                    Some(PeriodInterest {
                        window: *window,
                        earned,
                        annual_rate: balance.map(|balance| {
                            round2(earned / balance * (365.0 / days as f64) * 100.0)
                        }),
                    })
                })
                .collect();
            AccountInterest {
                account_id: account.id.clone(),
                monthly_net_flow,
                history_days,
                periods,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn transaction(id: &str, date: &str, amount: f64, mark: &str) -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": "Example", "category": "Other", "date": date, "amount": amount,
            "account": "Savings", "accountId": "savings", "pending": false,
            "classification": {
                "mark": mark, "kind": "income", "spending": false, "credit": false, "zelle": false
            },
        }))
        .unwrap()
    }

    #[test]
    fn interest_is_annualized_on_todays_balance_and_net_flow_needs_two_months() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let savings: Account = serde_json::from_value(json!({
            "id": "savings", "name": "Savings", "institution": "Bank", "type": "cash",
            "value": 10_000.0,
        }))
        .unwrap();
        let card: Account = serde_json::from_value(json!({
            "id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": -5.0,
        }))
        .unwrap();
        let transactions = [
            transaction("deposit", "2026-06-02", 3_000.0, "transfer"),
            transaction("old-interest", "2026-08-30", 20.0, "interest"),
            transaction("interest", "2026-09-30", 30.0, "interest"),
        ];
        let found = interest(&[savings, card], &transactions, today);
        assert_eq!(found.len(), 1, "only cash accounts");
        let month = found[0]
            .periods
            .iter()
            .find(|period| period.window == Window::Month)
            .unwrap();
        // The month counts the 30 days through today, so August 30 is outside it.
        assert_eq!(month.earned, 30.0);
        assert_eq!(month.annual_rate, Some(3.65));
        let all = found[0].periods.last().unwrap();
        assert_eq!((all.window, all.earned), (Window::All, 50.0));
        // 3,050 over the 121 days since June 2, per month.
        assert_eq!(
            found[0].monthly_net_flow,
            Some(from_minor(minor_units(3_050.0 / 121.0 * DAYS_PER_MONTH)))
        );
        let recent = interest(
            &[serde_json::from_value(json!({
                "id": "savings", "name": "Savings", "institution": "Bank", "type": "cash",
                "value": 0.0,
            }))
            .unwrap()],
            &transactions[2..],
            today,
        );
        assert_eq!(
            recent[0].monthly_net_flow, None,
            "a day of history has no pace"
        );
        assert!(recent[0]
            .periods
            .iter()
            .all(|period| period.annual_rate.is_none()));
    }

    #[test]
    fn idle_cash_needs_a_better_paying_account_and_a_quarter_of_history() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let account = |id: &str, value: f64| -> Account {
            serde_json::from_value(json!({
                "id": id, "name": id, "institution": "Bank", "type": "cash", "value": value,
            }))
            .unwrap()
        };
        let posted = |account: &str, id: &str, date: &str, amount: f64, mark: &str| {
            let mut transaction = transaction(id, date, amount, mark);
            transaction.account_id = Some(account.into());
            transaction
        };
        let accounts = [
            account("checking", 8_000.0),
            account("savings", 10_000.0),
            account("small", 400.0),
        ];
        // Savings earns about 4% a year; checking has posted for a year and earned nothing.
        let transactions = [
            posted("checking", "c-open", "2025-09-01", 5_000.0, "transfer"),
            posted("savings", "s-open", "2025-09-01", 9_000.0, "transfer"),
            posted("savings", "s-i1", "2026-07-31", 33.0, "interest"),
            posted("savings", "s-i2", "2026-08-31", 33.0, "interest"),
            posted("savings", "s-i3", "2026-09-30", 33.0, "interest"),
            posted("small", "sm-open", "2025-09-01", 400.0, "transfer"),
        ];
        let found = interest(&accounts, &transactions, today);
        let idle = idle_cash(&accounts, &found);
        assert_eq!(idle.len(), 1, "a small balance isn't worth moving");
        assert_eq!(idle[0].account_id, "checking");
        assert_eq!(idle[0].better_account_id, "savings");
        assert_eq!(idle[0].annual_rate, Some(0.0));
        assert!(idle[0].better_rate > 3.5);
        // A new account hasn't shown what it pays.
        let young = [
            posted("checking", "c-open", "2026-09-15", 8_000.0, "transfer"),
            transactions[1].clone(),
            transactions[2].clone(),
            transactions[3].clone(),
            transactions[4].clone(),
        ];
        assert!(idle_cash(&accounts, &interest(&accounts, &young, today)).is_empty());
        // Without a better account there is nowhere to move it.
        assert!(idle_cash(&accounts, &interest(&accounts, &transactions[..1], today)).is_empty());
    }
}
