//! What happened over each chart period (the trailing week, month, three months, year, and all
//! saved history): spending, income, and net, each against the period of the same length before
//! it, and the categories that drove spending. Amounts are summed in whole cents.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::calendar::{self};
use super::changes::{Window, WINDOWS};
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units};
use crate::transaction_policy::Kind;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategoryChange {
    pub category: String,
    pub spent: f64,
    pub previous: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct PeriodRecap {
    pub window: Window,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Saved activity starts after `start`, so the period is only partly covered.
    pub partial: bool,
    /// Purchases less reimbursements.
    pub spending: f64,
    pub income: f64,
    /// Income less spending, fees, and taxes.
    pub net: f64,
    /// Each measure's change from the period before, in percent of the earlier figure's size.
    /// Unavailable when history starts inside the period before, or its figure was zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spending_change_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub income_change_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub net_change_pct: Option<f64>,
    /// The largest categories, each beside the period before.
    pub categories: Vec<CategoryChange>,
}

const LISTED: usize = 3;

#[derive(Default)]
struct Totals {
    categories: BTreeMap<String, i64>,
    income: i64,
    /// Income other than dividends and interest: what work paid.
    earned: i64,
    fees_and_taxes: i64,
}

impl Totals {
    fn add_parts(&mut self, transaction: &Transaction) {
        for (category, amount) in transaction.parts() {
            *self.categories.entry(category.to_owned()).or_default() -= minor_units(amount);
        }
    }

    fn spending(&self) -> i64 {
        self.categories.values().sum::<i64>().max(0)
    }

    fn net(&self) -> i64 {
        self.income - self.spending() - self.fees_and_taxes
    }
}

/// Percent change with two decimals, from whole cents. Measured against the earlier figure's
/// size, so a net that rises from a loss reads as an increase.
fn change_pct(current: i64, previous: i64) -> Option<f64> {
    (previous != 0)
        .then(|| ((current - previous) as f64 * 10_000.0 / previous.abs() as f64).round() / 100.0)
}

fn totals<'a>(posted: impl Iterator<Item = &'a Transaction>) -> Totals {
    let mut totals = Totals::default();
    for transaction in posted {
        let amount = minor_units(transaction.amount);
        match transaction.kind() {
            // Reimbursements reduce the spending they repay, never below zero in total.
            Some(Kind::Expense) if amount < 0 => totals.add_parts(transaction),
            Some(Kind::Reimbursement) if amount > 0 => totals.add_parts(transaction),
            Some(Kind::Income) if amount > 0 => {
                totals.income += amount;
                totals.earned += amount;
            }
            Some(Kind::Dividend | Kind::Interest) if amount > 0 => totals.income += amount,
            Some(Kind::Fee | Kind::Tax) => totals.fees_and_taxes -= amount,
            _ => {}
        }
    }
    totals
}

/// Spending and earned income, in cents, posted from `from` through `through`, counted as
/// every period here counts them.
pub(super) fn spent_and_earned(
    posted: &[(NaiveDate, &Transaction)],
    from: NaiveDate,
    through: NaiveDate,
) -> (i64, i64) {
    let totals = totals(
        posted
            .iter()
            .filter(|(date, _)| *date >= from && *date <= through)
            .map(|(_, transaction)| *transaction),
    );
    (totals.spending(), totals.earned)
}

fn period(
    posted: &[(NaiveDate, &Transaction)],
    window: Window,
    start: NaiveDate,
    end: NaiveDate,
    previous_start: Option<NaiveDate>,
) -> Option<PeriodRecap> {
    let history_start = posted.first()?.0;
    let within = |from: NaiveDate, through: NaiveDate| {
        posted
            .iter()
            .filter(move |(date, _)| *date >= from && *date <= through)
            .map(|(_, transaction)| *transaction)
    };
    let current = totals(within(start, end));
    let previous = previous_start
        .filter(|previous| history_start <= *previous)
        .map(|previous| totals(within(previous, start - Duration::days(1))));

    let mut categories = current
        .categories
        .iter()
        .filter(|(_, spent)| **spent > 0)
        .map(|(category, spent)| CategoryChange {
            category: category.clone(),
            spent: from_minor(*spent),
            previous: from_minor(
                previous
                    .as_ref()
                    .and_then(|previous| previous.categories.get(category))
                    .copied()
                    .unwrap_or(0)
                    .max(0),
            ),
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        right
            .spent
            .total_cmp(&left.spent)
            .then_with(|| left.category.cmp(&right.category))
    });
    categories.truncate(LISTED);
    let spending = current.spending();
    Some(PeriodRecap {
        window,
        start,
        end,
        partial: history_start > start,
        spending: from_minor(spending),
        income: from_minor(current.income),
        net: from_minor(current.net()),
        spending_change_pct: previous
            .as_ref()
            .and_then(|previous| change_pct(spending, previous.spending())),
        income_change_pct: previous
            .as_ref()
            .and_then(|previous| change_pct(current.income, previous.income)),
        net_change_pct: previous
            .as_ref()
            .and_then(|previous| change_pct(current.net(), previous.net())),
        categories,
    })
}

