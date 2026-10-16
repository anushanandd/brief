//! Links credits to the purchases that earned them and places each credit in a window.
//!
//! The issuer applies a credit to the window of the purchase's transaction date, which may
//! close before either posts, and dates the credit the same. A credit pairs with the closest
//! earlier unpaired purchase it fits, within the terms' posting limit, preferring an exact
//! amount, or with one dated just after it when no earlier one fits. A reversal pairs with the
//! latest earlier credit of the same amount. An unpaired credit counts toward the window of its
//! own transaction date; only one the feed dates by posting, soon after a window opens, may
//! belong to either window, unless one of them can't hold it under its allowance. Delays are
//! measured between posting dates, since that is when a credit arrives.
use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};

use super::{ActivityKind, BenefitActivity, Period, PurchaseCredit, Rule};
use crate::numeric::from_minor;
use crate::numeric::minor_units;

/// Paired credits needed before their observed delay replaces the terms' limit.
const LEARNING_PAIRS: usize = 3;
/// Slack added to the longest observed delay.
const LAG_MARGIN_DAYS: i64 = 3;
/// How long before its purchase's date a credit may be dated: the issuer can credit the
/// authorization before the charge settles.
pub(super) const EARLY_CREDIT_DAYS: i64 = 3;

/// How long a benefit's credits take to post after the purchase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CreditLag {
    pub days: i64,
    /// Observed from this card's paired credits rather than taken from the rule data.
    pub learned: bool,
}

impl CreditLag {
    pub(super) fn observe(rule: &Rule, activity: &[BenefitActivity]) -> Self {
        let purchases = activity
            .iter()
            .filter(|item| item.benefit_id == rule.data.id && item.kind == ActivityKind::Purchase)
            .map(|item| (item.id.as_str(), item.posted))
            .collect::<HashMap<_, _>>();
        // A credit carries its purchase's transaction date, so only posting shows the delay.
        let delays = activity
            .iter()
            .filter(|item| {
                item.benefit_id == rule.data.id
                    && item.kind == ActivityKind::Credit
                    && !item.pending
            })
            .filter_map(|credit| {
                let posted = purchases.get(credit.paired_id.as_deref()?)?;
                Some((credit.posted - *posted).num_days())
            })
            .collect::<Vec<_>>();
        Self::from_delays(
            rule.data.posting_delay_days,
            rule.data.typical_delay_days,
            &delays,
        )
    }

    /// The terms' limit bounds pairing and decides when a credit is no longer coming; the
    /// usual delay, until learned, decides what is expected, late, or still settling.
    fn from_delays(limit: i64, typical: i64, delays: &[i64]) -> Self {
        match delays.iter().max() {
            Some(longest) if delays.len() >= LEARNING_PAIRS => Self {
                days: (longest + LAG_MARGIN_DAYS).min(limit),
                learned: true,
            },
            _ => Self {
                days: typical.min(limit),
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
                let days = (date - item.date).num_days();
                posted(item, ActivityKind::Purchase)
                    && item.paired_id.is_none()
                    && (-EARLY_CREDIT_DAYS..=limit).contains(&days)
                    && -minor_units(item.amount) >= amount
            })
            .min_by_key(|&index| {
                let days = (date - activity[index].date).num_days();
                (
                    days < 0,
                    -minor_units(activity[index].amount) != amount,
                    days.abs(),
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
        // Only a credit dated by its posting may belong to the window before.
        let earlier = (purchased.is_none()
            && !item.dated
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
                // The credit's delay runs from when the purchase posted.
                let age = (today - item.posted).num_days();
                if capacity <= 0 {
                    PurchaseCredit::OverAllowance
                } else if age <= lag.days {
                    PurchaseCredit::Expected {
                        amount,
                        by: item.posted + Duration::days(lag.days),
                    }
                } else if age <= limit {
                    PurchaseCredit::Late {
                        amount,
                        by: item.posted + Duration::days(limit),
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
            CreditLag::from_delays(56, 7, &[2, 3]),
            CreditLag {
                days: 7,
                learned: false
            }
        );
        assert_eq!(
            CreditLag::from_delays(56, 7, &[20, 30, 10]),
            CreditLag {
                days: 33,
                learned: true
            }
        );
        assert_eq!(CreditLag::from_delays(10, 7, &[9, 9, 9]).days, 10);
        assert_eq!(CreditLag::from_delays(5, 7, &[]).days, 5);
    }
}
