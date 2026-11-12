//! A card's catalog entry: found by the names Plaid reports, and turned into drafted rules
//! whose every term says it came from the catalog.
use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde_json::{json, Value};

use super::{BenefitDraft, CardDraft, CatalogCard, Confidence, TermCheck};
use crate::finance_contract::Account;
use crate::insights::benefits::{brand_key, CardRules, RuleSet, SourceKind};
use crate::providers::card_catalog::{CardCatalog, CatalogBenefit, CatalogCardEntry, CATALOG_HOME};

/// A catalog card matches when at least this share of its distinctive name words appear.
const MATCH_SCORE: f64 = 0.5;
const CANDIDATES: usize = 5;
const SEARCH_RESULTS: usize = 10;
const GENERIC: &[&str] = &[
    "the", "card", "cards", "credit", "of", "and", "for", "from", "by", "with",
];

pub(super) fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !GENERIC.contains(word))
        .map(str::to_owned)
        .collect()
}

pub fn summary(card: &CatalogCardEntry) -> CatalogCard {
    CatalogCard {
        id: card.id.clone(),
        name: card.name.clone(),
        issuer: card.issuer.clone(),
        annual_fee: card.annual_fee,
        benefits: card.benefits.iter().filter(|b| trackable(b)).count(),
    }
}

fn score(card: &CatalogCardEntry, query: &BTreeSet<String>) -> f64 {
    let name = words(&card.name);
    if name.is_empty() {
        return 0.0;
    }
    let mut score = name.intersection(query).count() as f64 / name.len() as f64;
    if !words(&card.issuer).is_disjoint(query) {
        score += 0.25;
    }
    if card.is_business != query.contains("business") {
        score -= 0.5;
    }
    score
}

/// Catalog cards for an account, best first: by the issuer's official name, the account's
/// name, and its institution. A discontinued card is offered only when nothing current fits.
pub fn candidates<'a>(catalog: &'a CardCatalog, account: &Account) -> Vec<&'a CatalogCardEntry> {
    scored(catalog, account)
        .into_iter()
        .map(|(_, card)| card)
        .collect()
}

fn query(account: &Account) -> BTreeSet<String> {
    [
        account.official_name.as_deref().unwrap_or_default(),
        &account.name,
        &account.institution,
    ]
    .iter()
    .flat_map(|text| words(text))
    .collect()
}

/// Whether every distinctive word of the card's name is among the account's names.
pub fn named(card: &CatalogCardEntry, account: &Account) -> bool {
    let name = words(&card.name);
    !name.is_empty() && name.is_subset(&query(account))
}

/// `candidates` with each card's match score.
pub fn scored<'a>(catalog: &'a CardCatalog, account: &Account) -> Vec<(f64, &'a CatalogCardEntry)> {
    let query = query(account);
    let mut scored = catalog
        .cards
        .iter()
        .map(|card| (score(card, &query), card))
        .filter(|(score, _)| *score >= MATCH_SCORE)
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0).then(left.1.id.cmp(&right.1.id)));
    let current = scored.iter().any(|(_, card)| !card.discontinued);
    scored
        .into_iter()
        .filter(|(_, card)| !current || !card.discontinued)
        .take(CANDIDATES)
        .collect()
}

/// Cards whose name or issuer holds most of the typed words.
pub fn search(catalog: &CardCatalog, text: &str) -> Vec<CatalogCard> {
    let query = words(text);
    if query.is_empty() {
        return Vec::new();
    }
    let mut found = catalog
        .cards
        .iter()
        .map(|card| {
            let known = words(&format!("{} {}", card.name, card.issuer));
            (
                query.intersection(&known).count() as f64 / query.len() as f64,
                card,
            )
        })
        .filter(|(share, _)| *share >= 0.5)
        .collect::<Vec<_>>();
    found.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then(left.1.discontinued.cmp(&right.1.discontinued))
            .then(left.1.name.cmp(&right.1.name))
    });
    found
        .into_iter()
        .take(SEARCH_RESULTS)
        .map(|(_, card)| summary(card))
        .collect()
}

