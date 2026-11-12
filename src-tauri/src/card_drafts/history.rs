//! What the user's own card shows: credits that posted, matched to drafted benefits. A match
//! confirms a benefit's credit wording, or proposes one where the catalog had none. Nothing
//! here leaves the Mac.
use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::catalog::{money, CREDIT};
use super::{CardDraft, Confidence, TermCheck};
use crate::finance_contract::Transaction;
use crate::insights::benefits::{RuleSource, SourceKind};
use crate::numeric::minor_units;

static CREDIT_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:credit|reimburse(?:ment|d)?)\b").expect("valid pattern")
});
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9#*]+").expect("valid pattern"));
const UNMATCHED: usize = 20;

/// A descriptor with numbers and spacing evened out, for showing and for a pattern.
fn plain(text: &str) -> String {
    DIGITS
        .replace_all(text, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn pattern(text: &str) -> Option<Regex> {
    Regex::new(&format!("(?i){text}")).ok()
}

/// The card's posted credits matched to the draft's benefits.
pub fn observe(draft: &mut CardDraft, transactions: &[Transaction]) {
    let account = draft.account_id.clone();
    let credits = transactions.iter().filter(|transaction| {
        transaction.account_id.as_deref() == Some(account.as_str())
            && !transaction.pending
            && transaction.amount > 0.0
            && (transaction
                .classification
                .as_ref()
                .is_some_and(|classification| classification.credit)
                || CREDIT_WORDS.is_match(&transaction.descriptors()))
    });
    let matchers = draft
        .rules
        .benefits
        .iter()
        .map(|rule| {
            (
                rule.credit_matches.as_deref().and_then(pattern),
                pattern(&rule.matches),
            )
        })
        .collect::<Vec<_>>();
    let mut seen = BTreeMap::<usize, Vec<&Transaction>>::new();
    let mut unmatched = Vec::<String>::new();
    for credit in credits {
        let text = credit.descriptors();
        let by_wording = matchers.iter().position(|(wording, _)| {
            wording
                .as_ref()
                .is_some_and(|wording| wording.is_match(&text))
        });
        let by_name = || {
            matchers
                .iter()
                .position(|(_, name)| name.as_ref().is_some_and(|name| name.is_match(&text)))
        };
        match by_wording.or_else(by_name) {
            Some(index) => seen.entry(index).or_default().push(credit),
            None => {
                let raw = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !raw.is_empty() && !unmatched.contains(&raw) && unmatched.len() < UNMATCHED {
                    unmatched.push(raw);
                }
            }
        }
    }
    for (index, credits) in seen {
        let rule = &mut draft.rules.benefits[index];
        let cap = minor_units(rule.cap);
        for credit in &credits {
            if minor_units(credit.amount) > cap {
                draft.warnings.push(format!(
                    "A {} credit of {} is more than its {} cap.",
                    rule.name,
                    money(credit.amount),
                    money(rule.cap)
                ));
            }
        }
        let example = plain(&credits[0].descriptors());
        let proposed = rule
            .credit_matches
            .is_none()
            .then(|| regex::escape(&example.to_lowercase()).replace(' ', r"\s+"));
        if let Some(proposed) = &proposed {
            rule.credit_matches = Some(proposed.clone());
        }
        rule.sources.push(RuleSource {
            kind: SourceKind::History,
            url: None,
            quote: Some(example.clone()),
            retrieved: None,
        });
        let full = credits
            .iter()
            .any(|credit| minor_units(credit.amount) == cap);
        let id = rule.id.clone();
        let Some(benefit) = draft.benefits.iter_mut().find(|benefit| benefit.id == id) else {
            continue;
        };
        benefit.credits_seen = credits.len();
        for term in &mut benefit.terms {
            let observed = term.field == CREDIT || (term.field == "cap" && full);
            if observed && term.confidence != Confidence::Conflict {
                term.confidence = Confidence::Observed;
                term.source = Some(SourceKind::History);
                term.quote = Some(example.clone());
                if term.field == CREDIT {
                    if let Some(proposed) = &proposed {
                        term.value = proposed.clone();
                        term.note = Some("Proposed from credits that posted on your card.".into());
                    }
                }
            }
        }
    }
    draft.unmatched_credits = unmatched;
    for (index, (_, name)) in matchers.iter().enumerate() {
        let rule = &mut draft.rules.benefits[index];
        if rule.logo_domain.is_some() {
            continue;
        }
        let Some((merchant, domain)) = transactions
            .iter()
            .filter(|transaction| {
                transaction.account_id.as_deref() == Some(account.as_str())
                    && !transaction.pending
                    && transaction.amount < 0.0
                    && name
                        .as_ref()
                        .is_some_and(|name| name.is_match(&transaction.descriptors()))
            })
            .filter_map(|transaction| {
                let domain = transaction
                    .website
                    .as_deref()
                    .and_then(crate::insights::forecast::website_domain)?;
                Some((
                    transaction.posted_date(),
                    transaction.merchant.clone(),
                    domain,
                ))
            })
            .max_by(|left, right| left.0.cmp(right.0))
            .map(|(_, merchant, domain)| (merchant, domain))
        else {
            continue;
        };
        rule.logo_domain = Some(domain.clone());
        let id = rule.id.clone();
        if let Some(benefit) = draft.benefits.iter_mut().find(|benefit| benefit.id == id) {
            benefit.terms.push(TermCheck {
                field: "logoDomain".into(),
                value: domain,
                confidence: Confidence::Observed,
                source: Some(SourceKind::History),
                sentence: None,
                quote: Some(merchant),
                note: Some("From where you used it on this card.".into()),
            });
        }
    }
}
