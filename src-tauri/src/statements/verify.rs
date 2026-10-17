//! Checks a statement against itself: its printed total and its running balances. They also
//! settle the sign convention, since a balance only follows its amounts one way.
use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;

use super::StatementRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "VerificationStatus")]
pub enum Status {
    /// The printed total and every running balance agree with the rows.
    Verified,
    /// Some running balances don't follow from the rows, often because the institution's
    /// balance includes money the export doesn't list.
    Partial,
    /// The statement prints nothing to check against.
    Unverified,
    /// The printed total disagrees with the rows. Such a file is never imported.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "StatementVerification")]
pub struct Check {
    pub status: Status,
    pub total_matches: Option<bool>,
    pub balance_rows: usize,
    pub balance_explained: usize,
}

pub(super) struct Outcome {
    pub check: Check,
    pub flip_signs: bool,
}

/// How many balances follow from the previous balance plus the row's amount, reading dates
/// oldest first. Exports often list one day's rows in any order, so within a day the rows are
/// taken in whichever order continues the chain. The first balance has no predecessor and
/// counts as explained.
fn explained(rows: &[&StatementRow], sign: Decimal) -> usize {
    let mut days = BTreeMap::<_, Vec<(Decimal, Decimal)>>::new();
    for row in rows {
        if let Some(balance) = row.balance {
            days.entry(row.posted_on)
                .or_default()
                .push((row.amount * sign, balance));
        }
    }
    let mut current: Option<Decimal> = None;
    let mut count = 0;
    for (_, mut remaining) in days {
        while !remaining.is_empty() {
            if let Some(index) = current.and_then(|current| {
                remaining
                    .iter()
                    .position(|(amount, balance)| current + amount == *balance)
            }) {
                current = Some(remaining.remove(index).1);
                count += 1;
                continue;
            }
            // A break: resume from the row no other row of the day leads to.
            let index = remaining
                .iter()
                .position(|(amount, balance)| {
                    !remaining
                        .iter()
                        .any(|(_, other)| *other == *balance - *amount)
                })
                .unwrap_or(0);
            if current.is_none() {
                count += 1;
            }
            current = Some(remaining.remove(index).1);
        }
    }
    count
}

pub(super) fn check(rows: &[StatementRow], total: Option<Decimal>) -> Outcome {
    let posted = rows.iter().filter(|row| !row.pending).collect::<Vec<_>>();
    let balance_rows = posted.iter().filter(|row| row.balance.is_some()).count();
    let sum = rows.iter().map(|row| row.amount).sum::<Decimal>();
    let evaluate = |sign: Decimal| {
        (
            total.map(|total| sum * sign == total),
            explained(&posted, sign),
        )
    };
    let (total_matches, balance_explained) = evaluate(Decimal::ONE);
    let (flipped_total, flipped_explained) = evaluate(Decimal::NEGATIVE_ONE);
    // Amounts are read as printed unless the statement's own figures say otherwise.
    let flip_signs = (total_matches == Some(false) && flipped_total == Some(true))
        || (flipped_total != Some(false)
            && total_matches != Some(true)
            && flipped_explained > balance_explained);
    let (total_matches, balance_explained) = if flip_signs {
        (flipped_total, flipped_explained)
    } else {
        (total_matches, balance_explained)
    };
    let status = if total_matches == Some(false) {
        Status::Failed
    } else if balance_rows > 0 {
        if balance_explained == balance_rows {
            Status::Verified
        } else {
            Status::Partial
        }
    } else if total_matches == Some(true) {
        Status::Verified
    } else {
        Status::Unverified
    };
    Outcome {
        check: Check {
            status,
            total_matches,
            balance_rows,
            balance_explained,
        },
        flip_signs,
    }
}
