//! What each cash account earns and how its money moves: the interest posted in each chart
//! period with its annualized rate on today's balance, and its usual monthly net flow over all
//! of its posted history.
use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::calendar::{self, Zone};
use super::changes::{Window, WINDOWS};
use crate::finance_contract::{Account, Transaction};
use crate::numeric::{from_minor, minor_units, round2};
use crate::transaction_policy::Mark;

/// A monthly average needs this much history; a few days would be stretched into a month.
const PACE_DAYS: i64 = 60;
const DAYS_PER_MONTH: f64 = 365.2425 / 12.0;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInterest {
    pub window: Window,
    pub earned: f64,
    /// Earned over the period's days, as a yearly percentage of today's balance to two places;
    /// absent without a positive balance.
    pub annual_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountInterest {
    pub account_id: String,
    /// Posted money in less money out per month, over all posted history once it spans two
    /// months.
    pub monthly_net_flow: Option<f64>,
    pub periods: Vec<PeriodInterest>,
}

pub fn interest(
    accounts: &[Account],
    transactions: &[Transaction],
    reference: &str,
    today: NaiveDate,
    zone: &dyn Zone,
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
                    let date =
                        calendar::transaction_date(transaction.posted_date(), reference, zone)?;
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
                periods,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::insights::calendar::NamedZone;

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
        let found = interest(
            &[savings, card],
            &transactions,
            "2026-09-30T12:00:00Z",
            today,
            &NamedZone::Utc,
        );
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
            "2026-09-30T12:00:00Z",
            today,
            &NamedZone::Utc,
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
}
