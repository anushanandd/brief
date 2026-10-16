//! Spending's periods and the judgments made about them. Spending covers the saved card, every
//! USD account with spending, or one of them, over a statement, a rolling week, or a calendar
//! month, quarter, or year, counting your spending, someone else's, or both. The renderer sums each period's activity; this module decides what a
//! statement opened at, where the current period is heading, and what is usual by this point.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use super::calendar::{self, Zone};
use super::statement::StatementCycle;
use crate::finance_contract::{Account, Transaction};
use crate::numeric::{js_round, minor_units, round2};

/// Earlier periods averaged for what is usual by this point.
const TYPICAL_PERIODS: usize = 3;
/// The pace is projected only after a week of the period, so one purchase can't dominate it.
const PROJECTION_DAYS: i64 = 7;
/// What Spending covers when the user picks every account.
pub const ALL_ACCOUNTS: &str = "all";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PeriodKind {
    Statement,
    Week,
    Month,
    Quarter,
    Year,
}

impl PeriodKind {
    fn months(self) -> Option<i64> {
        match self {
            Self::Month => Some(1),
            Self::Quarter => Some(3),
            Self::Year => Some(12),
            Self::Statement | Self::Week => None,
        }
    }
}

/// Where a period's spending is heading at its pace so far.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub total: f64,
    /// The same for each category with spending so far.
    pub categories: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpendingPeriod {
    pub kind: PeriodKind,
    pub start: NaiveDate,
    /// Usual spending by this point in the period, by category: the average of up to three
    /// earlier periods cut at the same day, when the saved history covers at least two.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical: Option<BTreeMap<String, f64>>,
    /// The current period only, once a week of it has passed with days still left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected: Option<Projection>,
    /// The saved card's statement balance when the period opened: today's balance less this
    /// cycle's activity, or for a closed cycle the autopay that paid the statement before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening: Option<f64>,
}

/// Whose spending a scope counts: yours, purchases made for someone else, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    Me,
    Other,
    All,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpendingScope {
    /// An account ID, or `all`.
    pub scope: String,
    pub owner: Owner,
    /// Newest first within each kind.
    pub periods: Vec<SpendingPeriod>,
}

/// One scope's activity, each transaction dated as the snapshot shows it.
struct Activity<'a> {
    dated: Vec<(NaiveDate, &'a Transaction)>,
    first: Option<NaiveDate>,
    owner: Owner,
}

impl<'a> Activity<'a> {
    fn new(transactions: Vec<&'a Transaction>, owner: Owner) -> Self {
        let dated: Vec<_> = transactions
            .into_iter()
            .filter_map(|transaction| {
                Some((
                    calendar::transaction_date(transaction.posted_date())?,
                    transaction,
                ))
            })
            .collect();
        let first = dated.iter().map(|(date, _)| *date).min();
        Self {
            dated,
            first,
            owner,
        }
    }

    fn spending_in(
        &self,
        from: NaiveDate,
        through: NaiveDate,
    ) -> impl Iterator<Item = &'a Transaction> + '_ {
        self.dated
            .iter()
            .filter(move |(date, transaction)| {
                *date >= from && *date <= through && spending_for(transaction, self.owner)
            })
            .map(|(_, transaction)| *transaction)
    }

    /// Spending by category in whole cents, each split part under its own category.
    fn by_category(&self, windows: &[(NaiveDate, NaiveDate)]) -> BTreeMap<String, i64> {
        let mut sums = BTreeMap::<String, i64>::new();
        for &(from, through) in windows {
            for transaction in self.spending_in(from, through) {
                for (category, amount) in transaction.parts() {
                    *sums.entry(category.to_owned()).or_default() += minor_units(amount.abs());
                }
            }
        }
        sums
    }
}

fn spending(transaction: &Transaction) -> bool {
    spending_for(transaction, Owner::Me)
}

fn spending_for(transaction: &Transaction, owner: Owner) -> bool {
    transaction
        .classification
        .as_ref()
        .is_some_and(|classification| match owner {
            Owner::Me => classification.spending,
            Owner::Other => classification.others_spending,
            Owner::All => classification.spending || classification.others_spending,
        })
}

/// The windows of one kind: how a period starts and ends, and the one before it.
struct Calendar<'a> {
    kind: PeriodKind,
    /// Statement closing dates, for the saved card.
    boundaries: &'a [NaiveDate],
}

