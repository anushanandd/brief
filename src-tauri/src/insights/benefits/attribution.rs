//! Links credits to the purchases that earned them and places each credit in a window.
//!
//! The issuer applies a credit to the window of the purchase, which may close before the
//! credit posts. A credit pairs with the closest earlier unpaired purchase it fits, within
//! the terms' posting limit, preferring an exact amount. A reversal pairs with the latest
//! earlier credit of the same amount. An unpaired credit posted soon after a window opens
//! may belong to either window, unless one of them can't hold it under its allowance.
use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};

use super::{ActivityKind, BenefitActivity, Period, PurchaseCredit, Rule};
use crate::numeric::from_minor;
use crate::numeric::minor_units;

/// Paired credits needed before their observed delay replaces the terms' limit.
const LEARNING_PAIRS: usize = 3;
/// Slack added to the longest observed delay.
const LAG_MARGIN_DAYS: i64 = 3;

/// How long a benefit's credits take to post after the purchase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CreditLag {
    pub days: i64,
    /// Observed from this card's paired credits rather than taken from the terms.
    pub learned: bool,
}

impl CreditLag {
    pub(super) fn observe(rule: &Rule, activity: &[BenefitActivity]) -> Self {
        let purchases = activity
            .iter()
            .filter(|item| item.benefit_id == rule.data.id && item.kind == ActivityKind::Purchase)
            .map(|item| (item.id.as_str(), item.date))
            .collect::<HashMap<_, _>>();
        let delays = activity
            .iter()
            .filter(|item| {
                item.benefit_id == rule.data.id
                    && item.kind == ActivityKind::Credit
                    && !item.pending
            })
            .filter_map(|credit| {
                let purchased = purchases.get(credit.paired_id.as_deref()?)?;
                Some((credit.date - *purchased).num_days())
            })
            .collect::<Vec<_>>();
        Self::from_delays(rule.data.posting_delay_days, &delays)
    }

    fn from_delays(limit: i64, delays: &[i64]) -> Self {
        match delays.iter().max() {
            Some(longest) if delays.len() >= LEARNING_PAIRS => Self {
                days: (longest + LAG_MARGIN_DAYS).min(limit),
                learned: true,
            },
            _ => Self {
                days: limit,
                learned: false,
            },
        }
    }
}

/// Pairs, places, and settles one rule's activity as of `today`.
pub(super) fn attribute(rule: &Rule, activity: &mut [BenefitActivity], today: NaiveDate) {
    let mut ours = (0..activity.len())
        .filter(|&index| activity[index].benefit_id == rule.data.id)
        .collect::<Vec<_>>();
    if ours.is_empty() {
        return;
    }
    // Oldest first, so earlier credits claim earlier purchases.
    ours.sort_by(|&left, &right| {
        (activity[left].date, &activity[left].id).cmp(&(activity[right].date, &activity[right].id))
    });
    pair_credits(rule, activity, &ours);
    pair_reversals(activity, &ours);
    let lag = CreditLag::observe(rule, activity);
    let used = place_credits(rule, activity, &ours, lag);
    if rule.data.issuer_credits {
        settle_purchases(rule, activity, &ours, lag, &used, today);
    }
}

fn posted(item: &BenefitActivity, kind: ActivityKind) -> bool {
    item.kind == kind && !item.pending
}

fn pair_credits(rule: &Rule, activity: &mut [BenefitActivity], ours: &[usize]) {
    let limit = rule.data.posting_delay_days;
    for &credit in ours {
        if !posted(&activity[credit], ActivityKind::Credit) {
            continue;
        }
        let (amount, date) = (minor_units(activity[credit].amount), activity[credit].date);
        let purchase = ours
            .iter()
            .copied()
            .filter(|&index| {
                let item = &activity[index];
                posted(item, ActivityKind::Purchase)
                    && item.paired_id.is_none()
                    && item.date <= date
                    && (date - item.date).num_days() <= limit
                    && -minor_units(item.amount) >= amount
            })
            .min_by_key(|&index| {
                (
                    -minor_units(activity[index].amount) != amount,
                    (date - activity[index].date).num_days(),
                )
            });
        if let Some(purchase) = purchase {
            activity[credit].paired_id = Some(activity[purchase].id.clone());
            activity[purchase].paired_id = Some(activity[credit].id.clone());
        }
    }
}

fn pair_reversals(activity: &mut [BenefitActivity], ours: &[usize]) {
    let mut undone = HashSet::new();
    for &reversal in ours {
        if !posted(&activity[reversal], ActivityKind::Reversal) {
            continue;
        }
        let (amount, date) = (
            -minor_units(activity[reversal].amount),
            activity[reversal].date,
        );
        let credit = ours.iter().rev().copied().find(|&index| {
            let item = &activity[index];
            posted(item, ActivityKind::Credit)
                && item.date <= date
                && minor_units(item.amount) == amount
                && !undone.contains(&index)
        });
        if let Some(credit) = credit {
            undone.insert(credit);
            activity[reversal].paired_id = Some(activity[credit].id.clone());
        }
    }
}

