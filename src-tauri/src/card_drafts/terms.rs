//! The issuer's published terms, read against a draft. Rust finds the sentences that state a
//! benefit's dollar amount and period and compares them itself; a model may only point at
//! sentences, and a sentence Rust can't read an amount and period from is never evidence.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use super::catalog::{money, period, words};
use super::{CardDraft, Confidence};
use crate::insights::benefits::{Alignment, Cadence, CardRule, RuleSource, SourceKind};
use crate::numeric::minor_units;

static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\s?(\d{1,3}(?:,\d{3})*(?:\.\d{2})?|\d+(?:\.\d{2})?)").expect("valid pattern")
});
static PERIODS: LazyLock<Vec<(Cadence, Regex)>> = LazyLock::new(|| {
    [
        (Cadence::Monthly, r"per month|a month|each month|every month|monthly|/\s?mo\b"),
        (Cadence::Quarterly, r"per quarter|a quarter|each quarter|every quarter|quarterly|every three months"),
        (Cadence::Semiannual, r"semi-?annual(?:ly)?|twice a year|every six months|each half[- ]year|per half[- ]year|bi-?annual(?:ly)?|january through june|july through december"),
        (Cadence::Annual, r"per year|a year|each year|every year|annual(?:ly)?|per calendar year|each calendar year|cardmember year|account anniversary year|anniversary year"),
        (Cadence::Renewal, r"every (?:4|four|4\.5|four and a half|five|5) years"),
    ]
    .into_iter()
    .map(|(cadence, pattern)| (cadence, Regex::new(&format!(r"(?i)\b(?:{pattern})")).expect("valid pattern")))
    .collect()
});
static ANNIVERSARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)cardmember year|account anniversary|anniversary year|anniversary date")
        .expect("valid pattern")
});
static LETTERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-z]{3,}").expect("valid pattern"));
static CALENDAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)calendar (?:year|month|quarter)").expect("valid pattern"));
const GENERIC_BENEFIT: &[&str] = &[
    "statement",
    "annual",
    "monthly",
    "benefit",
    "benefits",
    "up",
    "to",
    "per",
    "year",
    "each",
    "your",
];

