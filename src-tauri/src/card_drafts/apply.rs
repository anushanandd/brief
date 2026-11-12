//! Applying the open catalog's rules without asking. A credit card with no rules gets its
//! catalog card's rules when the match is clear; a close call waits for the user to pick one.
//! Rules Brief applied follow the catalog, keeping a changed allowance as a dated change; rules
//! the user edited or imported are never changed here.
use chrono::NaiveDate;

use super::{catalog, history};
use crate::finance_contract::{Account, Transaction};
use crate::insights::benefits::{
    AllowanceChange, CardRule, CardRules, Catalog, RuleOrigin, RuleSet,
};
use crate::providers::card_catalog::{CardCatalog, CatalogBenefit, CatalogCardEntry};
use crate::settings::{CardChoice, CardChoiceOption, FinanceSettings};

/// A clear match names the card fully (every distinctive word of its name is among the
/// account's names) and no other card scores within `MARGIN`, the issuer's bonus, of it.
pub const MARGIN: f64 = 0.25;

#[derive(Debug)]
pub enum Match<'a> {
    Clear(&'a CatalogCardEntry),
    Close(Vec<&'a CatalogCardEntry>),
    None,
}

pub fn best<'a>(catalog: &'a CardCatalog, account: &Account) -> Match<'a> {
    let scored = catalog::scored(catalog, account);
    let Some((top, card)) = scored.first() else {
        return Match::None;
    };
    let runner_up = scored.get(1).map_or(f64::NEG_INFINITY, |(score, _)| *score);
    if catalog::named(card, account) && top - runner_up >= MARGIN {
        Match::Clear(card)
    } else {
        Match::Close(
            scored
                .iter()
                .filter(|(score, _)| top - score < MARGIN)
                .map(|(_, card)| *card)
                .collect(),
        )
    }
}

/// Whether a card's benefits are already read against saved rules.
fn covered(settings: &FinanceSettings, account: &Account) -> bool {
    let cards = Catalog::from_settings(settings);
    settings
        .benefit_cards
        .get(&account.id)
        .and_then(|product| cards.product(product))
        .is_some()
        || cards
            .named_product(account.official_name.as_deref())
            .is_some()
        || (settings.spending_account_id.as_deref() == Some(account.id.as_str())
            && cards.only().is_some())
}

fn summary(card: &CatalogCardEntry) -> CardChoiceOption {
    CardChoiceOption {
        id: card.id.clone(),
        name: card.name.clone(),
        issuer: card.issuer.clone(),
    }
}

/// A card's catalog rules, drafted and checked against its credits, ready to save.
fn drafted(
    card: &CatalogCardEntry,
    account: Option<&Account>,
    transactions: &[Transaction],
    today: NaiveDate,
) -> Option<CardRules> {
    let mut draft = catalog::draft(card, account, today);
    history::observe(&mut draft, transactions);
    let mut rules = draft.rules;
    rules.origin = RuleOrigin::Catalog;
    rules.applied = Some(today);
    (!rules.benefits.is_empty() && RuleSet::new(rules.clone()).is_ok()).then_some(rules)
}

/// Saves a catalog card's rules for `account` and assigns them to it. The spending card, when
/// it was read against the only saved product, is pinned to that product first, since a
/// second product ends that fallback.
pub fn adopt(
    settings: &mut FinanceSettings,
    account: &Account,
    card: &CatalogCardEntry,
    transactions: &[Transaction],
    today: NaiveDate,
) -> bool {
    let Some(rules) = drafted(card, Some(account), transactions, today) else {
        return false;
    };
    let only = Catalog::from_settings(settings)
        .only()
        .map(|rules| rules.product.clone());
    if let (Some(spending), Some(only)) = (settings.spending_account_id.clone(), only) {
        settings.benefit_cards.entry(spending).or_insert(only);
    }
    let product = rules.product.clone();
    if !settings
        .card_rules
        .iter()
        .any(|saved| saved.product == product)
    {
        settings.card_rules.push(rules);
    }
    settings.benefit_cards.insert(account.id.clone(), product);
    settings
        .card_choices
        .retain(|choice| choice.account_id != account.id);
    true
}

/// A fresh draft of catalog-applied rules, keeping each changed allowance as a dated change
/// from `today` so earlier periods keep the terms they had.
pub fn merged(saved: &CardRules, mut fresh: CardRules, today: NaiveDate) -> CardRules {
    for rule in &mut fresh.benefits {
        let Some(before) = saved.benefits.iter().find(|old| old.id == rule.id) else {
            continue;
        };
        let current = before
            .allowance_changes
            .iter()
            .max_by_key(|change| change.from)
            .map_or((before.cap, &before.limit, before.cadence), |change| {
                (change.cap, &change.limit, change.cadence)
            });
        if current == (rule.cap, &rule.limit, rule.cadence) {
            rule.allowance_changes = before.allowance_changes.clone();
            continue;
        }
        let change = AllowanceChange {
            from: today,
            cap: rule.cap,
            limit: rule.limit.clone(),
            cadence: rule.cadence,
        };
        rule.cap = before.cap;
        rule.limit = before.limit.clone();
        rule.cadence = before.cadence;
        rule.allowance_changes = before
            .allowance_changes
            .iter()
            .filter(|earlier| earlier.from < today)
            .cloned()
            .collect();
        rule.allowance_changes.push(change);
    }
    fresh
}

