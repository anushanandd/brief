//! Spending's periods and the judgments made about them. Spending covers the saved card, every
//! USD account with spending, or one of them, over a statement, a rolling week, or a calendar
//! month, quarter, or year, or all of it, counting your spending, someone else's, or both. This
//! module sums each period and its categories beside the period before, and decides what a
//! statement opened at, where the current period is heading, and what is usual by this point.
//! The renderer only draws the period's activity.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use super::calendar::{self, Zone};
use super::statement::StatementCycle;
use crate::finance_contract::{Account, Transaction};
use crate::numeric::{from_minor, js_round, minor_units, round2};

mod detail;
pub use detail::PeriodDetail;

/// Earlier periods averaged for what is usual by this point.
const TYPICAL_PERIODS: usize = 3;
/// The pace is projected only after a week of the period, so one purchase can't dominate it.
const PROJECTION_DAYS: i64 = 7;
/// What Spending covers when the user picks every account.
pub const ALL_ACCOUNTS: &str = "all";
/// The months an account's spending chart shows.
const RECENT_MONTHS: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeriodKind {
    Statement,
    Week,
    Month,
    Quarter,
    Year,
    /// Everything from the first saved activity; it has no period before it.
    All,
}

impl PeriodKind {
    fn months(self) -> Option<i64> {
        match self {
            Self::Month => Some(1),
            Self::Quarter => Some(3),
            Self::Year => Some(12),
            Self::Statement | Self::Week | Self::All => None,
        }
    }
}

/// What a period spent, beside the period before to the same day.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spent {
    pub total: f64,
    /// None for all time, which has no period before it.
    pub previous: Option<f64>,
    pub change: Option<f64>,
    /// None when nothing was spent the period before.
    pub change_pct: Option<f64>,
    /// The total over the period's days so far.
    pub daily: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CategorySpent {
    pub name: String,
    #[serde(flatten)]
    pub spent: Spent,
    /// Its share of the period's spending, in percent.
    pub share: f64,
    /// Usual spending by this point (zero for a category new to the period) and how far above
    /// it this period is; none without enough history.
    pub typical: Option<f64>,
    pub above_typical: Option<f64>,
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
    /// Today for the current period, otherwise its last day.
    pub end: NaiveDate,
    #[serde(flatten)]
    pub spent: Spent,
    /// Spending still pending.
    pub pending: f64,
    /// The largest purchase's transaction ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largest: Option<String>,
    /// Categories with spending, largest first; a split part counts under its own category.
    pub categories: Vec<CategorySpent>,
    /// The statement balance at the period's end: the opening plus purchases, less credits and
    /// payments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<f64>,
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

/// A request for one period in full: whose spending, over which scope, which kind of period,
/// and how many periods back.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodRequest {
    pub scope: String,
    pub owner: Owner,
    pub kind: PeriodKind,
    pub offset: usize,
}

/// Whose spending a scope counts: yours, purchases made for someone else, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The current period of each kind.
    pub periods: Vec<SpendingPeriod>,
    /// The last few calendar months, oldest first.
    pub months: Vec<MonthSpent>,
}

/// One calendar month's spending, this month's to date.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonthSpent {
    /// `YYYY-MM`.
    pub month: String,
    pub total: f64,
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
            // All time starts at the first activity, which `periods` supplies.
            PeriodKind::All => None,
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
        return from_minor(minor_units(balance) - net(activity, start, end));
    }
    statement
        .autopays
        .iter()
        .find(|autopay| autopay.date >= start && autopay.date <= end)
        .map_or(0.0, |autopay| autopay.amount)
}

/// Purchases less credits and payments from `start` through `end`, in whole cents.
fn net(activity: &Activity, start: NaiveDate, end: NaiveDate) -> i64 {
    activity
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
        .sum()
}

/// A total in whole cents beside the one before it, averaged over `days`. The percent and the
/// average are rounded to hundredths.
fn spent(total: i64, previous: Option<i64>, days: i64) -> Spent {
    Spent {
        total: from_minor(total),
        previous: previous.map(from_minor),
        change: previous.map(|previous| from_minor(total - previous)),
        change_pct: previous
            .filter(|previous| *previous != 0)
            .map(|previous| round2((total - previous) as f64 / previous as f64 * 100.0)),
        daily: if days > 0 {
            round2(from_minor(total) / days as f64)
        } else {
            0.0
        },
    }
}