/// What a sentence says: the amounts it states and the periods it names.
pub(super) struct Stated {
    pub amounts: Vec<i64>,
    pub cadences: BTreeSet<CadenceKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CadenceKey(u8);

fn key(cadence: Cadence) -> CadenceKey {
    CadenceKey(match cadence {
        Cadence::Monthly => 0,
        Cadence::Quarterly => 1,
        Cadence::Semiannual => 2,
        Cadence::Annual => 3,
        Cadence::Renewal => 4,
        Cadence::Purchase => 5,
    })
}

pub(super) fn stated(sentence: &str) -> Stated {
    Stated {
        amounts: AMOUNT
            .captures_iter(sentence)
            .filter_map(|caps| caps[1].replace(',', "").parse::<f64>().ok())
            .map(minor_units)
            .collect(),
        cadences: PERIODS
            .iter()
            .filter(|(_, pattern)| pattern.is_match(sentence))
            .map(|(cadence, _)| key(*cadence))
            .collect(),
    }
}

/// The words a sentence must share to be about this benefit.
fn topic(rule: &CardRule) -> BTreeSet<String> {
    words(&rule.name)
        .into_iter()
        .chain(
            LETTERS
                .find_iter(&rule.matches.to_lowercase())
                .map(|word| word.as_str().to_owned()),
        )
        .filter(|word| word.len() >= 3 && !GENERIC_BENEFIT.contains(&word.as_str()))
        .collect()
}

pub enum Evidence {
    Agrees(usize),
    Differs {
        sentence: usize,
        amount: Option<i64>,
        cadence: bool,
    },
}

/// Whether a sentence states an amount and a period.
fn states_terms(sentence: &str) -> bool {
    let stated = stated(sentence);
    !stated.amounts.is_empty() && !stated.cadences.is_empty()
}

/// Whether a sentence names the benefit and states its terms.
fn readable(rule: &CardRule, sentence: &str) -> bool {
    states_terms(sentence) && !topic(rule).is_disjoint(&words(sentence))
}

/// The best evidence among all sentences: one that names the benefit and states the drafted
/// cap and period, else the first that names it and states something else.
pub fn evidence(
    rule: &CardRule,
    sentences: &[String],
    indexes: impl IntoIterator<Item = usize>,
) -> Option<Evidence> {
    evidence_in(rule, sentences, indexes, true)
}

/// As `evidence`, over sentences a model tied to the benefit, which needn't repeat its name.
fn evidence_in(
    rule: &CardRule,
    sentences: &[String],
    indexes: impl IntoIterator<Item = usize>,
    named: bool,
) -> Option<Evidence> {
    let cap = minor_units(rule.cap);
    let cadence = key(rule.cadence);
    let mut differs = None;
    for index in indexes {
        let Some(sentence) = sentences.get(index) else {
            continue;
        };
        let readable = if named {
            readable(rule, sentence)
        } else {
            states_terms(sentence)
        };
        if !readable {
            continue;
        }
        let stated = stated(sentence);
        let amount = stated.amounts.contains(&cap);
        let period = stated.cadences.contains(&cadence);
        if amount && period {
            return Some(Evidence::Agrees(index));
        }
        differs.get_or_insert(Evidence::Differs {
            sentence: index,
            amount: (!amount).then(|| stated.amounts[0]),
            cadence: !period,
        });
    }
    differs
}

/// The sentences a model picked that Rust can read an amount and a period from.
pub fn verified(sentences: &[String], picked: &[usize]) -> Vec<usize> {
    picked
        .iter()
        .copied()
        .filter(|index| {
            sentences
                .get(*index)
                .is_some_and(|sentence| states_terms(sentence))
        })
        .collect()
}

/// The draft checked against the issuer's terms page. `picks` are sentence indexes a model
/// chose per benefit id, used only where the rule finds no evidence of its own.
pub fn apply(
    draft: &mut CardDraft,
    url: &str,
    text: &str,
    picks: &BTreeMap<String, Vec<usize>>,
    today: NaiveDate,
) {
    let sentences = crate::providers::reading::sentences(text);
    draft.terms_url = Some(url.to_owned());
    draft.rules.terms_url = Some(url.to_owned());
    draft.rules.terms_hash = Some(crate::research::page_hash(text));
    for rule in &mut draft.rules.benefits {
        let found = evidence(rule, &sentences, 0..sentences.len()).or_else(|| {
            let picked = picks.get(&rule.id)?;
            evidence_in(rule, &sentences, verified(&sentences, picked), false)
        });
        let Some(found) = found else { continue };
        let Some(benefit) = draft
            .benefits
            .iter_mut()
            .find(|benefit| benefit.id == rule.id)
        else {
            continue;
        };
        let (index, agrees) = match &found {
            Evidence::Agrees(index) => (*index, true),
            Evidence::Differs { sentence, .. } => (*sentence, false),
        };
        let quote = sentences[index].clone();
        if agrees {
            rule.sources.push(RuleSource {
                kind: SourceKind::Terms,
                url: Some(url.to_owned()),
                quote: Some(quote.clone()),
                retrieved: Some(today),
            });
        }
        let anniversary = ANNIVERSARY.is_match(&quote);
        let calendar = CALENDAR.is_match(&quote);
        for term in &mut benefit.terms {
            let differs = match (&found, term.field.as_str()) {
                (Evidence::Agrees(_), "cap" | "cadence") => Some(false),
                (Evidence::Differs { amount, .. }, "cap") => Some(amount.is_some()),
                (Evidence::Differs { cadence, .. }, "cadence") => Some(*cadence),
                (_, "alignment") if anniversary || calendar => {
                    Some((rule.alignment == Alignment::Anniversary) != anniversary)
                }
                _ => None,
            };
            let Some(differs) = differs else { continue };
            if term.confidence == Confidence::Observed && !differs {
                continue;
            }
            term.confidence = if differs {
                Confidence::Conflict
            } else {
                Confidence::Explained
            };
            term.source = Some(SourceKind::Terms);
            term.sentence = Some(index);
            term.quote = Some(quote.clone());
            term.note = match (&found, term.field.as_str()) {
                (
                    Evidence::Differs {
                        amount: Some(amount),
                        ..
                    },
                    "cap",
                ) => Some(format!("The terms say {}.", money(*amount as f64 / 100.0))),
                (Evidence::Differs { cadence: true, .. }, "cadence") => Some(format!(
                    "The terms don't say per {}.",
                    period(cadence_name(rule.cadence), None)
                )),
                _ => None,
            };
        }
    }
    draft.sentences = sentences;
}

fn cadence_name(cadence: Cadence) -> &'static str {
    match cadence {
        Cadence::Monthly => "monthly",
        Cadence::Quarterly => "quarterly",
        Cadence::Semiannual => "semiannual",
        Cadence::Annual => "annual",
        Cadence::Renewal => "renewal",
        Cadence::Purchase => "purchase",
    }
}

/// Indexes of sentences that state an amount and a period, the ones worth showing a model.
pub fn candidates(sentences: &[String]) -> Vec<usize> {
    sentences
        .iter()
        .enumerate()
        .filter(|(_, sentence)| {
            let stated = stated(sentence);
            !stated.amounts.is_empty() && !stated.cadences.is_empty()
        })
        .map(|(index, _)| index)
        .collect()
}