/// The credit card a product's rules are read for: the one assigned it, else the spending card.
fn account_for<'a>(
    settings: &FinanceSettings,
    product: &str,
    accounts: &'a [Account],
) -> Option<&'a Account> {
    let id = settings
        .benefit_cards
        .iter()
        .find(|(_, assigned)| assigned.as_str() == product)
        .map(|(id, _)| id.clone())
        .or_else(|| settings.spending_account_id.clone())?;
    accounts.iter().find(|account| account.id == id)
}

/// The catalog card the user's rules describe: the one they were drafted from, else a clear
/// match for their names.
fn card_for<'a>(rules: &CardRules, catalog: &'a CardCatalog) -> Option<&'a CatalogCardEntry> {
    if let Some(card) = rules
        .catalog
        .as_ref()
        .and_then(|reference| catalog.card(&reference.id))
    {
        return Some(card);
    }
    let named = Account {
        official_name: Some(rules.name.clone()),
        name: rules.product.replace('-', " "),
        r#type: "credit".into(),
        ..Default::default()
    };
    match best(catalog, &named) {
        Match::Clear(card) => Some(card),
        _ => None,
    }
}

/// The catalog benefit a saved rule is: the same id, else the same name, else the one sharing
/// the most distinctive words with its name and pattern, when no other ties it.
fn benefit_for<'a>(rule: &CardRule, card: &'a CatalogCardEntry) -> Option<&'a CatalogBenefit> {
    if let Some(same) = card.benefits.iter().find(|benefit| benefit.id == rule.id) {
        return Some(same);
    }
    let name = catalog::words(&rule.name);
    if let Some(same) = card
        .benefits
        .iter()
        .find(|benefit| !name.is_empty() && catalog::words(&benefit.name) == name)
    {
        return Some(same);
    }
    let own = name
        .union(&catalog::words(&rule.matches))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut scored = card
        .benefits
        .iter()
        .map(|benefit| {
            let mut theirs = catalog::words(&benefit.name);
            for keyword in &benefit.match_keywords {
                theirs.extend(catalog::words(keyword));
            }
            (own.intersection(&theirs).count(), benefit)
        })
        .filter(|(shared, _)| *shared > 0)
        .collect::<Vec<_>>();
    scored.sort_by_key(|(shared, _)| std::cmp::Reverse(*shared));
    match scored.as_slice() {
        [(_, benefit)] => Some(*benefit),
        [(top, benefit), (next, _), ..] if top > next => Some(*benefit),
        _ => None,
    }
}

/// The user's rules with display details they lack filled from the catalog: each benefit's
/// category. Terms are never changed, and nothing the user set is replaced.
pub fn with_display_details(rules: &CardRules, catalog: &CardCatalog) -> Option<CardRules> {
    let card = card_for(rules, catalog)?;
    let mut next = rules.clone();
    let mut filled = false;
    for rule in next
        .benefits
        .iter_mut()
        .filter(|rule| rule.category.is_none())
    {
        if let Some(category) = benefit_for(rule, card).and_then(catalog::category) {
            rule.category = Some(category);
            filled = true;
        }
    }
    filled.then_some(next)
}

/// Whether the catalog is worth downloading: it is on and there is a credit card to read.
pub fn wanted(settings: &FinanceSettings, accounts: &[Account]) -> bool {
    !settings.card_catalog_off && accounts.iter().any(|account| account.r#type == "credit")
}

/// Settings with the catalog applied: catalog rules brought current, and each uncovered credit
/// card given its catalog card's rules or, when the match is close, a choice to make.
pub fn apply(
    settings: &FinanceSettings,
    accounts: &[Account],
    transactions: &[Transaction],
    catalog: &CardCatalog,
    today: NaiveDate,
) -> FinanceSettings {
    let mut next = settings.clone();
    if next.card_catalog_off {
        return next;
    }
    let mut refreshed = next.card_rules.clone();
    for rules in &mut refreshed {
        if rules.origin == RuleOrigin::User {
            if let Some(filled) = with_display_details(rules, catalog) {
                *rules = filled;
            }
            continue;
        }
        let Some(card) = rules
            .catalog
            .as_ref()
            .and_then(|reference| catalog.card(&reference.id))
            .filter(|card| rules.catalog.as_ref().is_some_and(|r| r.hash != card.hash))
        else {
            continue;
        };
        let account = account_for(&next, &rules.product, accounts);
        if let Some(fresh) = drafted(card, account, transactions, today) {
            *rules = merged(rules, fresh, today);
        }
    }
    next.card_rules = refreshed;
    for account in accounts.iter().filter(|account| account.r#type == "credit") {
        if covered(&next, account) {
            next.card_choices
                .retain(|choice| choice.account_id != account.id);
            continue;
        }
        match best(catalog, account) {
            Match::Clear(card) => {
                adopt(&mut next, account, card, transactions, today);
            }
            Match::Close(cards) => {
                next.card_choices
                    .retain(|choice| choice.account_id != account.id);
                next.card_choices.push(CardChoice {
                    account_id: account.id.clone(),
                    candidates: cards.into_iter().map(summary).collect(),
                });
            }
            Match::None => next
                .card_choices
                .retain(|choice| choice.account_id != account.id),
        }
    }
    next
}
