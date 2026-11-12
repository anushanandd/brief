//! How often each detector is right, by your own answers: a suggested rule explanation you
//! confirmed or dismissed, a detected chapter, a benefit match, a suggested category kept or
//! declined, and the second looks you marked reviewed. Only counting: each detector's last
//! `WINDOW` answers, ordered by the day they concern (by id where a dismissed record keeps
//! none), and the share accepted. A detector whose share falls below `FLOOR` over at least
//! `MINIMUM` answers is flagged, so a threshold is raised on evidence rather than a hunch.
use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

use crate::finance_contract::Transaction;
use crate::settings::{ChapterStatus, ExplanationReview, FinanceSettings};

/// The answers a detector's precision reads.
pub const WINDOW: usize = 20;
/// Fewer answers than this say too little to judge.
pub const MINIMUM: usize = 5;
/// A detector right less often than this is flagged.
pub const FLOOR: f64 = 0.7;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct DetectorPrecision {
    /// `rule:<program>`, `chapter:<kind>`, `benefit-match`, `category-suggestion`, or
    /// `second-look`.
    pub detector: String,
    /// Within the last `WINDOW` answers.
    pub accepted: usize,
    pub rejected: usize,
    /// Every answer ever given.
    pub answers: usize,
    /// Whether accepting is something you can do here; a second look can only be dismissed,
    /// so it has no precision.
    pub accepts_observed: bool,
    /// Accepted over accepted and rejected, once there are `MINIMUM` answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub precision: Option<f64>,
    /// Precision below `FLOOR`.
    pub low: bool,
}

/// One answer: the day it concerns (a sort key), and whether it accepted.
type Answer = (String, bool);

fn summarize(
    detector: String,
    mut answers: Vec<Answer>,
    accepts_observed: bool,
) -> DetectorPrecision {
    answers.sort();
    let total = answers.len();
    let recent = &answers[total.saturating_sub(WINDOW)..];
    let accepted = recent.iter().filter(|(_, accepted)| *accepted).count();
    let rejected = recent.len() - accepted;
    let precision = (accepts_observed && recent.len() >= MINIMUM)
        .then(|| accepted as f64 / recent.len() as f64);
    DetectorPrecision {
        detector,
        accepted,
        rejected,
        answers: total,
        accepts_observed,
        precision,
        low: precision.is_some_and(|precision| precision < FLOOR),
    }
}

/// Each detector with at least one answer, in name order.
pub fn precision(
    settings: &FinanceSettings,
    transactions: &[Transaction],
) -> Vec<DetectorPrecision> {
    let date = |id: &str| {
        transactions
            .iter()
            .find(|transaction| transaction.id == id)
            .map_or_else(|| id.to_owned(), |transaction| transaction.date.clone())
    };
    let mut answers = BTreeMap::<String, Vec<Answer>>::new();
    // An explanation's id is its rule's id and a hash; dismissed ones keep no date.
    for (id, review) in &settings.explanation_reviews {
        let program = id
            .rsplit_once(':')
            .map_or(id.as_str(), |(program, _)| program);
        answers
            .entry(format!("rule:{program}"))
            .or_default()
            .push((id.clone(), *review == ExplanationReview::Confirmed));
    }
    // A chapter's id is its kind, then what it is about and when.
    for choice in &settings.chapters {
        let kind = choice.id.split(':').next().unwrap_or_default();
        if kind == "planned" {
            continue;
        }
        answers
            .entry(format!("chapter:{kind}"))
            .or_default()
            .push((choice.id.clone(), choice.status == ChapterStatus::Confirmed));
    }
    for transaction in transactions {
        if let Some(confirmed) = transaction.benefit_confirmed {
            answers
                .entry("benefit-match".into())
                .or_default()
                .push((transaction.date.clone(), confirmed));
        }
    }
    for (id, review) in &settings.category_reviews {
        answers
            .entry("category-suggestion".into())
            .or_default()
            .push((date(id), review.accepted));
    }
    let dismissed = transactions
        .iter()
        .filter(|transaction| transaction.reviewed == Some(true))
        .map(|transaction| (transaction.date.clone(), false))
        .collect::<Vec<_>>();
    let mut found = answers
        .into_iter()
        .map(|(detector, answers)| summarize(detector, answers, true))
        .collect::<Vec<_>>();
    if !dismissed.is_empty() {
        found.push(summarize("second-look".into(), dismissed, false));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{CategoryReview, ChapterChoice};

    fn transaction(id: &str, date: &str) -> Transaction {
        serde_json::from_value(serde_json::json!({
            "id": id, "merchant": "Example", "category": "Dining", "date": date,
            "amount": -20.0, "account": "Card", "pending": false,
        }))
        .unwrap()
    }

    #[test]
    fn counts_each_detectors_answers_and_flags_one_wrong_too_often() {
        let transactions = (1..=9)
            .map(|day| transaction(&format!("t{day}"), &format!("2026-09-0{day}")))
            .collect::<Vec<_>>();
        // Category suggestions: kept on the first three days, declined on the last six.
        let category_reviews = transactions
            .iter()
            .enumerate()
            .map(|(index, transaction)| {
                (
                    transaction.id.clone(),
                    CategoryReview {
                        category: "Dining".into(),
                        accepted: index < 3,
                    },
                )
            })
            .collect();
        let settings = FinanceSettings {
            category_reviews,
            explanation_reviews: BTreeMap::from([
                ("bonus:aaaa".into(), ExplanationReview::Confirmed),
                ("bonus:bbbb".into(), ExplanationReview::Dismissed),
            ]),
            chapters: vec![ChapterChoice {
                id: "home:co:2026-03".into(),
                status: ChapterStatus::Confirmed,
                title: None,
                start: None,
                end: None,
            }],
            ..Default::default()
        };
        let found = precision(&settings, &transactions);
        let row = |detector: &str| found.iter().find(|row| row.detector == detector).unwrap();
        let category = row("category-suggestion");
        assert_eq!(
            (category.accepted, category.rejected, category.answers),
            (3, 6, 9)
        );
        assert!((category.precision.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!(category.low);
        // Two answers say too little to judge.
        let rule = row("rule:bonus");
        assert_eq!((rule.accepted, rule.rejected, rule.precision), (1, 1, None));
        assert!(!rule.low);
        assert_eq!(row("chapter:home").accepted, 1);
    }

    #[test]
    fn reads_only_the_latest_answers_and_never_rates_dismissals_alone() {
        let transactions = (0..30)
            .map(|day| {
                let mut transaction =
                    transaction(&format!("t{day:02}"), &format!("2026-08-{:02}", day + 1));
                transaction.benefit_confirmed = Some(day >= 10);
                transaction.reviewed = (day < 3).then_some(true);
                transaction
            })
            .collect::<Vec<_>>();
        let found = precision(&FinanceSettings::default(), &transactions);
        // The last twenty matches were all confirmed; the ten early mistakes are past.
        let matches = found
            .iter()
            .find(|row| row.detector == "benefit-match")
            .unwrap();
        assert_eq!(
            (matches.accepted, matches.rejected, matches.answers),
            (20, 0, 30)
        );
        assert_eq!(matches.precision, Some(1.0));
        let looks = found
            .iter()
            .find(|row| row.detector == "second-look")
            .unwrap();
        assert_eq!(
            (looks.rejected, looks.precision, looks.accepts_observed),
            (3, None, false)
        );
    }
}
