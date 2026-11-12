//! An account's or employer's terms page, read into the existing program templates. A value is
//! filled only when a sentence states it, and the sentence is returned beside it; the user
//! fills the rest and approves as before.
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use ts_rs::TS;

use super::terms::stated;
use crate::providers::exa::Page;
use crate::research::{Draft, Gap, GapKind, Template};

static FEE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:monthly (?:service|maintenance) fee|monthly fee|service fee|maintenance fee)\b",
    )
    .expect("valid pattern")
});
static DEPOSITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bdeposits?\b").expect("valid pattern"));
static BONUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbonus\b").expect("valid pattern"));

/// A prefilled value and the sentence that states it.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct PrefillQuote {
    pub field: String,
    pub sentence: String,
}

/// A program draft prefilled from a terms page.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProgramPrefill {
    pub draft: Draft,
    pub quotes: Vec<PrefillQuote>,
}

/// The first sentence matching `topic` with a dollar amount, and that amount.
fn first_amount(sentences: &[String], topic: &Regex) -> Option<(f64, String)> {
    sentences.iter().find_map(|sentence| {
        if !topic.is_match(sentence) {
            return None;
        }
        let cents = *stated(sentence).amounts.first()?;
        Some((cents as f64 / 100.0, sentence.clone()))
    })
}

pub fn prefill(gap: &Gap, page: &Page, today: NaiveDate) -> ProgramPrefill {
    let sentences = crate::providers::reading::sentences(&page.text);
    let template = match gap.kind {
        GapKind::Card => Template::BonusOffsetsFee,
        GapKind::Employer | GapKind::Account => Template::EmployeeDepositWaiver,
    };
    let mut quotes = Vec::new();
    let mut take = |field: &str, found: Option<(f64, String)>| {
        found.map(|(amount, sentence)| {
            quotes.push(PrefillQuote {
                field: field.into(),
                sentence,
            });
            amount
        })
    };
    let (amount, fee) = match template {
        Template::BonusOffsetsFee => (take("amount", first_amount(&sentences, &BONUS)), None),
        Template::EmployeeDepositWaiver => {
            let fee = take("fee", first_amount(&sentences, &FEE));
            let deposits = take(
                "amount",
                sentences.iter().find_map(|sentence| {
                    if !DEPOSITS.is_match(sentence)
                        || FEE.is_match(sentence) && stated(sentence).amounts.len() < 2
                    {
                        return None;
                    }
                    let largest = *stated(sentence).amounts.iter().max()?;
                    Some((largest as f64 / 100.0, sentence.clone()))
                }),
            );
            (deposits, fee)
        }
    };
    let card = gap.kind == GapKind::Card;
    let summary = quotes
        .first()
        .map(|quote| quote.sentence.clone())
        .unwrap_or_default();
    ProgramPrefill {
        draft: Draft {
            template,
            name: gap.name.clone(),
            organization: gap.name.clone(),
            summary,
            source_url: page.url.clone(),
            source_title: page.title.clone(),
            effective: today,
            account_name: if card {
                String::new()
            } else {
                gap.name.clone()
            },
            institution: String::new(),
            card_name: card.then(|| gap.name.clone()),
            bonus_words: card.then(|| "bonus".to_owned()),
            amount: amount.unwrap_or(0.0),
            fee,
            unobservable: None,
        },
        quotes,
    }
}