impl Calendar<'_> {
    /// The day after the last statement close before `end`; a close on `end` ends this cycle.
    fn statement_start(&self, end: NaiveDate) -> Option<NaiveDate> {
        let closed: Vec<NaiveDate> = self
            .boundaries
            .iter()
            .copied()
            .filter(|date| *date <= end)
            .collect();
        let ends_on_close = closed.last() == Some(&end);
        let index = closed.len().checked_sub(1 + usize::from(ends_on_close))?;
        closed.get(index)?.succ_opt()
    }

    fn start(&self, end: NaiveDate) -> Option<NaiveDate> {
        match self.kind {
            PeriodKind::Statement => self.statement_start(end),
            PeriodKind::Week => Some(end - Duration::days(6)),
            PeriodKind::Month => Some(calendar::add_months_first(end, 0)),
            PeriodKind::Quarter => Some(calendar::first_of_month(
                end.year(),
                i64::from(end.month0() / 3 * 3 + 1),
            )),
            PeriodKind::Year => Some(calendar::first_of_month(end.year(), 1)),
        }
    }

    /// The start of the comparable period before one that starts on `from`.
    fn previous_start(&self, from: NaiveDate) -> NaiveDate {
        match self.kind {
            PeriodKind::Week => from - Duration::days(7),
            PeriodKind::Statement => from
                .pred_opt()
                .and_then(|day| self.statement_start(day))
                .unwrap_or_else(|| calendar::add_months_first(from, -1)),
            kind => calendar::add_months_first(from, -kind.months().unwrap_or(1)),
        }
    }

    /// The whole period, so a pace can say how much of it is left.
    fn last_day(&self, start: NaiveDate, end: NaiveDate) -> NaiveDate {
        match self.kind {
            PeriodKind::Week => end,
            PeriodKind::Statement => self
                .boundaries
                .iter()
                .copied()
                .find(|date| *date >= end)
                .unwrap_or(end),
            kind => {
                calendar::add_months_first(start, kind.months().unwrap_or(1)) - Duration::days(1)
            }
        }
    }

    /// The end of each period, newest first: today, then each earlier period's last day.
    fn ends(&self, today: NaiveDate, earliest: NaiveDate) -> Vec<NaiveDate> {
        let mut ends = vec![today];
        match self.kind {
            PeriodKind::Statement => {
                let closed: Vec<NaiveDate> = self
                    .boundaries
                    .iter()
                    .copied()
                    .filter(|date| *date <= today)
                    .collect();
                let skip = usize::from(closed.last() == Some(&today));
                ends.extend(closed.iter().rev().skip(skip));
            }
            PeriodKind::Week => {
                let mut end = today - Duration::days(7);
                while end >= earliest {
                    ends.push(end);
                    end -= Duration::days(7);
                }
            }
            kind => {
                let months = kind.months().unwrap_or(1);
                let mut start = self.start(today).unwrap_or(today);
                loop {
                    let end = start - Duration::days(1);
                    if end < earliest {
                        break;
                    }
                    ends.push(end);
                    start = calendar::add_months_first(start, -months);
                }
            }
        }
        ends
    }
}

/// What is usual by this point: up to three earlier periods, each cut at the same day count,
/// averaged by category. Only periods the saved history fully covers count, and at least two.
fn typical(
    calendar: &Calendar,
    activity: &Activity,
    start: NaiveDate,
    end: NaiveDate,
) -> Option<BTreeMap<String, f64>> {
    let elapsed = (end - start).num_days() + 1;
    let history = activity.first.map_or(end, |first| first.min(end));
    let mut windows = Vec::new();
    let mut next = start;
    while windows.len() < TYPICAL_PERIODS {
        let from = calendar.previous_start(next);
        if from >= next || from < history {
            break;
        }
        let through = (from + Duration::days(elapsed - 1)).min(next - Duration::days(1));
        windows.push((from, through));
        next = from;
    }
    (windows.len() >= 2).then(|| {
        activity
            .by_category(&windows)
            .into_iter()
            .map(|(category, cents)| {
                (
                    category,
                    js_round(cents as f64 / windows.len() as f64) / 100.0,
                )
            })
            .collect()
    })
}