/// A period's sums from `start` through `end`, beside `before`, the period before cut at the
/// same day. Its judgments are left for the caller.
fn summed(
    activity: &Activity,
    kind: PeriodKind,
    start: NaiveDate,
    end: NaiveDate,
    before: Option<(NaiveDate, NaiveDate)>,
) -> SpendingPeriod {
    let days = (end - start).num_days() + 1;
    let cents = |transaction: &&Transaction| minor_units(transaction.amount.abs());
    let current: Vec<&Transaction> = activity.spending_in(start, end).collect();
    let total = current.iter().map(cents).sum();
    let previous = before.map(|(from, through)| {
        activity
            .spending_in(from, through)
            .map(|transaction| cents(&transaction))
            .sum()
    });
    let earlier = before.map_or_else(BTreeMap::new, |window| activity.by_category(&[window]));
    let mut categories: Vec<CategorySpent> = activity
        .by_category(&[(start, end)])
        .into_iter()
        .map(|(name, value)| CategorySpent {
            spent: spent(
                value,
                before.map(|_| earlier.get(&name).copied().unwrap_or(0)),
                days,
            ),
            share: if total == 0 {
                0.0
            } else {
                round2(value as f64 / total as f64 * 100.0)
            },
            typical: None,
            above_typical: None,
            name,
        })
        .collect();
    categories.sort_by(|left, right| {
        right
            .spent
            .total
            .total_cmp(&left.spent.total)
            .then_with(|| left.name.cmp(&right.name))
    });
    SpendingPeriod {
        kind,
        start,
        end,
        spent: spent(total, previous, days),
        pending: from_minor(
            current
                .iter()
                .filter(|transaction| transaction.pending)
                .map(cents)
                .sum(),
        ),
        // The first of equally large purchases.
        largest: current
            .iter()
            .min_by(|left, right| cents(right).cmp(&cents(left)))
            .map(|transaction| transaction.id.clone()),
        categories,
        balance: None,
        typical: None,
        projected: None,
        opening: None,
    }
}

/// One scope Spending offers, counted for one owner: its activity and, on the saved card, the
/// card and its statement evidence.
pub struct Scope<'a> {
    pub id: &'a str,
    pub owner: Owner,
    activity: Activity<'a>,
    card: Option<&'a Account>,
    statement: Option<&'a StatementCycle>,
    today: NaiveDate,
}