/// Every brand a catalog benefit could name, the words a drafted rule's logo is asked for by.
pub fn brands(catalog: &CardCatalog) -> Vec<String> {
    catalog
        .cards
        .iter()
        .flat_map(|card| &card.benefits)
        .flat_map(|benefit| benefit.match_keywords.iter().chain([&benefit.name]))
        .filter_map(|text| brand_key(text))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn slug(text: &str) -> String {
    let slug = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars().take(64).collect()
}

/// The cadence and, for a longer custom interval, its renewal months.
fn cadence(benefit: &CatalogBenefit) -> Option<(&'static str, Option<u32>)> {
    match benefit.frequency.as_deref()? {
        "monthly" => Some(("monthly", None)),
        "quarterly" => Some(("quarterly", None)),
        "semiannual" | "semi-annual" => Some(("semiannual", None)),
        "annual" | "yearly" => Some(("annual", None)),
        "custom" => match benefit.interval_months? {
            1 => Some(("monthly", None)),
            3 => Some(("quarterly", None)),
            6 => Some(("semiannual", None)),
            12 => Some(("annual", None)),
            months if months > 12 => Some(("renewal", Some(months))),
            _ => None,
        },
        _ => None,
    }
}

/// The catalog's category as a lowercase word, or none when it isn't one.
pub(super) fn category(benefit: &CatalogBenefit) -> Option<String> {
    let category = benefit
        .category
        .as_deref()?
        .trim()
        .to_lowercase()
        .replace(' ', "-");
    (!category.is_empty()
        && category.len() <= 32
        && category.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
    .then_some(category)
}

fn trackable(benefit: &CatalogBenefit) -> bool {
    benefit.kind.as_deref() != Some("perk")
        && benefit.value.is_some_and(|value| value > 0.0)
        && cadence(benefit).is_some()
}

pub(super) fn money(amount: f64) -> String {
    if amount.fract() == 0.0 {
        format!("${amount:.0}")
    } else {
        format!("${amount:.2}")
    }
}

pub(super) fn period(cadence: &str, months: Option<u32>) -> String {
    match (cadence, months) {
        ("monthly", _) => "month".into(),
        ("quarterly", _) => "quarter".into(),
        ("semiannual", _) => "half-year".into(),
        ("annual", _) => "year".into(),
        (_, Some(months)) => format!("{months} months"),
        _ => cadence.into(),
    }
}

fn any_of(alternatives: &[String]) -> Option<String> {
    let alternatives = alternatives
        .iter()
        .map(|text| {
            text.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|text| !text.is_empty())
        .map(|text| regex::escape(&text))
        .collect::<BTreeSet<_>>();
    (!alternatives.is_empty()).then(|| {
        format!(
            r"\b(?:{})\b",
            alternatives.into_iter().collect::<Vec<_>>().join("|")
        )
    })
}

fn check(field: &str, value: String, confidence: Confidence, note: Option<&str>) -> TermCheck {
    TermCheck {
        field: field.into(),
        value,
        confidence,
        source: (confidence == Confidence::Suggested).then_some(SourceKind::Catalog),
        sentence: None,
        quote: None,
        note: note.map(str::to_owned),
    }
}

/// One catalog benefit as a rule, or none for a perk Brief can't track.
fn rule(
    benefit: &CatalogBenefit,
    id: &str,
    card: &CatalogCardEntry,
    today: NaiveDate,
) -> Option<Value> {
    if !trackable(benefit) {
        return None;
    }
    let value = benefit.value?;
    let (cadence, months) = cadence(benefit)?;
    let matches = any_of(&benefit.match_keywords)
        .unwrap_or_else(|| regex::escape(&benefit.name.to_lowercase()));
    let instruction = [
        benefit
            .enrollment_required
            .then(|| "Enroll first.".to_owned()),
        benefit
            .notes
            .clone()
            .filter(|notes| !notes.trim().is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    Some(json!({
        "id": id,
        "name": benefit.name,
        "limit": format!("{} / {}", money(value), period(cadence, months)),
        "cap": value,
        "cadence": cadence,
        "matches": matches,
        "creditMatches": any_of(&benefit.descriptor_examples),
        "instruction": if instruction.is_empty() { "See the issuer's terms.".to_owned() } else { instruction },
        "effectiveUntil": benefit.end_date.as_deref().and_then(|date| date.get(..10)).and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
        "url": card.url.clone().filter(|url| url.starts_with("https://")).unwrap_or_else(|| CATALOG_HOME.to_owned()),
        "issuerCredits": !benefit.in_app,
        "estimateFromPurchases": benefit.in_app && !benefit.match_keywords.is_empty(),
        "alignment": if benefit.alignment.as_deref() == Some("anniversary") { "anniversary" } else { "calendar" },
        "renewalTerms": months.map(|months| vec![json!({ "above": 0.0, "months": months })]).unwrap_or_default(),
        "category": category(benefit),
        "sources": [{ "kind": "catalog", "url": CATALOG_HOME, "quote": null, "retrieved": today }],
    }))
}

fn benefit_draft(benefit: &CatalogBenefit, id: &str) -> BenefitDraft {
    let mut terms = Vec::new();
    if let (Some(value), Some((cadence, months))) = (benefit.value, cadence(benefit)) {
        terms.push(check("cap", money(value), Confidence::Suggested, None));
        terms.push(check(
            "cadence",
            period(cadence, months),
            Confidence::Suggested,
            None,
        ));
        terms.push(check(
            "alignment",
            benefit
                .alignment
                .clone()
                .unwrap_or_else(|| "calendar".into()),
            Confidence::Suggested,
            None,
        ));
        terms.push(if benefit.match_keywords.is_empty() {
            check(
                "matches",
                benefit.name.clone(),
                Confidence::Unconfirmed,
                Some("The catalog gives no match words, so the benefit's name stands in."),
            )
        } else {
            check(
                "matches",
                benefit.match_keywords.join(", "),
                Confidence::Suggested,
                None,
            )
        });
        terms.push(if benefit.in_app {
            check(
                CREDIT,
                "None".into(),
                Confidence::Suggested,
                Some("Spent in the partner's app; no statement credit posts."),
            )
        } else if benefit.descriptor_examples.is_empty() {
            check(
                CREDIT,
                "Not known yet".into(),
                Confidence::Unconfirmed,
                Some("Learned from your credits once one posts."),
            )
        } else {
            check(
                CREDIT,
                benefit.descriptor_examples.join(", "),
                Confidence::Suggested,
                benefit
                    .match_verified
                    .then_some("The catalog's maintainers have seen this credit post."),
            )
        });
    }
    BenefitDraft {
        id: id.to_owned(),
        name: benefit.name.clone(),
        terms,
        credits_seen: 0,
        include: true,
    }
}

/// The term field for a benefit's credit wording, as the renderer names it.
pub(super) const CREDIT: &str = "creditMatches";

/// The catalog card's credits as drafted rules for `account_id`. A benefit whose patterns
/// don't compile is left out with a warning.
pub fn draft(card: &CatalogCardEntry, account: Option<&Account>, today: NaiveDate) -> CardDraft {
    let mut ids = BTreeSet::new();
    let mut rules = Vec::new();
    let mut benefits = Vec::new();
    let mut warnings = Vec::new();
    for benefit in &card.benefits {
        let base = slug(if benefit.id.is_empty() {
            &benefit.name
        } else {
            &benefit.id
        });
        let base = if base.is_empty() {
            "benefit".to_owned()
        } else {
            base
        };
        let mut id = base.clone();
        let mut n = 2;
        while !ids.insert(id.clone()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let Some(rule) = rule(benefit, &id, card, today) else {
            warnings.push(format!(
                "{} isn't tracked: it has no dollar allowance on a period Brief follows.",
                benefit.name
            ));
            continue;
        };
        if single(card, &rule).is_err() {
            warnings.push(format!(
                "{} was left out: its match words don't form a valid pattern.",
                benefit.name
            ));
            continue;
        }
        benefits.push(benefit_draft(benefit, &id));
        rules.push(rule);
    }
    let names = [
        Some(card.name.clone()),
        account.and_then(|account| account.official_name.clone()),
    ]
    .into_iter()
    .flatten()
    .map(|name| regex::escape(name.trim()))
    .filter(|name| !name.is_empty())
    .collect::<BTreeSet<_>>();
    let rules: CardRules = serde_json::from_value(json!({
        "product": slug(&card.id),
        "name": card.name,
        "version": card.last_verified.clone().unwrap_or_else(|| today.to_string()),
        "termsReviewed": card.last_verified.clone().unwrap_or_else(|| today.to_string()),
        "annualFee": card.annual_fee,
        "annualFeeMatches": card.annual_fee.map(|_| "annual (?:membership )?fee"),
        "officialNames": format!("^(?:{})$", names.into_iter().collect::<Vec<_>>().join("|")),
        "benefits": rules,
        "catalog": { "id": card.id, "hash": card.hash },
    }))
    .expect("drafted rules have the shape CardRules reads");
    CardDraft {
        account_id: account
            .map(|account| account.id.clone())
            .unwrap_or_default(),
        catalog: Some(summary(card)),
        candidates: Vec::new(),
        rules,
        benefits,
        terms_url: None,
        sentences: Vec::new(),
        unmatched_credits: Vec::new(),
        warnings,
    }
}

fn single(card: &CatalogCardEntry, rule: &Value) -> Result<RuleSet, String> {
    let rules: CardRules = serde_json::from_value(json!({
        "product": "check", "name": card.name, "version": "", "termsReviewed": "",
        "benefits": [rule],
    }))
    .map_err(|error| error.to_string())?;
    RuleSet::new(rules)
}