/// Sets each posted credit's and reversal's window, and returns the certain credits per
/// window start, in cents.
fn place_credits(
    rule: &Rule,
    activity: &mut [BenefitActivity],
    ours: &[usize],
    lag: CreditLag,
) -> HashMap<NaiveDate, i64> {
    let dates = ours
        .iter()
        .map(|&index| (activity[index].id.clone(), activity[index].date))
        .collect::<HashMap<_, _>>();
    let mut used = HashMap::<NaiveDate, i64>::new();
    let mut open = Vec::<(usize, Period, Period)>::new();
    for &index in ours {
        let item = &activity[index];
        if !posted(item, ActivityKind::Credit) {
            continue;
        }
        let purchased = item
            .paired_id
            .as_ref()
            .and_then(|id| dates.get(id))
            .copied();
        let period = rule.period(purchased.unwrap_or(item.date));
        let earlier = (purchased.is_none()
            && period.cadence.is_calendar()
            && (item.date - period.start).num_days() < lag.days)
            .then(|| rule.previous_period(&period))
            .flatten();
        activity[index].credit_window = Some(period.start);
        match earlier {
            Some(earlier) => open.push((index, period, earlier)),
            None => *used.entry(period.start).or_default() += minor_units(activity[index].amount),
        }
    }
    // A window can't credit more than its allowance, so when only one window can hold an
    // open credit, it belongs there. Allowances that exclude tax can't decide this.
    for (index, later, earlier) in open {
        let amount = minor_units(activity[index].amount);
        let fits = |period: &Period, used: &HashMap<NaiveDate, i64>| {
            !rule.data.track_remaining
                || used.get(&period.start).copied().unwrap_or(0) + amount <= minor_units(period.cap)
        };
        match (fits(&earlier, &used), fits(&later, &used)) {
            (false, true) => *used.entry(later.start).or_default() += amount,
            (true, false) => {
                activity[index].credit_window = Some(earlier.start);
                *used.entry(earlier.start).or_default() += amount;
            }
            _ => activity[index].alternate_credit_window = Some(earlier.start),
        }
    }
    let windows = ours
        .iter()
        .map(|&index| {
            let item = &activity[index];
            (
                item.id.clone(),
                (item.credit_window, item.alternate_credit_window),
            )
        })
        .collect::<HashMap<_, _>>();
    for &index in ours {
        let item = &activity[index];
        if !posted(item, ActivityKind::Reversal) {
            continue;
        }
        let (window, alternate) = match item.paired_id.as_ref().and_then(|id| windows.get(id)) {
            Some(&placement) => placement,
            // An unmatched reversal may undo a credit from the previous window.
            None => {
                let period = rule.period(item.date);
                (
                    Some(period.start),
                    rule.previous_period(&period).map(|earlier| earlier.start),
                )
            }
        };
        let amount = minor_units(item.amount);
        if let (Some(window), None) = (window, alternate) {
            *used.entry(window).or_default() += amount;
        }
        activity[index].credit_window = window;
        activity[index].alternate_credit_window = alternate;
    }
    used
}

fn settle_purchases(
    rule: &Rule,
    activity: &mut [BenefitActivity],
    ours: &[usize],
    lag: CreditLag,
    used: &HashMap<NaiveDate, i64>,
    today: NaiveDate,
) {
    let credits = ours
        .iter()
        .map(|&index| (activity[index].id.clone(), activity[index].amount))
        .collect::<HashMap<_, _>>();
    let limit = rule.data.posting_delay_days;
    for &index in ours {
        let item = &activity[index];
        if !posted(item, ActivityKind::Purchase) {
            continue;
        }
        let credit = match item.paired_id.as_ref().and_then(|id| credits.get(id)) {
            Some(&amount) => PurchaseCredit::Credited { amount },
            None => {
                let period = rule.period(item.date);
                // Without a dollar allowance (such as one plus tax), the purchase sets it.
                let capacity = if !rule.data.track_remaining {
                    -minor_units(item.amount)
                } else if period.cadence.is_calendar() {
                    minor_units(period.cap) - used.get(&period.start).copied().unwrap_or(0)
                } else {
                    minor_units(period.cap)
                };
                let amount = from_minor((-minor_units(item.amount)).min(capacity));
                let age = (today - item.date).num_days();
                if capacity <= 0 {
                    PurchaseCredit::OverAllowance
                } else if age <= lag.days {
                    PurchaseCredit::Expected {
                        amount,
                        by: item.date + Duration::days(lag.days),
                    }
                } else if age <= limit {
                    PurchaseCredit::Late {
                        amount,
                        by: item.date + Duration::days(limit),
                    }
                } else {
                    PurchaseCredit::NotCredited
                }
            }
        };
        activity[index].credit = Some(credit);
    }
}

#[cfg(test)]
mod tests {
    use super::CreditLag;

    #[test]
    fn credit_lag_is_learned_only_from_enough_pairs_and_never_exceeds_the_terms() {
        assert_eq!(
            CreditLag::from_delays(56, &[2, 3]),
            CreditLag {
                days: 56,
                learned: false
            }
        );
        assert_eq!(
            CreditLag::from_delays(56, &[2, 3, 1]),
            CreditLag {
                days: 6,
                learned: true
            }
        );
        assert_eq!(CreditLag::from_delays(10, &[9, 9, 9]).days, 10);
    }
}