impl Scope<'_> {
    /// The kinds of period it offers: statements once the card has closing dates.
    fn kinds(&self) -> Vec<PeriodKind> {
        self.statement
            .filter(|statement| !statement.boundaries.is_empty())
            .map(|_| PeriodKind::Statement)
            .into_iter()
            .chain([
                PeriodKind::Week,
                PeriodKind::Month,
                PeriodKind::Quarter,
                PeriodKind::Year,
                PeriodKind::All,
            ])
            .collect()
    }

    /// A kind's calendar; statements without closing dates are calendar months.
    fn calendar(&self, kind: PeriodKind) -> Calendar<'_> {
        let boundaries = self
            .statement
            .map_or(&[][..], |statement| &statement.boundaries);
        let kind = if kind == PeriodKind::Statement && boundaries.is_empty() {
            PeriodKind::Month
        } else {
            kind
        };
        Calendar { kind, boundaries }
    }

    /// The first day all time counts from: the first activity, which for someone else's
    /// spending is the first purchase that is theirs.
    fn first(&self) -> NaiveDate {
        self.activity
            .dated
            .iter()
            .filter(|(date, transaction)| {
                *date <= self.today && (self.activity.owner != Owner::Other || transaction.other)
            })
            .map(|(date, _)| *date)
            .min()
            .unwrap_or(self.today)
    }

    /// The period `offset` periods before the current one of `kind`, summed and judged, with
    /// whether the saved history reaches a period before it. All time has only one.
    pub fn period(&self, kind: PeriodKind, offset: usize) -> Option<(SpendingPeriod, bool)> {
        let activity = &self.activity;
        if kind == PeriodKind::All {
            return (offset == 0).then(|| {
                let period = summed(activity, PeriodKind::All, self.first(), self.today, None);
                (period, false)
            });
        }
        let calendar = self.calendar(kind);
        let ends = calendar.ends(self.today, activity.first.unwrap_or(self.today));
        let end = *ends.get(offset)?;
        let start = calendar.start(end)?;
        let current = offset == 0;
        let from = calendar.previous_start(start);
        let through =
            (from + Duration::days((end - start).num_days())).min(start - Duration::days(1));
        // A statement balance is never only someone else's; today's balance anchors the
        // current cycle.
        let balance = self
            .card
            .filter(|card| card.r#type == "credit" && current)
            .and_then(|card| card.value)
            .map(f64::abs);
        let opening = self
            .statement
            .filter(|_| calendar.kind == PeriodKind::Statement && activity.owner != Owner::Other)
            .map(|statement| opening(activity, statement, balance, start, end));
        let typical = typical(&calendar, activity, start, end);
        let mut sums = summed(activity, calendar.kind, start, end, Some((from, through)));
        if let Some(typical) = &typical {
            for category in &mut sums.categories {
                let usual = typical.get(&category.name).copied().unwrap_or(0.0);
                category.typical = Some(usual);
                category.above_typical = Some(from_minor(
                    minor_units(category.spent.total) - minor_units(usual),
                ));
            }
        }
        let period = SpendingPeriod {
            typical,
            projected: current
                .then(|| projected(&calendar, activity, start, end))
                .flatten(),
            balance: opening
                .map(|opening| from_minor(minor_units(opening) + net(activity, start, end))),
            opening,
            ..sums
        };
        let earlier = ends
            .get(offset + 1)
            .is_some_and(|end| calendar.start(*end).is_some());
        Some((period, earlier))
    }

    /// Spending in each of the last `count` calendar months, oldest first, this one to date.
    fn months(&self, count: i64) -> Vec<MonthSpent> {
        (0..count)
            .rev()
            .map(|back| {
                let start = calendar::add_months_first(self.today, -back);
                let end =
                    (calendar::add_months_first(start, 1) - Duration::days(1)).min(self.today);
                MonthSpent {
                    month: start.format("%Y-%m").to_string(),
                    total: from_minor(
                        self.activity
                            .spending_in(start, end)
                            .map(|transaction| minor_units(transaction.amount.abs()))
                            .sum(),
                    ),
                }
            })
            .collect()
    }
}

/// Every scope Spending offers, for each owner it can count.
pub fn scope_list<'a>(
    transactions: &'a [Transaction],
    accounts: &'a [Account],
    card: Option<&'a Account>,
    statement: Option<&'a StatementCycle>,
    today: NaiveDate,
) -> Vec<Scope<'a>> {
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
            let on_card = card.filter(|card| card.id == scope);
            Scope {
                id: scope,
                owner,
                activity: Activity::new(transactions.iter().filter(in_scope).collect(), owner),
                card: on_card,
                statement: statement.filter(|_| on_card.is_some()),
                today,
            }
        })
        .collect()
}