/// The current period's spending carried to its end at the daily pace so far.
fn projected(
    calendar: &Calendar,
    activity: &Activity,
    start: NaiveDate,
    end: NaiveDate,
) -> Option<Projection> {
    let elapsed = (end - start).num_days() + 1;
    let remaining = ((calendar.last_day(start, end) - start).num_days() + 1 - elapsed).max(0);
    if remaining == 0 || elapsed < PROJECTION_DAYS {
        return None;
    }
    let at_pace = |cents: i64| {
        let spent = cents as f64 / 100.0;
        round2(spent + spent / elapsed as f64 * remaining as f64)
    };
    let total = activity
        .spending_in(start, end)
        .map(|transaction| minor_units(transaction.amount.abs()))
        .sum();
    Some(Projection {
        total: at_pace(total),
        categories: activity
            .by_category(&[(start, end)])
            .into_iter()
            .map(|(category, cents)| (category, at_pace(cents)))
            .collect(),
    })
}

/// What the saved card's statement opened at. Purchases add to the balance and credits and
/// payments take from it; anything else leaves it.
fn opening(
    activity: &Activity,
    statement: &StatementCycle,
    balance: Option<f64>,
    start: NaiveDate,
    end: NaiveDate,
) -> f64 {
    if let Some(balance) = balance {
        let net: i64 = activity
            .dated
            .iter()
            .filter(|(date, _)| *date >= start && *date <= end)
            .map(|(_, transaction)| {
                if spending_for(transaction, activity.owner) {
                    minor_units(transaction.amount.abs())
                } else if transaction.amount > 0.0 {
                    -minor_units(transaction.amount)
                } else {
                    0
                }
            })
            .sum();
        return (minor_units(balance) - net) as f64 / 100.0;
    }
    statement
        .autopays
        .iter()
        .find(|autopay| autopay.date >= start && autopay.date <= end)
        .map_or(0.0, |autopay| autopay.amount)
}

/// Every scope Spending offers, with its periods back to the start of its history.
pub fn scopes(
    transactions: &[Transaction],
    accounts: &[Account],
    card: Option<&Account>,
    statement: Option<&StatementCycle>,
    updated_at: &str,
    zone: &dyn Zone,
) -> Vec<SpendingScope> {
    let Some(today) = calendar::calendar_date(updated_at, zone) else {
        return Vec::new();
    };
    let spenders: BTreeSet<&str> = transactions
        .iter()
        .filter(|transaction| spending(transaction))
        .filter_map(|transaction| transaction.account_id.as_deref())
        .collect();
    let options: Vec<&Account> = accounts
        .iter()
        .filter(|account| {
            account.id != ALL_ACCOUNTS
                && account.currency.as_deref().is_none_or(|code| code == "USD")
                && (spenders.contains(account.id.as_str())
                    || card.is_some_and(|card| card.id == account.id))
        })
        .collect();
    let ids: BTreeSet<&str> = options.iter().map(|account| account.id.as_str()).collect();
    // Someone else's spending and everyone's are offered only once a purchase is theirs.
    let owners: &[Owner] = if transactions.iter().any(|transaction| {
        transaction
            .classification
            .as_ref()
            .is_some_and(|classification| classification.others_spending)
    }) {
        &[Owner::Me, Owner::Other, Owner::All]
    } else {
        &[Owner::Me]
    };
    options
        .iter()
        .map(|account| account.id.as_str())
        .chain([ALL_ACCOUNTS])
        .flat_map(|scope| owners.iter().map(move |&owner| (scope, owner)))
        .map(|(scope, owner)| {
            let in_scope = |transaction: &&Transaction| match transaction.account_id.as_deref() {
                Some(id) if scope == ALL_ACCOUNTS => ids.contains(id),
                id => id == Some(scope),
            };
            let activity = Activity::new(transactions.iter().filter(in_scope).collect(), owner);
            let on_card = card.filter(|card| card.id == scope);
            SpendingScope {
                scope: scope.to_owned(),
                owner,
                periods: periods(
                    &activity,
                    on_card,
                    statement.filter(|_| on_card.is_some()),
                    today,
                ),
            }
        })
        .collect()
}

