//! Cash ahead: known checking and savings balances carried forward day by day through the
//! recurring bills and income expected to reach those accounts. Each day has an expected
//! balance and a range from the smallest and largest amounts each series has posted, so the
//! line is an estimate that says how wide it is. Amounts are summed in whole cents.
//!
//! Each account starts from its available balance when the bank reports one, so pending
//! holds that no forecast predicts already count. A pending bill the forecast matched is then
//! in that balance, so its next occurrence isn't subtracted again.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::forecast::{EventState, ExpectedEvent};
use crate::finance_contract::Account;
use crate::numeric::{from_minor, minor_units};

pub const HORIZON_DAYS: i64 = 90;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CashDay {
    pub date: NaiveDate,
    pub expected: f64,
    /// The balance if every bill is its largest and every deposit its smallest.
    pub low: f64,
    /// The balance if every bill is its smallest and every deposit its largest.
    pub high: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CashEvent {
    pub date: NaiveDate,
    pub recurring_id: String,
    pub title: String,
    pub amount: f64,
    /// False for occurrences after the next one, which repeat the series' cadence.
    pub next: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CashTimeline {
    pub horizon_days: i64,
    /// Known USD cash available today. The line is unavailable while any cash balance is
    /// unknown.
    pub cash: f64,
    /// Available minus ledger balance across the accounts that report one: negative when
    /// pending charges hold cash, positive when pending deposits add to it.
    pub pending_change: f64,
    pub cash_incomplete: bool,
    /// One entry for today and for every later day the balance changes.
    pub days: Vec<CashDay>,
    pub events: Vec<CashEvent>,
    /// The day the expected balance is lowest, when it falls below today's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lowest: Option<CashDay>,
}

/// `events` is the forecast over `HORIZON_DAYS`, including each series' later occurrences.
pub fn timeline(accounts: &[Account], events: &[ExpectedEvent], today: NaiveDate) -> CashTimeline {
    let cash_accounts = accounts
        .iter()
        .filter(|account| account.r#type == "cash")
        .collect::<Vec<_>>();
    let usd = |account: &&&Account| account.currency.as_deref().is_none_or(|code| code == "USD");
    let start = |account: &Account| account.available_balance.or(account.value);
    let cash = cash_accounts
        .iter()
        .filter(usd)
        .filter_map(|account| start(account))
        .map(minor_units)
        .sum::<i64>();
    let pending_change = cash_accounts
        .iter()
        .filter(usd)
        .filter_map(|account| {
            Some(minor_units(account.available_balance?) - minor_units(account.value?))
        })
        .sum::<i64>();
    let cash_incomplete = cash_accounts.is_empty()
        || cash_accounts
            .iter()
            .any(|account| account.value.is_none() || !usd(&account));
    let mut timeline = CashTimeline {
        horizon_days: HORIZON_DAYS,
        cash: from_minor(cash),
        pending_change: from_minor(pending_change),
        cash_incomplete,
        days: Vec::new(),
        events: Vec::new(),
        lowest: None,
    };
    if cash_incomplete {
        return timeline;
    }

    let horizon = today + Duration::days(HORIZON_DAYS);
    // Per day: the expected, lowest, and highest change in cents.
    let mut changes = BTreeMap::<NaiveDate, (i64, i64, i64)>::new();
    for event in events.iter().filter(|event| {
        event.account_id.as_deref().is_some_and(|id| {
            cash_accounts
                .iter()
                .any(|account| account.id == id && usd(&account))
        })
    }) {
        let amount = minor_units(event.amount);
        let (small, large) = (
            minor_units(event.amount_low).abs(),
            minor_units(event.amount_high).abs(),
        );
        let range = if amount < 0 {
            (-large.max(-amount), -small.min(-amount))
        } else {
            (small.min(amount), large.max(amount))
        };
        // A pending match already left the available balance it started from.
        let held = event.state == EventState::Pending
            && cash_accounts.iter().any(|account| {
                Some(account.id.as_str()) == event.account_id.as_deref()
                    && account.available_balance.is_some()
            });
        // An occurrence already overdue is still to come, so it lands today.
        let occurrences = std::iter::once((event.date.max(today), true))
            .filter(|_| !held)
            .chain(event.later.iter().map(|date| (*date, false)));
        for (date, next) in occurrences.filter(|(date, _)| *date <= horizon) {
            // A pending match is the actual amount; only estimates carry a range.
            let (low, high) = if next && event.state == EventState::Pending {
                (amount, amount)
            } else {
                range
            };
            let day = changes.entry(date).or_default();
            day.0 += amount;
            day.1 += low;
            day.2 += high;
            timeline.events.push(CashEvent {
                date,
                recurring_id: event.recurring_id.clone(),
                title: event.title.clone(),
                amount: from_minor(amount),
                next,
            });
        }
    }
    timeline.events.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then(left.title.cmp(&right.title))
    });

    let (mut expected, mut low, mut high) = (cash, cash, cash);
    let day = |date, expected, low, high| CashDay {
        date,
        expected: from_minor(expected),
        low: from_minor(low),
        high: from_minor(high),
    };
    if !changes.contains_key(&today) {
        timeline.days.push(day(today, cash, cash, cash));
    }
    for (date, change) in changes {
        expected += change.0;
        low += change.1;
        high += change.2;
        timeline.days.push(day(date, expected, low, high));
    }
    timeline.lowest = timeline
        .days
        .iter()
        .filter(|day| day.expected < timeline.cash)
        .min_by(|left, right| left.expected.total_cmp(&right.expected))
        .cloned();
    timeline
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::insights::forecast::EventKind;

    fn account(id: &str, kind: &str, value: Option<f64>, currency: &str) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": kind, "value": value,
            "currency": currency,
        }))
        .unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn event(
        id: &str,
        account: &str,
        date: &str,
        amount: f64,
        low: f64,
        high: f64,
    ) -> ExpectedEvent {
        ExpectedEvent {
            id: id.into(),
            recurring_id: id.into(),
            title: id.into(),
            kind: EventKind::Payment,
            date: day(date),
            amount,
            amount_low: low,
            amount_high: high,
            frequency: "Monthly",
            observations: 4,
            last_seen: day("2026-08-20"),
            state: EventState::Expected,
            account_id: Some(account.into()),
            matched_transaction_id: None,
            source_transaction_id: id.into(),
            later: Vec::new(),
            per_year: 12,
        }
    }

    #[test]
    fn balances_follow_expected_activity_with_a_range_for_estimates() {
        let accounts = [
            account("plaid:checking", "cash", Some(2_000.0), "USD"),
            account("plaid:card", "credit", Some(-500.0), "USD"),
        ];
        let mut rent = event(
            "rent",
            "plaid:checking",
            "2026-09-20",
            -1_500.0,
            1_500.0,
            1_500.0,
        );
        rent.later = vec![day("2026-10-20"), day("2026-11-20")];
        let power = event("power", "plaid:checking", "2026-09-20", -80.0, 60.0, 140.0);
        let pay = event(
            "pay",
            "plaid:checking",
            "2026-09-25",
            2_400.0,
            2_300.0,
            2_500.0,
        );
        // A card subscription never touches cash, and an overdue bill is still to come.
        let stream = event("stream", "plaid:card", "2026-09-18", -15.0, 15.0, 15.0);
        let late = event("late", "plaid:checking", "2026-09-14", -100.0, 100.0, 100.0);
        let result = timeline(
            &accounts,
            &[rent, power, pay, stream, late],
            day("2026-09-16"),
        );

        assert_eq!(result.cash, 2_000.0);
        assert_eq!(
            result.days,
            vec![
                CashDay {
                    date: day("2026-09-16"),
                    expected: 1_900.0,
                    low: 1_900.0,
                    high: 1_900.0
                },
                CashDay {
                    date: day("2026-09-20"),
                    expected: 320.0,
                    low: 260.0,
                    high: 340.0
                },
                CashDay {
                    date: day("2026-09-25"),
                    expected: 2_720.0,
                    low: 2_560.0,
                    high: 2_840.0
                },
                CashDay {
                    date: day("2026-10-20"),
                    expected: 1_220.0,
                    low: 1_060.0,
                    high: 1_340.0
                },
                CashDay {
                    date: day("2026-11-20"),
                    expected: -280.0,
                    low: -440.0,
                    high: -160.0
                },
            ]
        );
        assert_eq!(result.lowest.unwrap().date, day("2026-11-20"));
        assert_eq!(result.events.len(), 6);
        assert!(result.events.iter().all(|event| event.title != "stream"));
        assert_eq!(result.events.iter().filter(|event| !event.next).count(), 2);
    }

    #[test]
    fn a_pending_match_is_exact_and_later_estimates_keep_their_range() {
        let accounts = [account("plaid:checking", "cash", Some(1_000.0), "USD")];
        let mut bill = event("bill", "plaid:checking", "2026-09-18", -90.0, 60.0, 140.0);
        bill.state = EventState::Pending;
        bill.later = vec![day("2026-10-18")];
        let result = timeline(&accounts, &[bill], day("2026-09-16"));
        assert_eq!(
            result.days[1],
            CashDay {
                date: day("2026-09-18"),
                expected: 910.0,
                low: 910.0,
                high: 910.0
            }
        );
        assert_eq!(
            result.days[2],
            CashDay {
                date: day("2026-10-18"),
                expected: 820.0,
                low: 770.0,
                high: 850.0
            }
        );
    }

    #[test]
    fn available_balances_start_the_line_without_counting_a_pending_bill_twice() {
        let mut checking = account("plaid:checking", "cash", Some(1_000.0), "USD");
        // $90 of the bill is already held, plus a $40 hold no forecast predicts.
        checking.available_balance = Some(870.0);
        let savings = account("plaid:savings", "cash", Some(500.0), "USD");
        let mut bill = event("bill", "plaid:checking", "2026-09-18", -90.0, 60.0, 140.0);
        bill.state = EventState::Pending;
        bill.later = vec![day("2026-10-18")];
        let result = timeline(&[checking, savings], &[bill], day("2026-09-16"));
        assert_eq!(result.cash, 1_370.0);
        assert_eq!(result.pending_change, -130.0);
        assert_eq!(
            result.days,
            vec![
                CashDay {
                    date: day("2026-09-16"),
                    expected: 1_370.0,
                    low: 1_370.0,
                    high: 1_370.0
                },
                CashDay {
                    date: day("2026-10-18"),
                    expected: 1_280.0,
                    low: 1_230.0,
                    high: 1_310.0
                },
            ]
        );
        assert_eq!(result.events.len(), 1);
    }

    #[test]
    fn unknown_or_foreign_cash_makes_the_line_unavailable() {
        let bill = event("bill", "plaid:checking", "2026-09-18", -90.0, 90.0, 90.0);
        for accounts in [
            vec![account("plaid:checking", "cash", None, "USD")],
            vec![
                account("plaid:checking", "cash", Some(500.0), "USD"),
                account("plaid:euro", "cash", Some(500.0), "EUR"),
            ],
            vec![account("plaid:card", "credit", Some(-50.0), "USD")],
        ] {
            let result = timeline(&accounts, std::slice::from_ref(&bill), day("2026-09-16"));
            assert!(result.cash_incomplete);
            assert!(result.days.is_empty() && result.events.is_empty() && result.lowest.is_none());
        }
    }
}