/// Every scope Spending offers with its current period of each kind and its recent months.
/// Earlier periods are read one at a time through `Scope::period`.
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
    scope_list(transactions, accounts, card, statement, today)
        .iter()
        .map(|scope| SpendingScope {
            scope: scope.id.to_owned(),
            owner: scope.owner,
            periods: scope
                .kinds()
                .into_iter()
                .filter_map(|kind| scope.period(kind, 0).map(|(period, _)| period))
                .collect(),
            months: scope.months(RECENT_MONTHS),
        })
        .collect()
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
        // The autopay takes 110 off and the purchase adds 25.
        assert_eq!((current.spent.total, current.balance), (25.0, Some(25.0)));
        // 25 spent in 22 days, carried through the 31-day cycle.
        assert_eq!(current.projected.as_ref().unwrap().total, 35.23);
        // The cycle before is read on demand.
        let unknown_card = card(None);
        let statement = statement();
        let list = scope_list(
            &transactions,
            std::slice::from_ref(&unknown_card),
            Some(&unknown_card),
            Some(&statement),
            day("2026-09-14"),
        );
        let card_scope = list
            .iter()
            .find(|scope| scope.id == "card" && scope.owner == Owner::Me)
            .unwrap();
        let (earlier, before) = card_scope.period(PeriodKind::Statement, 1).unwrap();
        assert_eq!(earlier.start, day("2026-07-24"));
        assert_eq!(
            (earlier.opening, earlier.projected.as_ref()),
            (Some(100.0), None)
        );
        assert_eq!(
            (earlier.end, earlier.spent.total),
            (day("2026-08-23"), 60.0)
        );
        // History reaches one more cycle, then none.
        assert!(before);
        assert!(card_scope
            .period(PeriodKind::Statement, 2)
            .is_some_and(|(_, more)| !more));
        assert!(card_scope.period(PeriodKind::Statement, 3).is_none());
        // With today's balance, it opens at that balance less the cycle's activity.
        let known = scopes(Some(-42.0));
        let current = period(&known, "card", PeriodKind::Statement, "2026-08-24");
        assert_eq!(current.opening, Some(127.0));
        assert_eq!(current.balance, Some(42.0));
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
        // Each category in view against what is usual, zero for one new to the month.
        let against = month
            .categories
            .iter()
            .map(|category| {
                (
                    category.name.as_str(),
                    category.typical,
                    category.above_typical,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            against,
            vec![
                ("Dining", Some(20.0), Some(5.0)),
                ("Travel", Some(0.0), Some(5.0))
            ]
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

    #[test]
    fn each_period_sums_its_spending_beside_the_period_before_at_the_same_day() {
        let mut pending = transaction("pending", "2026-09-02", -25.0, "Dining");
        pending.pending = true;
        let transactions = [
            transaction("current", "2026-09-01", -100.0, "Shopping"),
            pending,
            // A payment, either way, is not spending.
            transaction("payment", "2026-09-03", 40.0, "Payment"),
            transaction("transfer", "2026-09-01", -500.0, "Payment"),
            transaction("previous", "2026-08-01", -50.0, "Shopping"),
            // August past the third, where September has reached.
            transaction("outside", "2026-08-20", -900.0, "Shopping"),
        ];
        let card = card(None);
        let scopes = scopes(
            &transactions,
            std::slice::from_ref(&card),
            None,
            None,
            "2026-09-03T12:00:00Z",
            &NamedZone::Utc,
        );
        let month = period(&scopes, "card", PeriodKind::Month, "2026-09-01");
        assert_eq!(
            (month.end, &month.spent, month.pending),
            (
                day("2026-09-03"),
                &Spent {
                    total: 125.0,
                    previous: Some(50.0),
                    change: Some(75.0),
                    change_pct: Some(150.0),
                    daily: 41.67,
                },
                25.0
            )
        );
        assert_eq!(month.largest.as_deref(), Some("current"));
        let categories = month
            .categories
            .iter()
            .map(|category| {
                (
                    category.name.as_str(),
                    category.spent.total,
                    category.spent.previous,
                    category.share,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            categories,
            vec![
                ("Shopping", 100.0, Some(50.0), 80.0),
                ("Dining", 25.0, Some(0.0), 20.0)
            ]
        );
        // All time runs from the first activity and has nothing before it.
        let all = period(&scopes, "card", PeriodKind::All, "2026-08-01");
        assert_eq!(
            (all.spent.total, all.spent.previous, all.spent.change_pct),
            (1_075.0, None, None)
        );
    }

    #[test]
    fn a_month_after_february_compares_with_february_through_its_last_day() {
        for year in [2024, 2025, 2026] {
            let card = card(None);
            let scopes = scopes(
                &[
                    transaction("feb", &format!("{year}-02-28"), -100.0, "Shopping"),
                    transaction("march", &format!("{year}-03-01"), -50.0, "Shopping"),
                ],
                std::slice::from_ref(&card),
                None,
                None,
                &format!("{year}-03-31T12:00:00Z"),
                &NamedZone::Utc,
            );
            let march = period(&scopes, "card", PeriodKind::Month, &format!("{year}-03-01"));
            assert_eq!(
                (march.spent.total, march.spent.previous, march.spent.daily),
                (50.0, Some(100.0), 1.61)
            );
        }
    }

    /// The card scope, counted for `owner`, as of `today`.
    fn card_scope<'a>(
        transactions: &'a [Transaction],
        accounts: &'a [Account],
        statement: Option<&'a StatementCycle>,
        owner: Owner,
        today: &str,
    ) -> Scope<'a> {
        scope_list(
            transactions,
            accounts,
            accounts.first(),
            statement,
            day(today),
        )
        .into_iter()
        .find(|scope| scope.id == "card" && scope.owner == owner)
        .unwrap()
    }

    fn with(transaction: Transaction, fields: serde_json::Value) -> Transaction {
        let mut value = serde_json::to_value(transaction).unwrap();
        for (key, field) in fields.as_object().unwrap() {
            value[key] = field.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_period_in_full_runs_its_spending_beside_the_whole_period_before() {
        let transactions = [
            transaction("current", "2026-09-01", -100.0, "Shopping"),
            with(
                transaction("pending", "2026-09-02", -25.0, "Dining"),
                json!({ "pending": true }),
            ),
            transaction("payment", "2026-09-03", 40.0, "Payment"),
            transaction("transfer", "2026-09-01", -500.0, "Payment"),
            transaction("previous", "2026-08-01", -50.0, "Shopping"),
            transaction("late-august", "2026-08-20", -900.0, "Shopping"),
        ];
        let accounts = [card(None)];
        let scope = card_scope(&transactions, &accounts, None, Owner::Me, "2026-09-03");
        let month = scope.detail(PeriodKind::Month, 0).unwrap();
        // The pace counts spending only, pending included; the month before runs whole.
        assert_eq!(month.view.pace.current, [100.0, 125.0, 125.0]);
        assert_eq!(month.view.pace.previous.len(), 31);
        assert_eq!(month.view.pace.previous.last(), Some(&950.0));
        assert_eq!(month.view.pace.end, day("2026-09-30"));
        assert_eq!(
            month.view.pace.days[1],
            detail::PaceDay {
                index: 1,
                date: day("2026-09-02"),
                total: 25.0,
                purchases: vec![detail::Purchase {
                    id: "pending".into(),
                    value: 25.0,
                }],
            }
        );
        // A month draws its pace, not bars; all time bars by month from its first day.
        assert!(month.bars.is_empty());
        let all = scope.detail(PeriodKind::All, 0).unwrap();
        let bars = all
            .bars
            .iter()
            .map(|bar| {
                (
                    bar.from,
                    bar.to,
                    bar.value,
                    bar.categories
                        .iter()
                        .map(|category| category.name.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            bars,
            [
                (
                    day("2026-08-01"),
                    day("2026-08-31"),
                    950.0,
                    vec!["Shopping"]
                ),
                (
                    day("2026-09-01"),
                    day("2026-09-03"),
                    125.0,
                    vec!["Dining", "Shopping"]
                ),
            ]
        );
        // Everything dated in the month is listed, newest first, and netted by merchant.
        assert_eq!(
            month.view.transaction_ids,
            ["payment", "pending", "transfer", "current"]
        );
        assert_eq!(month.view.merchants[0].name, "transfer");
        assert_eq!(month.view.merchants.last().unwrap().total, 40.0);
        // August has nothing before it, so the history reaches no earlier month.
        let august = scope.detail(PeriodKind::Month, 1).unwrap();
        assert_eq!((august.period.spent.total, august.earlier), (950.0, false));
        assert!(scope.detail(PeriodKind::Month, 2).is_none());
    }

    #[test]
    fn all_time_bars_by_month_with_credits_set_against_spending() {
        let credit = with(
            transaction("credit", "2026-09-01", 10.0, "Credit"),
            json!({ "classification": {
                "mark": "refund", "kind": "reimbursement", "spending": false, "credit": true,
                "zelle": false,
            } }),
        );
        let transactions = [
            transaction("spend", "2026-09-01", -30.0, "Shopping"),
            credit,
            transaction("july", "2026-07-15", -5.0, "Dining"),
        ];
        let accounts = [card(None)];
        let scope = card_scope(&transactions, &accounts, None, Owner::Me, "2026-09-03");
        let all = scope.detail(PeriodKind::All, 0).unwrap();
        // July, an empty August, then September with the credit set against it.
        assert_eq!(
            all.bars
                .iter()
                .map(|bar| (bar.from, bar.value))
                .collect::<Vec<_>>(),
            [
                (day("2026-07-15"), 5.0),
                (day("2026-08-01"), 0.0),
                (day("2026-09-01"), 20.0)
            ]
        );
        assert_eq!(
            all.bars[2]
                .categories
                .iter()
                .map(|category| (category.name.as_str(), category.value))
                .collect::<Vec<_>>(),
            [("Credits", -10.0), ("Shopping", 30.0)]
        );
        // The credit is not spending.
        assert_eq!(all.period.spent.total, 35.0);
    }

    #[test]
    fn a_statement_paces_through_its_closing_date() {
        let transactions = [
            transaction("august-pay", "2026-08-07", 100.0, "Payment"),
            transaction("august-spend", "2026-08-03", -60.0, "Shopping"),
            transaction("september-pay", "2026-09-07", 110.0, "Payment"),
            transaction("current-spend", "2026-09-12", -25.0, "Shopping"),
        ];
        let accounts = [card(None)];
        let statement = statement();
        let scope = card_scope(
            &transactions,
            &accounts,
            Some(&statement),
            Owner::Me,
            "2026-09-14",
        );
        let cycle = scope.detail(PeriodKind::Statement, 0).unwrap();
        assert_eq!(
            (cycle.view.pace.start, cycle.view.pace.end),
            (day("2026-08-24"), day("2026-09-23"))
        );
        assert_eq!(cycle.view.pace.current.len(), 22);
        assert_eq!(cycle.view.pace.current.last(), Some(&25.0));
        assert_eq!(cycle.view.pace.previous.len(), 31);
        assert_eq!(cycle.view.pace.previous.last(), Some(&60.0));
        assert_eq!(cycle.view.pace.projected, Some(35.23));
        // Payments are not spending, so only the purchase marks the pace.
        assert_eq!(
            cycle
                .view
                .pace
                .days
                .iter()
                .map(|day| (day.index, day.total))
                .collect::<Vec<_>>(),
            [(19, 25.0)]
        );
    }

    #[test]
    fn a_focus_narrows_to_one_category_and_owners_list_their_own() {
        let split = with(
            transaction("split", "2026-09-02", -100.0, "Groceries"),
            json!({ "merchant": "Example Market", "splits": [
                { "category": "Groceries", "amount": -70.0 },
                { "category": "Gifts", "amount": -30.0 },
            ] }),
        );
        let theirs = with(
            transaction("theirs", "2026-09-02", -40.0, "Dining"),
            json!({ "other": true, "classification": {
                "mark": "initial", "kind": "owed", "spending": false, "othersSpending": true,
                "zelle": false,
            } }),
        );
        let transactions = [
            split,
            with(
                transaction("whole", "2026-09-01", -20.0, "Groceries"),
                json!({ "merchant": "Example Market" }),
            ),
            transaction("dinner", "2026-09-03", -15.0, "Dining"),
            theirs,
        ];
        let accounts = [card(None)];
        let mine = card_scope(&transactions, &accounts, None, Owner::Me, "2026-09-03");
        let month = mine.detail(PeriodKind::Month, 0).unwrap();
        let groceries = &month.focus["Groceries"];
        // Only the grocery part of the split counts toward its pace and its share of the bars.
        assert_eq!(groceries.pace.current, [20.0, 90.0, 90.0]);
        let all = mine.detail(PeriodKind::All, 0).unwrap();
        assert_eq!(
            all.bars[0]
                .categories
                .iter()
                .map(|category| (category.name.as_str(), category.value))
                .collect::<Vec<_>>(),
            [("Dining", 15.0), ("Gifts", 30.0), ("Groceries", 90.0)]
        );
        assert_eq!(groceries.transaction_ids, ["split", "whole"]);
        // Every category with spending has its own view.
        assert_eq!(
            month.focus.keys().collect::<Vec<_>>(),
            ["Dining", "Gifts", "Groceries"]
        );
        assert_eq!(
            (
                groceries.merchants.len(),
                groceries.merchants[0].total,
                groceries.merchants[0].key.as_str()
            ),
            (1, -120.0, "example market")
        );
        // Your list leaves their purchase out; theirs lists only it.
        let all_mine = mine.detail(PeriodKind::Month, 0).unwrap();
        assert!(!all_mine
            .view
            .transaction_ids
            .contains(&"theirs".to_string()));
        let other = card_scope(&transactions, &accounts, None, Owner::Other, "2026-09-03");
        let theirs = other.detail(PeriodKind::Month, 0).unwrap();
        assert_eq!(theirs.view.transaction_ids, ["theirs"]);
        assert_eq!(theirs.period.spent.total, 40.0);
    }
}