fn periods(
    activity: &Activity,
    card: Option<&Account>,
    statement: Option<&StatementCycle>,
    today: NaiveDate,
) -> Vec<SpendingPeriod> {
    let earliest = activity.first.unwrap_or(today);
    let kinds = statement
        .filter(|statement| !statement.boundaries.is_empty())
        .map(|_| PeriodKind::Statement)
        .into_iter()
        .chain([
            PeriodKind::Week,
            PeriodKind::Month,
            PeriodKind::Quarter,
            PeriodKind::Year,
        ]);
    let balance = card
        .filter(|card| card.r#type == "credit")
        .and_then(|card| card.value)
        .map(f64::abs);
    let mut periods = Vec::new();
    for kind in kinds {
        let calendar = Calendar {
            kind,
            boundaries: statement.map_or(&[][..], |statement| &statement.boundaries),
        };
        for (index, end) in calendar.ends(today, earliest).into_iter().enumerate() {
            let Some(start) = calendar.start(end) else {
                continue;
            };
            let current = index == 0;
            periods.push(SpendingPeriod {
                kind,
                start,
                typical: typical(&calendar, activity, start, end),
                projected: current
                    .then(|| projected(&calendar, activity, start, end))
                    .flatten(),
                // A statement balance is never only someone else's.
                opening: statement
                    .filter(|_| kind == PeriodKind::Statement && activity.owner != Owner::Other)
                    .map(|statement| {
                        opening(activity, statement, balance.filter(|_| current), start, end)
                    }),
            });
        }
    }
    periods
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::insights::calendar::NamedZone;
    use crate::insights::statement::{Autopay, StatementSource};

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn transaction(id: &str, date: &str, amount: f64, category: &str) -> Transaction {
        let spending = amount < 0.0 && category != "Payment";
        serde_json::from_value(json!({
            "id": id, "merchant": id, "category": category, "date": date, "amount": amount,
            "account": "Card", "accountId": "card", "pending": false, "description": null,
            "classification": {
                "mark": if spending { "initial" } else { "payment" },
                "kind": if spending { "expense" } else { "transfer" },
                "spending": spending, "zelle": false,
            },
        }))
        .unwrap()
    }

    fn card(value: Option<f64>) -> Account {
        serde_json::from_value(json!({
            "id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": value,
        }))
        .unwrap()
    }

    fn statement() -> StatementCycle {
        StatementCycle {
            source: StatementSource::Autopay,
            boundaries: [
                "2026-06-22",
                "2026-07-23",
                "2026-08-23",
                "2026-09-23",
                "2026-10-23",
            ]
            .map(day)
            .to_vec(),
            autopays: [
                ("2026-07-07", 90.0),
                ("2026-08-07", 100.0),
                ("2026-09-07", 110.0),
            ]
            .map(|(date, amount)| Autopay {
                date: day(date),
                amount,
            })
            .to_vec(),
            last_close: None,
            next_close: None,
            liability: None,
        }
    }

    fn period<'a>(
        scopes: &'a [SpendingScope],
        scope: &str,
        kind: PeriodKind,
        start: &str,
    ) -> &'a SpendingPeriod {
        owned_period(scopes, scope, Owner::Me, kind, start)
    }

    fn owned_period<'a>(
        scopes: &'a [SpendingScope],
        scope: &str,
        owner: Owner,
        kind: PeriodKind,
        start: &str,
    ) -> &'a SpendingPeriod {
        scopes
            .iter()
            .find(|entry| entry.scope == scope && entry.owner == owner)
            .unwrap()
            .periods
            .iter()
            .find(|period| period.kind == kind && period.start == day(start))
            .unwrap()
    }

    #[test]
    fn statements_open_at_the_balance_less_activity_or_the_autopay_that_paid_the_last() {
        let transactions = [
            transaction("july-pay", "2026-07-07", 90.0, "Payment"),
            transaction("july-spend", "2026-07-02", -40.0, "Shopping"),
            transaction("august-pay", "2026-08-07", 100.0, "Payment"),
            transaction("august-spend", "2026-08-03", -60.0, "Shopping"),
            transaction("september-pay", "2026-09-07", 110.0, "Payment"),
            transaction("current-spend", "2026-09-12", -25.0, "Shopping"),
        ];
        let at = "2026-09-14T12:00:00Z";
        let scopes = |value: Option<f64>| {
            let card = card(value);
            scopes(
                &transactions,
                std::slice::from_ref(&card),
                Some(&card),
                Some(&statement()),
                at,
                &NamedZone::Utc,
            )
        };
        let unknown = scopes(None);
        // Without a balance, the cycle opens at the autopay that paid the statement before it.
        let current = period(&unknown, "card", PeriodKind::Statement, "2026-08-24");
        assert_eq!(current.opening, Some(110.0));
        // 25 spent in 22 days, carried through the 31-day cycle.
        assert_eq!(current.projected.as_ref().unwrap().total, 35.23);
        let earlier = period(&unknown, "card", PeriodKind::Statement, "2026-07-24");
        assert_eq!(
            (earlier.opening, earlier.projected.as_ref()),
            (Some(100.0), None)
        );
        // With today's balance, it opens at that balance less the cycle's activity.
        let known = scopes(Some(-42.0));
        let current = period(&known, "card", PeriodKind::Statement, "2026-08-24");
        assert_eq!(current.opening, Some(127.0));
        // Other kinds of period have no statement.
        let month = period(&known, "card", PeriodKind::Month, "2026-09-01");
        assert_eq!(month.opening, None);
    }

    #[test]
    fn usual_spending_averages_covered_earlier_periods_at_the_same_point() {
        let dining = |id: &str, date: &str, amount: f64| transaction(id, date, -amount, "Dining");
        let history = vec![
            transaction("start", "2026-06-01", -1.0, "Shopping"),
            dining("june", "2026-06-05", 30.0),
            dining("july", "2026-07-05", 10.0),
            dining("august", "2026-08-05", 20.0),
            // Later in August than September has reached.
            dining("late", "2026-08-20", 50.0),
            dining("now", "2026-09-03", 25.0),
            transaction("new", "2026-09-04", -5.0, "Travel"),
        ];
        let at = "2026-09-10T12:00:00Z";
        let card = card(None);
        let run = |transactions: &[Transaction]| {
            scopes(
                transactions,
                std::slice::from_ref(&card),
                None,
                None,
                at,
                &NamedZone::Utc,
            )
        };
        let scopes = run(&history);
        let month = period(&scopes, "card", PeriodKind::Month, "2026-09-01");
        assert_eq!(
            month.typical,
            Some(BTreeMap::from([
                ("Dining".to_string(), 20.0),
                ("Shopping".to_string(), 0.33),
            ]))
        );
        // A week and more in with days left, so the month is projected.
        assert_eq!(month.projected.as_ref().unwrap().total, 90.0);
        assert_eq!(month.projected.as_ref().unwrap().categories["Travel"], 15.0);
        // One covered earlier period is not enough to call anything usual.
        let short = run(&history[2..]);
        assert_eq!(
            period(&short, "card", PeriodKind::Month, "2026-09-01").typical,
            None
        );
        // Every scope is offered: the account and all of them.
        assert_eq!(
            scopes
                .iter()
                .map(|entry| entry.scope.as_str())
                .collect::<Vec<_>>(),
            ["card", ALL_ACCOUNTS]
        );
    }

    #[test]
    fn a_rolling_week_is_never_projected() {
        let card = card(None);
        let scopes = scopes(
            &[transaction("spend", "2026-09-08", -70.0, "Dining")],
            std::slice::from_ref(&card),
            None,
            None,
            "2026-09-10T12:00:00Z",
            &NamedZone::Utc,
        );
        let week = period(&scopes, "card", PeriodKind::Week, "2026-09-04");
        assert_eq!(week.projected, None);
        // The quarter is well past a week in, with days left, so it is.
        let quarter = period(&scopes, "card", PeriodKind::Quarter, "2026-07-01");
        assert!(quarter.projected.is_some());
    }

    #[test]
    fn purchases_for_someone_else_count_only_as_theirs_or_everyones() {
        let mine = transaction("mine", "2026-09-02", -70.0, "Dining");
        let mut theirs = transaction("theirs", "2026-09-03", -30.0, "Dining");
        crate::transaction_policy::owe(theirs.classification.as_mut().unwrap(), theirs.amount);
        let card = card(Some(-100.0));
        let scopes = scopes(
            &[mine, theirs],
            std::slice::from_ref(&card),
            Some(&card),
            Some(&statement()),
            "2026-09-10T12:00:00Z",
            &NamedZone::Utc,
        );
        let total = |owner| {
            owned_period(&scopes, "card", owner, PeriodKind::Month, "2026-09-01")
                .projected
                .as_ref()
                .unwrap()
                .total
        };
        // Ten days of a 30-day month, carried to its end.
        assert_eq!(
            (total(Owner::Me), total(Owner::Other), total(Owner::All)),
            (210.0, 90.0, 300.0)
        );
        // The card's balance holds both purchases, so only everyone's spending opens at it.
        let opening = |owner| {
            owned_period(&scopes, "card", owner, PeriodKind::Statement, "2026-08-24").opening
        };
        assert_eq!(opening(Owner::All), Some(0.0));
        assert_eq!(opening(Owner::Other), None);
    }
}
