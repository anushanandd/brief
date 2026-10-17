//! The evidence graph's reasoning: published program rules run over observed records, each
//! conclusion with its proof, and life chapters detected from them. Derived on each read like
//! the other insights; only the user's confirmations, dismissals, edits, plans, and approved
//! rules are saved, in settings.
//!
//! Four kinds of statement are never mixed. Observed: a source states it, or you set it.
//! Derived: arithmetic or dates read from observed facts. Explained: a cited rule whose every
//! condition observed records meet. Suggested: a pattern no rule settles, or a rule with a
//! condition no record can show; yours to confirm or dismiss.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    documents::Document,
    finance_contract::{Account, Transaction},
    objects::Tier,
    settings::{ExplanationReview, FinanceSettings},
};

pub mod chapters;
pub mod employment;
mod engine;
pub mod programs;
#[cfg(test)]
mod tests;

/// One condition of a rule, as its proof states it, and what met it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ProofStep")]
pub struct Step {
    pub says: String,
    /// Object ids of the records that met it.
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    pub met: bool,
}

/// Why a conclusion holds: the rule, its cited source, and each condition with its evidence.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Proof {
    pub program: String,
    pub name: String,
    pub source: programs::Source,
    pub reviewed: NaiveDate,
    pub steps: Vec<Step>,
    /// Conditions no record can show, which keep the conclusion Suggested.
    pub unchecked: Vec<String>,
    pub notes: Vec<String>,
}

/// A conclusion a rule reached.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Explanation {
    pub id: String,
    pub program: String,
    pub title: String,
    pub says: String,
    /// Explained, Suggested, or Observed once you confirm a suggestion.
    pub tier: Tier,
    /// Object ids.
    pub subject: String,
    pub predicate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<NaiveDate>,
    pub proof: Proof,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "EvidenceAlertKind")]
pub enum AlertKind {
    /// A watched monthly condition fell short.
    Watch,
    /// Something a rule expects to follow hasn't.
    Expect,
    /// What a chapter's end changes.
    Ends,
}

/// A rule's note about what to watch or expect.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "EvidenceAlert")]
pub struct Alert {
    pub id: String,
    pub program: String,
    pub kind: AlertKind,
    pub says: String,
    pub date: NaiveDate,
    /// Object id.
    pub subject: String,
}

/// A rule Brief knows, for the list of programs and their sources.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProgramSummary {
    pub id: String,
    pub name: String,
    pub organization: String,
    pub summary: String,
    pub source: programs::Source,
    pub reviewed: NaiveDate,
    /// Not read against its source for over a year.
    pub stale: bool,
    /// Approved by you from research, rather than built in.
    pub approved: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "EvidenceGraph")]
pub struct Evidence {
    pub explanations: Vec<Explanation>,
    pub alerts: Vec<Alert>,
    pub chapters: Vec<chapters::Chapter>,
    pub now: chapters::Now,
    pub residency: Vec<chapters::Residency>,
    pub programs: Vec<ProgramSummary>,
}

/// What rules concluded, before chapters, which need the trips derived later.
pub struct Explained {
    pub employers: Vec<employment::Employer>,
    pub explanations: Vec<Explanation>,
    pub alerts: Vec<Alert>,
    pub programs: Vec<programs::Program>,
}

/// Runs every rule. A suggestion you dismissed is dropped; one you confirmed becomes
/// Observed, with you as its authority.
pub fn explain(
    transactions: &[Transaction],
    accounts: &[Account],
    documents: &[Document],
    settings: &FinanceSettings,
    today: NaiveDate,
) -> Explained {
    let employers = employment::employers(documents, transactions, today);
    let programs = programs::programs(&settings.programs);
    let facts = engine::Facts {
        transactions,
        accounts,
        employers: &employers,
        today,
    };
    let mut explanations = Vec::new();
    let mut alerts = Vec::new();
    for program in &programs {
        let (found, noted) = engine::evaluate(program, &facts);
        explanations.extend(found);
        alerts.extend(noted);
    }
    explanations.retain_mut(|explanation| {
        match settings.explanation_reviews.get(&explanation.id) {
            Some(ExplanationReview::Dismissed) => false,
            Some(ExplanationReview::Confirmed) => {
                if explanation.tier == Tier::Suggested {
                    explanation.tier = Tier::Observed;
                }
                true
            }
            None => true,
        }
    });
    Explained {
        employers,
        explanations,
        alerts,
        programs,
    }
}

/// An Explained offset (a bonus that repays a card's fee) files the deposit as a refund of
/// the fee, so it reduces that spending instead of counting as income, unless you linked the
/// deposit yourself. Suggested offsets change nothing until you confirm them.
pub fn apply(transactions: &mut [Transaction], explanations: &[Explanation]) {
    let mut refunds = Vec::new();
    for explanation in explanations.iter().filter(|explanation| {
        explanation.predicate == "offsets"
            && matches!(explanation.tier, Tier::Explained | Tier::Observed)
    }) {
        let (Some(deposit), Some(fee)) = (
            explanation.subject.strip_prefix("transaction:"),
            explanation
                .object
                .as_deref()
                .and_then(|object| object.strip_prefix("transaction:")),
        ) else {
            continue;
        };
        if let Some(index) = transactions
            .iter()
            .position(|transaction| transaction.id == deposit && transaction.refund_of.is_none())
        {
            refunds.push((index, fee.to_owned()));
        }
    }
    crate::transaction_policy::link_refunds(transactions, refunds);
}

/// Everything the evidence graph found, with chapters.
pub fn evidence(
    explained: Explained,
    transactions: &[Transaction],
    accounts: &[Account],
    away: &std::collections::BTreeSet<String>,
    settings: &FinanceSettings,
    today: NaiveDate,
) -> Evidence {
    // Each employer's chapter carries the rules that bound it, and what its end changes.
    let mut enabled = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut endings = std::collections::BTreeMap::<String, Vec<String>>::new();
    for explanation in &explained.explanations {
        for step in &explanation.proof.steps {
            for id in &step.evidence {
                if let Some(key) = id.strip_prefix("organization:") {
                    let list = enabled.entry(key.to_owned()).or_default();
                    if !list.contains(&explanation.program) {
                        list.push(explanation.program.clone());
                    }
                }
            }
        }
    }
    for alert in explained
        .alerts
        .iter()
        .filter(|alert| alert.kind == AlertKind::Ends)
    {
        if let Some(key) = alert.subject.strip_prefix("organization:") {
            endings
                .entry(key.to_owned())
                .or_default()
                .push(alert.says.clone());
        }
    }
    let chapters = chapters::chapters(&chapters::Inputs {
        transactions,
        accounts,
        employers: &explained.employers,
        away,
        programs: &enabled,
        endings: &endings,
        choices: &settings.chapters,
        planned: &settings.planned_chapters,
        today,
    });
    Evidence {
        now: chapters::now(&chapters, today),
        residency: chapters::residency(&chapters, today),
        explanations: explained.explanations,
        alerts: explained
            .alerts
            .into_iter()
            .filter(|alert| alert.kind != AlertKind::Ends)
            .collect(),
        chapters,
        programs: explained
            .programs
            .iter()
            .map(|program| ProgramSummary {
                id: program.id.clone(),
                name: program.name.clone(),
                organization: program.organization.clone(),
                summary: program.summary.clone(),
                source: program.source.clone(),
                reviewed: program.reviewed,
                stale: program.stale(today),
                approved: settings
                    .programs
                    .iter()
                    .any(|approved| approved.id == program.id),
            })
            .collect(),
    }
}
