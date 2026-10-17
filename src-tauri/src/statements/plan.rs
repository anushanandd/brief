//! Decides which statement rows fill an account's history. A provider is the source of truth
//! from its earliest reported activity on; statements fill only the days before it. Rows that
//! overlapping statements both contain count once, while identical rows within one statement
//! (two equal purchases on one day) all count.
//!
//! For a Plaid cash or card account, the statement is the issuer's final record, so the days
//! Plaid covers are reconciled: each statement row is matched to the Plaid transaction it
//! describes, and what is left on either side is reported. A row Plaid never reported joins
//! history only once the user confirms it, and only while Plaid still hasn't reported it, so
//! a late Plaid row is never counted twice.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::Serialize;

use super::StatementRow;

/// One dated amount the provider already reports for the account, in the statement's sign.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderActivity {
    /// The provider's transaction id.
    pub id: String,
    pub date: NaiveDate,
    pub amount: Decimal,
    /// The provider's text for it, which tells apart charges of one amount.
    pub description: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// Rows this statement adds to the account's history.
    pub added: usize,
    /// Rows the provider already reports.
    pub covered: usize,
    /// Rows an earlier statement already added.
    pub repeated: usize,
    /// Pending rows, which the provider reports once they post.
    pub pending: usize,
    /// Rows in the provider's days that it never reported, awaiting the user's confirmation.
    pub missing: usize,
}

pub struct Planned<'a> {
    /// Stable for the same row in any statement, so edits survive removing and re-importing.
    pub id: String,
    pub row: &'a StatementRow,
    /// The row's amount signed as the account sees it: positive moves money in.
    pub amount: Decimal,
}

/// A statement row in the days the provider covers that it never reported.
pub struct Missing<'a> {
    /// The statement it came from, in the order given.
    pub statement: usize,
    pub planned: Planned<'a>,
    /// The user confirmed it, so it fills history.
    pub confirmed: bool,
}

/// How the statement days the provider covers compare with what it reported.
pub struct Reconciliation<'a> {
    pub from: NaiveDate,
    pub through: NaiveDate,
    /// Provider transactions a statement row confirms.
    pub matched: Vec<String>,
    /// Provider transactions whose amount a statement row describing them disagrees with,
    /// with the statement's amount.
    pub amount_differs: Vec<(String, Decimal)>,
    pub missing: Vec<Missing<'a>>,
    /// Posted provider transactions well inside a statement's days that it doesn't list.
    pub extra: Vec<String>,
}

pub struct Plan<'a> {
    pub rows: Vec<Planned<'a>>,
    /// Per statement, in the order given.
    pub counts: Vec<Counts>,
    pub provider_start: Option<NaiveDate>,
    /// Present when the account is reconciled and a statement reaches the provider's days.
    pub reconciliation: Option<Reconciliation<'a>>,
    /// Each provider transaction a statement row describes, at any amount, with that row.
    pub described: Vec<(String, &'a StatementRow)>,
}

/// A posted statement row near the provider's first day may appear there a few days later.
const BOUNDARY_DAYS: i64 = 7;
/// A provider may date a transaction up to this many days before the statement does.
const EARLIER_DAYS: i64 = 3;
/// A provider transaction this close to a statement's first or last day may belong to the
/// neighbouring statement, so it is never reported as missing from this one.
const EDGE_DAYS: i64 = 7;
/// Two rows of different amounts are the same transaction only when dated this close and
/// described this alike (percent of shared words).
const AMOUNT_DIFFERS_DAYS: i64 = 3;
const AMOUNT_DIFFERS_SIMILARITY: u32 = 50;

fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

fn key(row: &StatementRow) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        row.posted_on,
        row.amount.normalize(),
        words_in_order(&row.description),
        row.symbol.as_deref().unwrap_or(""),
        row.quantity
            .map(|value| value.normalize().to_string())
            .unwrap_or_default(),
    )
}