/// A recap for each chart period that has any posted activity.
pub fn recap(transactions: &[Transaction], today: NaiveDate) -> Vec<PeriodRecap> {
    let posted = calendar::posted_through(transactions, today);
    let Some(first) = posted.first().map(|(date, _)| *date) else {
        return Vec::new();
    };
    WINDOWS
        .iter()
        .filter_map(|(window, days)| match days {
            Some(days) => {
                let start = today - Duration::days(days - 1);
                period(
                    &posted,
                    *window,
                    start,
                    today,
                    Some(start - Duration::days(*days)),
                )
            }
            None => period(&posted, *window, first, today, None),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    use crate::transaction_policy::{Classification, Mark};

    fn txn(id: &str, date: &str, category: &str, amount: f64, kind: Kind) -> Transaction {
        let mut value = json!({
            "id": id, "merchant": "Example", "category": category, "date": date,
            "amount": amount, "account": "Checking", "accountId": "plaid:checking",
            "pending": false,
        });
        value["classification"] = serde_json::to_value(Classification {
            mark: Mark::Initial,
            spending: kind == Kind::Expense,
            kind,
            credit: false,
            zelle: false,
            others_spending: false,
        })
        .unwrap();
        serde_json::from_value(value).unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn each_period_is_compared_with_the_one_of_the_same_length_before_it() {
        let transactions = vec![
            txn("old", "2026-05-02", "Dining", -10.0, Kind::Expense),
            txn("prior-week", "2026-09-05", "Dining", -40.0, Kind::Expense),
            txn("rent", "2026-09-10", "Rent", -1_500.0, Kind::Expense),
            txn("food", "2026-09-12", "Dining", -60.0, Kind::Expense),
            txn("back", "2026-09-13", "Dining", 20.0, Kind::Reimbursement),
            txn("pay", "2026-09-15", "Income", 4_000.0, Kind::Income),
            txn("fee", "2026-09-16", "Fees", -12.0, Kind::Fee),
            txn("move", "2026-09-16", "Transfer", -900.0, Kind::Transfer),
        ];
        let result = recap(&transactions, day("2026-09-16"));
        let get = |window: Window| {
            result
                .iter()
                .find(|period| period.window == window)
                .unwrap()
        };

        // The week is Sep 10–16; the week before it is Sep 3–9.
        let week = get(Window::Week);
        assert_eq!(
            (week.start, week.end),
            (day("2026-09-10"), day("2026-09-16"))
        );
        assert!(!week.partial);
        assert_eq!(week.spending, 1_540.0);
        assert_eq!(week.income, 4_000.0);
        assert_eq!(week.net, 2_448.0);
        // The week before spent $40 with no income: spending rose 3,750%, income has no base,
        // and net rose from a $40 loss to a $2,448 gain, 6,220% of the loss's size.
        assert_eq!(week.spending_change_pct, Some(3_750.0));
        assert_eq!(week.income_change_pct, None);
        assert_eq!(week.net_change_pct, Some(6_220.0));
        assert_eq!(
            week.categories,
            vec![
                CategoryChange {
                    category: "Rent".into(),
                    spent: 1_500.0,
                    previous: 0.0
                },
                CategoryChange {
                    category: "Dining".into(),
                    spent: 40.0,
                    previous: 40.0
                },
            ]
        );

        // A year reaches back before the first activity, so it is partial with no prior.
        let year = get(Window::Year);
        assert!(year.partial);
        assert_eq!(year.spending_change_pct, None);
        assert_eq!(year.spending, 1_590.0);

        // All history starts at the first activity and has nothing to compare against.
        let all = get(Window::All);
        assert_eq!(all.start, day("2026-05-02"));
        assert!(!all.partial);
        assert_eq!(all.net_change_pct, None);
    }

    #[test]
    fn no_activity_has_no_recap() {
        assert!(recap(&[], day("2026-09-16")).is_empty());
    }
}
