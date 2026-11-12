//! Drafting a card product's benefit rules. A draft starts from the open card benefits
//! catalog, is checked against the issuer's published terms, and is confirmed by the credits
//! that posted on the user's card. `apply` saves clear matches without asking.
pub mod apply;
pub mod catalog;
pub mod history;
pub mod programs;
pub mod terms;
#[cfg(test)]
mod tests;

use serde::Serialize;
use ts_rs::TS;

use crate::insights::benefits::{CardRule, CardRules, SourceKind};

/// How sure a drafted term is, by its best evidence.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// A credit that posted on the card matches it.
    Observed,
    /// A sentence of the issuer's terms states it.
    Explained,
    /// Only the catalog says so.
    Suggested,
    /// The terms say something different from the draft.
    Conflict,
    /// Nothing supports it yet.
    Unconfirmed,
}

/// One drafted term of a benefit (its cap, cadence, reset, merchants, or credit wording) and
/// what it rests on.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TermCheck {
    pub field: String,
    pub value: String,
    pub confidence: Confidence,
    pub source: Option<SourceKind>,
    /// The terms sentence it cites, by its index in `CardDraft::sentences`.
    pub sentence: Option<usize>,
    pub quote: Option<String>,
    pub note: Option<String>,
}

/// One drafted benefit, its terms checked.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct BenefitDraft {
    pub id: String,
    pub name: String,
    pub terms: Vec<TermCheck>,
    /// Credits on the card its credit wording matches.
    pub credits_seen: usize,
    /// Whether the draft keeps it; a perk with no dollar value starts left out.
    pub include: bool,
}

/// A card in the catalog, as a search lists it.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCard {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub annual_fee: Option<f64>,
    pub benefits: usize,
}

/// A card's drafted rules and the evidence for each term.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CardDraft {
    pub account_id: String,
    /// The catalog card the draft starts from, and others that matched, best first.
    pub catalog: Option<CatalogCard>,
    pub candidates: Vec<CatalogCard>,
    /// The rules as drafted, ready to save once approved.
    pub rules: CardRules,
    pub benefits: Vec<BenefitDraft>,
    /// The issuer's terms, split into the sentences terms cite.
    pub terms_url: Option<String>,
    pub sentences: Vec<String>,
    /// Descriptors of credits on the card that no drafted benefit explains.
    pub unmatched_credits: Vec<String>,
    pub warnings: Vec<String>,
}

/// One term that differs between saved rules and a fresh draft.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TermChange {
    pub benefit: String,
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Saved rules whose catalog entry or terms page changed since they were approved.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CardRulesChange {
    pub product: String,
    pub name: String,
    /// What changed: the catalog entry, the terms page, or both.
    pub reason: String,
    pub draft: CardDraft,
    pub changes: Vec<TermChange>,
}

fn terms_of(rule: &CardRule) -> [(&'static str, Option<String>); 6] {
    [
        ("cap", Some(catalog::money(rule.cap))),
        (
            "cadence",
            Some(format!("{:?}", rule.cadence).to_lowercase()),
        ),
        (
            "alignment",
            Some(format!("{:?}", rule.alignment).to_lowercase()),
        ),
        ("matches", Some(rule.matches.clone())),
        (catalog::CREDIT, rule.credit_matches.clone()),
        (
            "effectiveUntil",
            rule.effective_until.map(|day| day.to_string()),
        ),
    ]
}

/// The terms that differ between saved rules and a fresh draft, benefit by benefit.
pub fn diff(saved: &CardRules, fresh: &CardRules) -> Vec<TermChange> {
    let mut changes = Vec::new();
    if saved.annual_fee != fresh.annual_fee {
        changes.push(TermChange {
            benefit: fresh.name.clone(),
            field: "annualFee".into(),
            before: saved.annual_fee.map(catalog::money),
            after: fresh.annual_fee.map(catalog::money),
        });
    }
    for rule in &saved.benefits {
        match fresh
            .benefits
            .iter()
            .find(|candidate| candidate.id == rule.id)
        {
            None => changes.push(TermChange {
                benefit: rule.name.clone(),
                field: "benefit".into(),
                before: Some(rule.name.clone()),
                after: None,
            }),
            Some(next) => {
                for ((field, before), (_, after)) in terms_of(rule).into_iter().zip(terms_of(next))
                {
                    if before != after {
                        changes.push(TermChange {
                            benefit: rule.name.clone(),
                            field: field.into(),
                            before,
                            after,
                        });
                    }
                }
            }
        }
    }
    for rule in &fresh.benefits {
        if !saved
            .benefits
            .iter()
            .any(|candidate| candidate.id == rule.id)
        {
            changes.push(TermChange {
                benefit: rule.name.clone(),
                field: "benefit".into(),
                before: None,
                after: Some(rule.name.clone()),
            });
        }
    }
    changes
}
