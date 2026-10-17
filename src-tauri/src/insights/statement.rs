//! Card statement cycles. Issuer-reported statement dates (Plaid Liabilities) are used when
//! available; otherwise a stable posted autopay cadence estimates the closing day.
use std::{collections::BTreeMap, sync::LazyLock};

use chrono::{Datelike, Duration, NaiveDate};
use regex::Regex;
use serde::Serialize;
use ts_rs::TS;

use super::calendar;
use crate::finance_contract::CreditLiability;
use crate::finance_contract::Transaction;

static AUTOPAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bautopay payment(?: received)?\s*-?\s*thank you\b").expect("valid pattern")
});
/// Amex autopay posts about fifteen days after the statement closes.
const CLOSE_LEAD_DAYS: i64 = 15;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Autopay {
    pub date: NaiveDate,
    pub amount: f64,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatementSource {
    /// Statement dates reported by the card issuer through Plaid Liabilities.
    Issuer,
    /// Closing days estimated from a stable posted autopay cadence.
    Autopay,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatementCycle {
    pub source: StatementSource,
    /// One closing date per month, oldest first.
    pub boundaries: Vec<NaiveDate>,
    pub autopays: Vec<Autopay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_close: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_close: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub liability: Option<CreditLiability>,
}

/// Posted autopay receipts on or before the reference day, one per date.
pub fn autopays(transactions: &[&Transaction], reference: &str) -> Vec<Autopay> {
    let reference_day = calendar::transaction_date(reference);
    let mut payments = transactions
        .iter()
        .filter_map(|transaction| {
            let text = format!(
                "{} {}",
                transaction.merchant,
                transaction.description.as_deref().unwrap_or("")
            );
            let date = calendar::transaction_date(transaction.posted_date())?;
            (transaction.amount > 0.0 && !transaction.pending && AUTOPAY.is_match(&text)).then_some(
                Autopay {
                    date,
                    amount: transaction.amount,
                },
            )
        })
        .filter(|payment| reference_day.is_some_and(|day| payment.date <= day))
        .collect::<Vec<_>>();
    payments.sort_by_key(|payment| payment.date);
    payments.dedup_by(|later, earlier| later.date == earlier.date);
    payments
}

fn months_between(earlier: NaiveDate, later: NaiveDate) -> i64 {
    calendar::month_index(later) - calendar::month_index(earlier)
}

/// Monthly closing days covering the comparison history, oldest first.
fn monthly_boundaries(
    transactions: &[&Transaction],
    reference_day: NaiveDate,
    typical_day: u32,
    observed: &BTreeMap<(i32, u32), NaiveDate>,
    fallback_first: NaiveDate,
) -> Vec<NaiveDate> {
    let first = transactions
        .iter()
        .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
        .min()
        .unwrap_or(fallback_first);
    let coverage = calendar::add_months_first(reference_day, -25);
    let mut month = calendar::add_months_first(first.min(coverage), -1);
    let last = calendar::add_months_first(reference_day, 1);
    let mut boundaries = Vec::new();
    while month <= last {
        let end = calendar::month_end(month.year(), month.month()).day();
        boundaries.push(
            observed
                .get(&(month.year(), month.month()))
                .copied()
                .unwrap_or_else(|| month + Duration::days(typical_day.min(end) as i64 - 1)),
        );
        month = calendar::add_months_first(month, 1);
    }
    boundaries
}

struct AutopayEvidence {
    typical_day: u32,
    observed: BTreeMap<(i32, u32), NaiveDate>,
    first_close: NaiveDate,
}

/// Closing days observed from a stable autopay cadence, or `None` without consistent evidence.
fn autopay_evidence(transactions: &[&Transaction], reference: &str) -> Option<AutopayEvidence> {
    let dates = autopays(transactions, reference)
        .into_iter()
        .map(|payment| payment.date)
        .collect::<Vec<_>>();
    if dates.len() < 2 {
        return None;
    }
    let intervals = dates
        .windows(2)
        .map(|pair| {
            let months = months_between(pair[0], pair[1]);
            if months > 0 && months <= 3 {
                (pair[1] - pair[0]).num_days() as f64 / months as f64
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    let monthly = intervals
        .iter()
        .filter(|days| (24.0..=38.0).contains(*days))
        .count();
    if (monthly as f64) < intervals.len() as f64 * 0.6 {
        return None;
    }
    let closings = dates
        .iter()
        .map(|date| *date - Duration::days(CLOSE_LEAD_DAYS))
        .collect::<Vec<_>>();
    let mut days = closings.iter().map(|date| date.day()).collect::<Vec<_>>();
    days.sort_unstable();
    Some(AutopayEvidence {
        typical_day: days[days.len() / 2],
        observed: closings
            .iter()
            .map(|date| ((date.year(), date.month()), *date))
            .collect(),
        first_close: closings[0],
    })
}

/// Closing dates inferred from a stable autopay cadence; empty without consistent evidence.
pub fn autopay_boundaries(transactions: &[&Transaction], reference: &str) -> Vec<NaiveDate> {
    let (Some(reference_day), Some(evidence)) = (
        calendar::transaction_date(reference),
        autopay_evidence(transactions, reference),
    ) else {
        return Vec::new();
    };
    monthly_boundaries(
        transactions,
        reference_day,
        evidence.typical_day,
        &evidence.observed,
        evidence.first_close,
    )
}

/// The statement cycle for a card: issuer dates when reported, else autopay evidence.
pub fn cycle(
    transactions: &[&Transaction],
    reference: &str,
    liability: Option<&CreditLiability>,
) -> Option<StatementCycle> {
    let reference_day = calendar::transaction_date(reference)?;
    let issuer_close = liability
        .and_then(|liability| liability.last_statement_date.as_deref())
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .filter(|date| *date <= reference_day);
    let (source, boundaries) = match issuer_close {
        Some(close) => {
            // The issuer's latest closing date is exact and sets the usual day; older months
            // keep any closings observed from autopay evidence.
            let mut observed = autopay_evidence(transactions, reference)
                .map(|evidence| evidence.observed)
                .unwrap_or_default();
            observed.insert((close.year(), close.month()), close);
            (
                StatementSource::Issuer,
                monthly_boundaries(transactions, reference_day, close.day(), &observed, close),
            )
        }
        None => (
            StatementSource::Autopay,
            autopay_boundaries(transactions, reference),
        ),
    };
    if boundaries.is_empty() {
        return None;
    }
    Some(StatementCycle {
        source,
        last_close: boundaries
            .iter()
            .rev()
            .find(|date| **date <= reference_day)
            .copied(),
        next_close: boundaries
            .iter()
            .find(|date| **date > reference_day)
            .copied(),
        boundaries,
        autopays: autopays(transactions, reference),
        liability: liability.cloned(),
    })
}
