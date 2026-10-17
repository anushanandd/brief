//! Credit cards: how much of each limit is in use, overall and per card, and what the issuer
//! charged in interest on the last statement. Issuers report the statement balance to the
//! credit bureaus, so a card's next closing date is when its utilization is recorded. Amounts
//! are summed in whole cents; a card without a reported limit is left out of the totals and
//! counted, never assumed.
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::calendar;
use super::statement::StatementCycle;
use crate::finance_contract::Account;
use crate::numeric::{from_minor, minor_units};

/// The share of a limit commonly cited as the point where utilization starts to cost score.
/// It is a widely repeated guide, not a published scoring rule, and the interface says so.
pub const HIGH_UTILIZATION: f64 = 30.0;
/// How far ahead of a statement closing a high balance is worth raising.
pub const CLOSING_NOTICE_DAYS: i64 = 7;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CardCredit {
    pub account_id: String,
    /// What is owed now, positive.
    pub balance: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub available: Option<f64>,
    /// Percent of the limit in use now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub utilization: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub statement_utilization: Option<f64>,
    /// The next statement closing date: the issuer's cycle for the spending card, otherwise a
    /// month after the last reported statement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub closes_on: Option<NaiveDate>,
    pub closes_estimated: bool,
    /// Interest the issuer charged on the last statement, across every rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interest_charged: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Credit {
    pub cards: Vec<CardCredit>,
    /// Across cards with a reported limit.
    pub balance: f64,
    pub limit: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub utilization: Option<f64>,
    /// Cards whose issuer reports no limit, left out of the overall figure.
    pub cards_without_limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interest_charged: Option<f64>,
}

/// Percent with two decimals, from whole cents.
fn percent(part: i64, whole: i64) -> Option<f64> {
    (whole > 0).then(|| (part as f64 * 10_000.0 / whole as f64).round() / 100.0)
}

/// The first monthly anniversary of the last closing date on or after today. A cycle that
/// closes at month end keeps closing at month end, and one on the 31st closes on the last day
/// of shorter months.
fn next_monthly_close(last: NaiveDate, today: NaiveDate) -> NaiveDate {
    let mut months = 1;
    loop {
        let index = calendar::month_index(last) + months;
        let (year, month) = (index.div_euclid(12) as i32, index.rem_euclid(12) as u32 + 1);
        let end = calendar::month_end(year, month);
        let close = if calendar::is_month_end(last) {
            end
        } else {
            NaiveDate::from_ymd_opt(year, month, last.day()).unwrap_or(end)
        };
        if close >= today {
            return close;
        }
        months += 1;
    }
}

pub fn credit(
    accounts: &[Account],
    spending_card: Option<(&str, &StatementCycle)>,
    today: NaiveDate,
) -> Credit {
    let usd = |account: &&Account| account.currency.as_deref().is_none_or(|code| code == "USD");
    let mut credit = Credit::default();
    let (mut balance, mut limit, mut interest) = (0_i64, 0_i64, None::<i64>);
    for account in accounts
        .iter()
        .filter(|account| account.r#type == "credit")
        .filter(usd)
    {
        let Some(value) = account.value else {
            continue;
        };
        let owed = minor_units(-value).max(0);
        let card_limit = account
            .credit_limit
            .map(minor_units)
            .filter(|limit| *limit > 0);
        let liability = account.liability.as_ref();
        let statement_balance = liability
            .and_then(|liability| liability.last_statement_balance)
            .map(minor_units);
        let charged = liability.and_then(|liability| {
            let charges = liability
                .aprs
                .iter()
                .filter_map(|apr| apr.interest_charged)
                .map(minor_units)
                .collect::<Vec<_>>();
            (!charges.is_empty()).then(|| charges.iter().sum::<i64>())
        });
        let issuer_close = spending_card
            .filter(|(id, _)| *id == account.id)
            .and_then(|(_, cycle)| cycle.next_close)
            .filter(|close| *close >= today);
        let estimated_close = liability
            .and_then(|liability| liability.last_statement_date.as_deref())
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            .map(|last| next_monthly_close(last, today));
        if let Some(card_limit) = card_limit {
            balance += owed;
            limit += card_limit;
        } else {
            credit.cards_without_limit += 1;
        }
        if let Some(charged) = charged {
            interest = Some(interest.unwrap_or(0) + charged);
        }
        credit.cards.push(CardCredit {
            account_id: account.id.clone(),
            balance: from_minor(owed),
            limit: card_limit.map(from_minor),
            available: account.available_balance,
            utilization: card_limit.and_then(|limit| percent(owed, limit)),
            statement_utilization: card_limit
                .zip(statement_balance)
                .and_then(|(limit, statement)| percent(statement.max(0), limit)),
            closes_on: issuer_close.or(estimated_close),
            closes_estimated: issuer_close.is_none(),
            interest_charged: charged.map(from_minor),
        });
    }
    credit.balance = from_minor(balance);
    credit.limit = from_minor(limit);
    credit.utilization = percent(balance, limit);
    credit.interest_charged = interest.map(from_minor);
    credit
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn card(id: &str, value: f64, limit: Option<f64>, statement: Option<&str>) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": "credit", "value": value,
            "currency": "USD", "creditLimit": limit,
            "liability": statement.map(|date| json!({
                "lastStatementDate": date, "lastStatementBalance": 400.0, "minimumPayment": 35.0,
                "nextPaymentDueDate": null, "lastPaymentDate": null, "lastPaymentAmount": null,
                "purchaseApr": 24.99, "isOverdue": false,
                "aprs": [
                    { "type": "purchase_apr", "percentage": 24.99, "balanceSubjectToApr": 300.0, "interestCharged": 6.16 },
                    { "type": "cash_apr", "percentage": 29.99, "balanceSubjectToApr": 50.0, "interestCharged": 1.23 }
                ]
            })),
        }))
        .unwrap()
    }

    #[test]
    fn measures_utilization_per_card_and_only_across_known_limits() {
        let accounts = [
            card("plaid:a", -600.0, Some(2_000.0), Some("2026-08-31")),
            card("plaid:b", -100.0, Some(8_000.0), None),
            // A card paid past zero owes nothing; one without a limit stays out of the total.
            card("plaid:c", 25.0, Some(1_000.0), None),
            card("plaid:d", -900.0, None, None),
        ];
        let result = credit(&accounts, None, day("2026-09-20"));
        assert_eq!(result.balance, 700.0);
        assert_eq!(result.limit, 11_000.0);
        assert_eq!(result.utilization, Some(6.36));
        assert_eq!(result.cards_without_limit, 1);
        assert_eq!(result.interest_charged, Some(7.39));
        let first = &result.cards[0];
        assert_eq!(first.utilization, Some(30.0));
        assert_eq!(first.statement_utilization, Some(20.0));
        // August 31 closes on September 30, the last day of the month.
        assert_eq!(first.closes_on, Some(day("2026-09-30")));
        assert!(first.closes_estimated);
        assert_eq!(result.cards[2].balance, 0.0);
        assert_eq!(result.cards[3].utilization, None);
    }

    #[test]
    fn steps_whole_months_past_a_stale_statement() {
        assert_eq!(
            next_monthly_close(day("2026-05-15"), day("2026-09-16")),
            day("2026-10-15")
        );
        assert_eq!(
            next_monthly_close(day("2026-01-31"), day("2026-02-10")),
            day("2026-02-28")
        );
        assert_eq!(
            next_monthly_close(day("2026-02-28"), day("2026-03-01")),
            day("2026-03-31")
        );
    }
}