fn words_in_order(text: &str) -> String {
    text.to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The share of distinct words two descriptions have in common, in percent.
fn similarity(left: &str, right: &str) -> u32 {
    let (left, right) = (words(left), words(right));
    let union = left.union(&right).count();
    if union == 0 {
        return 0;
    }
    (left.intersection(&right).count() * 100 / union) as u32
}

fn row_id(account_id: &str, key: &str, occurrence: usize) -> String {
    let digest = super::content_id(format!("{account_id}|{key}").as_bytes());
    format!("statement:{}-{occurrence}", &digest[..20])
}

/// The id each posted row of one statement takes in history, as `plan` names it; none for a
/// pending row.
pub fn row_ids(account_id: &str, rows: &[StatementRow]) -> Vec<Option<String>> {
    let mut seen = BTreeMap::<String, usize>::new();
    rows.iter()
        .map(|row| {
            (!row.pending).then(|| {
                let key = key(row);
                let occurrence = seen.entry(key.clone()).or_default();
                let id = row_id(account_id, &key, *occurrence);
                *occurrence += 1;
                id
            })
        })
        .collect()
}

/// Pairs rows with provider transactions, closest first: same sign, fewest days apart, most
/// alike. `pairs` yields candidate (row, provider, sort key); each side is used once.
fn assign<K: Ord>(
    pairs: impl Iterator<Item = (usize, usize, K)>,
    rows: &mut [Option<usize>],
    used: &mut [bool],
) {
    let mut pairs = pairs.collect::<Vec<_>>();
    pairs.sort_by(|left, right| (&left.2, left.0, left.1).cmp(&(&right.2, right.0, right.1)));
    for (row, provider, _) in pairs {
        if rows[row].is_none() && !used[provider] {
            rows[row] = Some(provider);
            used[provider] = true;
        }
    }
}

/// Whether the provider dates `activity` close enough to `row` to be the same transaction.
fn within(row: &StatementRow, activity: &ProviderActivity) -> bool {
    (row.posted_on - Duration::days(EARLIER_DAYS)..=row.posted_on + Duration::days(BOUNDARY_DAYS))
        .contains(&activity.date)
}

/// Each statement's sign against the provider's: −1 for one whose amounts run the other way,
/// as a card issuer's export listing charges as positive does, when the statement's own total
/// and balances can't settle it. Judged by the rows the provider reports at the same size in
/// the same days; a statement with none follows the account's other statements.
fn signs(statements: &[&[StatementRow]], provider: &[ProviderActivity]) -> Vec<Decimal> {
    let tallies = statements
        .iter()
        .map(|rows| {
            rows.iter()
                .filter(|row| !row.pending)
                .fold((0, 0), |(same, opposite), row| {
                    let mut sizes = provider.iter().filter(|activity| {
                        within(row, activity) && activity.amount.abs() == row.amount.abs()
                    });
                    match sizes.clone().any(|activity| activity.amount == row.amount) {
                        true => (same + 1, opposite),
                        false if sizes.next().is_some() => (same, opposite + 1),
                        false => (same, opposite),
                    }
                })
        })
        .collect::<Vec<(usize, usize)>>();
    let (same, opposite) = tallies.iter().fold((0, 0), |total, tally| {
        (total.0 + tally.0, total.1 + tally.1)
    });
    tallies
        .iter()
        .map(|&(own_same, own_opposite)| {
            let flipped = if own_same + own_opposite > 0 {
                own_opposite > own_same
            } else {
                opposite > same
            };
            if flipped {
                Decimal::NEGATIVE_ONE
            } else {
                Decimal::ONE
            }
        })
        .collect()
}

/// Statements are given oldest import first; an earlier statement keeps the rows it shares
/// with a later one. `reconcile` holds the confirmed row ids of an account whose provider
/// days are reconciled; without it, rows in those days are taken as reported.
pub fn plan<'a>(
    account_id: &str,
    statements: &[&'a [StatementRow]],
    provider: &[ProviderActivity],
    reconcile: Option<&BTreeSet<String>>,
) -> Plan<'a> {
    let provider_start = provider.iter().map(|activity| activity.date).min();
    let mut counts = vec![Counts::default(); statements.len()];
    let mut taken = BTreeMap::<String, usize>::new();
    let mut unique = Vec::new();
    for (index, rows) in statements.iter().enumerate() {
        let mut seen = BTreeMap::<String, usize>::new();
        for row in rows.iter() {
            if row.pending {
                counts[index].pending += 1;
                continue;
            }
            let key = key(row);
            let occurrence = seen.entry(key.clone()).or_default();
            if *occurrence < taken.get(&key).copied().unwrap_or(0) {
                counts[index].repeated += 1;
            } else {
                unique.push((index, row, row_id(account_id, &key, *occurrence)));
            }
            *occurrence += 1;
        }
        for (key, count) in seen {
            let entry = taken.entry(key).or_default();
            *entry = (*entry).max(count);
        }
    }
    // Rows nearest the provider's first day claim boundary matches first.
    unique.sort_by(|left, right| {
        right
            .1
            .posted_on
            .cmp(&left.1.posted_on)
            .then_with(|| left.2.cmp(&right.2))
    });
    let near = |row: &StatementRow| {
        provider_start.is_some_and(|start| row.posted_on + Duration::days(BOUNDARY_DAYS) >= start)
    };
    let signs = match reconcile {
        Some(_) => signs(statements, provider),
        None => vec![Decimal::ONE; statements.len()],
    };
    let signed = |position: usize| unique[position].1.amount * signs[unique[position].0];

    // Same amount first; then, in the provider's days, one described alike for another amount.
    let mut matched = vec![None; unique.len()];
    let mut differs = vec![None; unique.len()];
    let mut used = vec![false; provider.len()];
    let candidates = |filter: &dyn Fn(&StatementRow, &ProviderActivity) -> bool| {
        unique
            .iter()
            .enumerate()
            .filter(|(_, (_, row, _))| near(row))
            .flat_map(|(index, (_, row, _))| {
                provider
                    .iter()
                    .enumerate()
                    .filter(move |(_, activity)| filter(row, activity))
                    .map(move |(candidate, activity)| (index, candidate, *row, activity))
            })
            .collect::<Vec<_>>()
    };
    let same_amount = candidates(&|row, activity| {
        within(row, activity) && activity.amount.abs() == row.amount.abs()
    });
    assign(
        same_amount.iter().map(|(index, candidate, row, activity)| {
            let days = (activity.date - row.posted_on).num_days().abs();
            let sign = activity.amount != signed(*index);
            let alike = similarity(&row.description, &activity.description);
            (*index, *candidate, (sign, days, u32::MAX - alike))
        }),
        &mut matched,
        &mut used,
    );
    if reconcile.is_some() {
        let start = provider_start.unwrap_or(NaiveDate::MAX);
        let described = candidates(&|row, activity| {
            row.posted_on >= start
                && (activity.date - row.posted_on).num_days().abs() <= AMOUNT_DIFFERS_DAYS
                && activity.amount.abs() != row.amount.abs()
        });
        assign(
            described
                .iter()
                .filter_map(|(index, candidate, row, activity)| {
                    let alike = similarity(&row.description, &activity.description);
                    (alike >= AMOUNT_DIFFERS_SIMILARITY && matched[*index].is_none()).then(|| {
                        let days = (activity.date - row.posted_on).num_days().abs();
                        (*index, *candidate, (u32::MAX - alike, days))
                    })
                }),
            &mut differs,
            &mut used,
        );
    }

    let mut rows = Vec::new();
    let mut reconciliation = reconcile.and_then(|_| {
        let start = provider_start?;
        let through = statements
            .iter()
            .flat_map(|rows| rows.iter())
            .map(|row| row.posted_on)
            .max()?;
        (through >= start).then(|| Reconciliation {
            from: start.max(
                statements
                    .iter()
                    .flat_map(|rows| rows.iter())
                    .map(|row| row.posted_on)
                    .min()
                    .unwrap_or(start),
            ),
            through,
            matched: Vec::new(),
            amount_differs: Vec::new(),
            missing: Vec::new(),
            extra: Vec::new(),
        })
    });
    let amounts = (0..unique.len()).map(signed).collect::<Vec<_>>();
    let mut described = Vec::new();
    for (position, (index, row, id)) in unique.into_iter().enumerate() {
        let amount = amounts[position];
        let planned = Planned { id, row, amount };
        let in_provider_days = provider_start.is_some_and(|start| row.posted_on >= start);
        if let Some(candidate) = matched[position].or(differs[position]) {
            described.push((provider[candidate].id.clone(), row));
        }
        if let Some(candidate) = matched[position] {
            counts[index].covered += 1;
            if let Some(reconciliation) = reconciliation.as_mut().filter(|_| in_provider_days) {
                reconciliation.matched.push(provider[candidate].id.clone());
            }
        } else if let Some(candidate) = differs[position] {
            counts[index].covered += 1;
            if let Some(reconciliation) = reconciliation.as_mut() {
                reconciliation
                    .amount_differs
                    .push((provider[candidate].id.clone(), amount));
            }
        } else if !in_provider_days {
            counts[index].added += 1;
            rows.push(planned);
        } else if let Some(confirmed) = reconcile {
            let confirmed = confirmed.contains(&planned.id);
            if confirmed {
                counts[index].added += 1;
                rows.push(Planned {
                    id: planned.id.clone(),
                    row,
                    amount,
                });
            } else {
                // Left out of history until the user confirms it.
                counts[index].missing += 1;
            }
            if let Some(reconciliation) = reconciliation.as_mut() {
                reconciliation.missing.push(Missing {
                    statement: index,
                    planned,
                    confirmed,
                });
            }
        } else {
            // Without reconciliation, the provider is taken to report its own days.
            counts[index].covered += 1;
        }
    }
    if let Some(reconciliation) = reconciliation.as_mut() {
        let spans = statements
            .iter()
            .filter_map(|rows| {
                let dates = rows
                    .iter()
                    .filter(|row| !row.pending)
                    .map(|row| row.posted_on);
                Some((dates.clone().min()?, dates.max()?))
            })
            .collect::<Vec<_>>();
        reconciliation.extra = provider
            .iter()
            .zip(&used)
            .filter(|(activity, used)| {
                !**used
                    && spans.iter().any(|(first, last)| {
                        activity.date >= *first + Duration::days(EDGE_DAYS)
                            && activity.date <= *last - Duration::days(EDGE_DAYS)
                    })
            })
            .map(|(activity, _)| activity.id.clone())
            .collect();
        reconciliation.matched.sort();
        reconciliation.amount_differs.sort();
        reconciliation.missing.sort_by(|left, right| {
            (left.planned.row.posted_on, &left.planned.id)
                .cmp(&(right.planned.row.posted_on, &right.planned.id))
        });
    }
    rows.sort_by(|left, right| {
        right
            .row
            .posted_on
            .cmp(&left.row.posted_on)
            .then_with(|| left.id.cmp(&right.id))
    });
    Plan {
        rows,
        counts,
        provider_start,
        reconciliation,
        described,
    }
}
