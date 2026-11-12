//! Card benefits: a card product's rules, and the engine that reads its activity against them.
//!
//! Rules are data (`CardRules`) saved in settings, imported from a file or drafted by research;
//! none are compiled in. `example-card.json` is a generic card for the preview and fixtures.
//!
//! An unknown issuer descriptor stays unverified rather than being guessed from a merchant's
//! name. The feed never links a credit to its purchase, so `attribution` pairs them by amount
//! and date; a credit that can't be placed counts as a range across two windows.
//!
//! A card is read against the product assigned to it, else the one its `official_name` names;
//! the spending card also falls back to the only saved product.
mod attribution;
mod brands;
mod history;
mod windows;

use std::{collections::BTreeMap, sync::LazyLock};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::calendar::{self, NamedZone, Zone};
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units};
use attribution::CreditLag;
pub use brands::brand_key;
pub use history::{card_years, BenefitHistory, BenefitReview, CardBenefitYear, PlannedCharge};
use windows::{anniversary_window, benefit_window};

/// A leftover under a dollar buys nothing, so a window that leaves only cents was used in
/// full. The cents still count as missed, so totals stay exact.
const NEGLIGIBLE_CENTS: i64 = 100;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Cadence {
    Monthly,
    Quarterly,
    Semiannual,
    Annual,
    Renewal,
    Purchase,
}

impl Cadence {
    pub fn is_calendar(self) -> bool {
        !matches!(self, Self::Renewal | Self::Purchase)
    }
}

/// A charge of exactly this amount at this merchant (lowercase).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
pub struct MerchantAmount {
    pub merchant: String,
    pub amount: f64,
}

/// A pattern a purchase must also match before `date`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
pub struct DatedRequirement {
    pub pattern: String,
    pub date: NaiveDate,
}

/// The allowance from `from` on, when the terms change it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
pub struct AllowanceChange {
    pub from: NaiveDate,
    pub cap: f64,
    pub limit: String,
    pub cadence: Cadence,
}

/// One benefit of a card product. Patterns are case-insensitive regular expressions matched
/// against a transaction's descriptors.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CardRule {
    pub id: String,
    pub name: String,
    /// The allowance as the terms state it, such as "$25 / month".
    pub limit: String,
    pub cap: f64,
    pub cadence: Cadence,
    /// A purchase this benefit covers.
    pub matches: String,
    /// The issuer's statement credit for it.
    #[serde(default)]
    pub credit_matches: Option<String>,
    #[serde(default)]
    pub excludes: Option<String>,
    #[serde(default)]
    pub merchant_amount_matches: Vec<MerchantAmount>,
    #[serde(default)]
    pub exclude_merchant_amounts: Vec<MerchantAmount>,
    #[serde(default)]
    pub match_requires_before: Option<DatedRequirement>,
    /// What to do to earn the credit, in the terms' words.
    pub instruction: String,
    #[serde(default)]
    pub effective_from: Option<NaiveDate>,
    #[serde(default)]
    pub effective_until: Option<NaiveDate>,
    /// Where the terms are published.
    pub url: String,
    /// An IANA zone, when the terms set the deadline in one.
    #[serde(default)]
    pub time_zone: Option<String>,
    #[serde(default)]
    pub deadline_note: Option<String>,
    #[serde(default)]
    pub allowance_changes: Vec<AllowanceChange>,
    /// Whether the issuer posts a statement credit; otherwise the benefit is spent elsewhere.
    #[serde(default = "yes")]
    pub issuer_credits: bool,
    #[serde(default = "yes")]
    pub track_remaining: bool,
    /// The cap for a calendar month (1–12), where the terms set a different one.
    #[serde(default)]
    pub month_caps: BTreeMap<u32, f64>,
    /// Use is estimated from qualifying purchases, because no credit posts.
    #[serde(default)]
    pub estimate_from_purchases: bool,
    /// The longest wait the terms allow between a purchase and its credit.
    #[serde(default = "default_posting_delay")]
    pub posting_delay_days: i64,
    /// How long credits usually take to post, used until this card's own credits show it.
    #[serde(default = "default_typical_delay")]
    pub typical_delay_days: i64,
    /// Spent automatically by a subscription, so there is nothing to do before the deadline
    /// except check that the credit posted.
    #[serde(default)]
    pub recurring: bool,
    /// The smallest qualifying charge. A smaller remainder can't be used.
    #[serde(default)]
    pub minimum_charge: Option<f64>,
    /// For a renewal benefit, how long a credit lasts by what it paid: each term covers a
    /// credit of more than `above`. Without one, a credit renews after four years.
    #[serde(default)]
    pub renewal_terms: Vec<RenewalTerm>,
    /// Whether periods follow the calendar or the cardmember year.
    #[serde(default)]
    pub alignment: Alignment,
    /// Where each term came from, so a saved rule can show its evidence.
    #[serde(default)]
    pub sources: Vec<RuleSource>,
    /// The merchant's domain, for its logo; without one, the logo follows where it was used.
    #[serde(default)]
    pub logo_domain: Option<String>,
    /// What kind of benefit it is (dining, rideshare, hotel…), for its icon.
    #[serde(default)]
    pub category: Option<String>,
}

/// Whether a benefit's periods follow the calendar or the cardmember year, which starts on
/// the card's anniversary.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    #[default]
    Calendar,
    Anniversary,
}

/// Where a term came from.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// The open card benefits catalog.
    Catalog,
    /// A sentence of the issuer's published terms.
    Terms,
    /// Credits that posted on the user's card.
    History,
    /// Typed or changed by the user.
    User,
}

/// One piece of evidence for a rule: what it says and where it was found.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuleSource {
    pub kind: SourceKind,
    #[serde(default)]
    pub url: Option<String>,
    /// The sentence or descriptor quoted.
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub retrieved: Option<NaiveDate>,
}

/// Which catalog entry a product's rules were drafted from, and its content hash then, so a
/// later change in the catalog can be shown as a diff.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRef {
    pub id: String,
    pub hash: String,
}

/// A renewal credit of more than `above` dollars renews after `months`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
pub struct RenewalTerm {
    pub above: f64,
    pub months: i64,
}

fn yes() -> bool {
    true
}

fn default_posting_delay() -> i64 {
    56
}

fn default_typical_delay() -> i64 {
    7
}

/// A card product's benefit rules, as saved in settings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CardRules {
    /// Identifies the card product these rules describe.
    pub product: String,
    /// The card's name as shown when choosing a product.
    pub name: String,
    pub version: String,
    /// When the terms were last read against the issuer's.
    pub terms_reviewed: String,
    /// The annual fee and the descriptor its charge posts under.
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub annual_fee_matches: Option<String>,
    /// The issuer's name for the product as Plaid reports it in `official_name`, which
    /// recognizes the card without the user assigning it, and the names of other products
    /// that would otherwise match.
    #[serde(default)]
    pub official_names: Option<String>,
    #[serde(default)]
    pub official_name_excludes: Option<String>,
    /// The issuer's own statement-credit wording, which also covers benefits added after the
    /// rules were written.
    #[serde(default)]
    pub issuer_credit_matches: Option<String>,
    pub benefits: Vec<CardRule>,
    /// The catalog entry these rules were drafted from, if any.
    #[serde(default)]
    pub catalog: Option<CatalogRef>,
    /// The issuer's terms page these rules were checked against, and its text's hash then.
    #[serde(default)]
    pub terms_url: Option<String>,
    #[serde(default)]
    pub terms_hash: Option<String>,
    /// Whether Brief applied these rules from the catalog, which it keeps current, or the user
    /// edited or imported them, which it never changes.
    #[serde(default)]
    pub origin: RuleOrigin,
    /// When Brief last applied them from the catalog.
    #[serde(default)]
    pub applied: Option<NaiveDate>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub enum RuleOrigin {
    Catalog,
    #[default]
    User,
}

#[derive(Clone)]
struct Rule {
    data: CardRule,
    matches: Regex,
    credit_matches: Option<Regex>,
    excludes: Option<Regex>,
    requires_before: Option<(Regex, NaiveDate)>,
    zone: Option<NamedZone>,
    /// For an anniversary-aligned rule read against one card, the month and day its
    /// cardmember year starts.
    anniversary: Option<(u32, u32)>,
    /// An anniversary-aligned rule whose card has no annual fee charge saved, so its periods
    /// fall back to the calendar.
    anniversary_unknown: bool,
}

/// One allowance window of a rule, identified by its first day.
#[derive(Clone, Debug, PartialEq)]
struct Period {
    start: NaiveDate,
    end: NaiveDate,
    cadence: Cadence,
    cap: f64,
    limit: String,
}

impl Rule {
    fn zone<'a>(&'a self, fallback: &'a dyn Zone) -> &'a dyn Zone {
        match &self.zone {
            Some(named) => named,
            None => fallback,
        }
    }

    fn allowance(&self, day: NaiveDate) -> Option<&AllowanceChange> {
        self.data
            .allowance_changes
            .iter()
            .rev()
            .find(|change| change.from <= day)
    }

    /// The calendar window, or the cardmember-year window for an anniversary-aligned rule
    /// whose anniversary is known. Monthly periods always follow the calendar.
    fn window(&self, day: NaiveDate, cadence: Cadence) -> (NaiveDate, NaiveDate) {
        match self.anniversary {
            Some(anchor)
                if matches!(
                    cadence,
                    Cadence::Annual | Cadence::Semiannual | Cadence::Quarterly
                ) =>
            {
                anniversary_window(day, cadence, anchor)
            }
            _ => benefit_window(day, cadence),
        }
    }

    /// The window containing `day`, with the allowance in effect on `day`.
    fn period(&self, day: NaiveDate) -> Period {
        let definition = &self.data;
        let allowance = self.allowance(day);
        let cadence = allowance.map_or(definition.cadence, |change| change.cadence);
        let (mut start, end) = self.window(day, cadence);
        // A changed cadence starts a new allowance, not a retroactive application of the new cap.
        let first_day = match allowance {
            Some(change) if change.cadence != definition.cadence => Some(change.from),
            _ => definition.effective_from,
        };
        if let Some(first_day) = first_day.filter(|first| *first > start) {
            start = first_day;
        }
        let cap = definition
            .month_caps
            .get(&day.month())
            .copied()
            .unwrap_or_else(|| allowance.map_or(definition.cap, |change| change.cap));
        Period {
            start,
            end,
            cadence,
            cap,
            limit: allowance.map_or(definition.limit.clone(), |change| change.limit.clone()),
        }
    }

    /// The window before `period`, when the benefit existed then.
    fn previous_period(&self, period: &Period) -> Option<Period> {
        let has_prior = self
            .data
            .effective_from
            .is_none_or(|from| from < period.start);
        has_prior.then(|| self.period(period.start - Duration::days(1)))
    }

    /// Applies the smallest qualifying charge: a smaller remainder can't be used.
    fn usable(&self, remaining: i64) -> i64 {
        match self.data.minimum_charge {
            Some(minimum) if remaining < minor_units(minimum) => 0,
            _ => remaining,
        }
    }

    /// What is left to spend now: a usable remainder, unless it is only cents.
    fn spendable(&self, remaining: i64) -> i64 {
        match self.usable(remaining) {
            cents if cents < NEGLIGIBLE_CENTS => 0,
            cents => cents,
        }
    }
}

fn pattern(source: &str) -> Regex {
    Regex::new(&format!("(?i){source}")).expect("built-in benefit patterns are valid")
}

/// A saved pattern, which a rule file may get wrong: an invalid one fails the whole file.
fn saved_pattern(product: &str, source: &str) -> Result<Regex, String> {
    Regex::new(&format!("(?i){source}"))
        .map_err(|_| format!("{product}'s rules have an invalid pattern: {source}"))
}

/// A card product's rules, ready to read activity against.
#[derive(Clone)]
pub struct RuleSet {
    pub product: String,
    pub name: String,
    pub version: String,
    pub terms_reviewed: String,
    annual_fee: Option<f64>,
    annual_fee_matches: Option<Regex>,
    official_names: Option<Regex>,
    official_name_excludes: Option<Regex>,
    issuer_credit_matches: Option<Regex>,
    rules: Vec<Rule>,
}

impl RuleSet {
    /// Compiles a product's saved rules, or says what is wrong with them.
    pub fn new(file: CardRules) -> Result<Self, String> {
        let product = file.product.clone();
        let optional = |source: Option<&str>| {
            source
                .map(|source| saved_pattern(&product, source))
                .transpose()
        };
        let rules = file
            .benefits
            .into_iter()
            .map(|data| {
                Ok(Rule {
                    matches: saved_pattern(&product, &data.matches)?,
                    credit_matches: optional(data.credit_matches.as_deref())?,
                    excludes: optional(data.excludes.as_deref())?,
                    requires_before: data
                        .match_requires_before
                        .as_ref()
                        .map(|requirement| {
                            Ok::<_, String>((
                                saved_pattern(&product, &requirement.pattern)?,
                                requirement.date,
                            ))
                        })
                        .transpose()?,
                    zone: data
                        .time_zone
                        .as_deref()
                        .map(|name| {
                            NamedZone::parse(name).ok_or_else(|| {
                                format!("{product}'s rules name an unknown time zone: {name}")
                            })
                        })
                        .transpose()?,
                    data,
                    anniversary: None,
                    anniversary_unknown: false,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            annual_fee_matches: optional(file.annual_fee_matches.as_deref())?,
            official_names: optional(file.official_names.as_deref())?,
            official_name_excludes: optional(file.official_name_excludes.as_deref())?,
            issuer_credit_matches: optional(file.issuer_credit_matches.as_deref())?,
            product: file.product,
            name: file.name,
            version: file.version,
            terms_reviewed: file.terms_reviewed,
            annual_fee: file.annual_fee,
            rules,
        })
    }
}

impl RuleSet {
    /// These rules read against one card: an anniversary-aligned benefit's year starts on the
    /// day its annual fee last posted, or follows the calendar when no fee charge is saved.
    pub fn for_card(&self, transactions: &[&Transaction]) -> Self {
        let mut card = self.clone();
        if !card
            .rules
            .iter()
            .any(|rule| rule.data.alignment == Alignment::Anniversary)
        {
            return card;
        }
        let anniversary = self.annual_fee_matches.as_ref().and_then(|matches| {
            transactions
                .iter()
                .filter(|transaction| !transaction.pending && transaction.amount < 0.0)
                .filter(|transaction| {
                    matches.is_match(&format!(
                        "{} {}",
                        transaction.merchant,
                        transaction.description.as_deref().unwrap_or("")
                    ))
                })
                .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
                .max()
                .map(|date| (date.month(), date.day()))
        });
        for rule in &mut card.rules {
            if rule.data.alignment == Alignment::Anniversary {
                rule.anniversary = anniversary;
                rule.anniversary_unknown = anniversary.is_none();
            }
        }
        card
    }

    /// Whether the issuer's official name for a card names this product.
    pub fn names(&self, official_name: &str) -> bool {
        self.official_names
            .as_ref()
            .is_some_and(|names| names.is_match(official_name.trim()))
            && !self
                .official_name_excludes
                .as_ref()
                .is_some_and(|excludes| excludes.is_match(official_name))
    }
}

/// Issuer-posted credits set against the card's annual fee for the current membership year.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FeeRecovery {
    pub annual_fee: f64,
    /// Posted credits net of reversals since `since` on purchases that were yours, hidden
    /// benefits included: hiding a benefit stops reminders, not the money it returned.
    pub recovered: f64,
    /// Money from elsewhere filed as repaying the fee charge since `since`: a bank's
    /// engagement bonus a program explains, or a refund link you made.
    pub offsets: f64,
    /// Whether `recovered` and `offsets` together reach the fee.
    pub covered: bool,
    /// How far `recovered` and `offsets` together are from the fee, short of it or past it.
    pub gap: f64,
    /// Credits in the same period on purchases made for someone else: the allowance was used,
    /// but the saving went to them.
    pub for_others: f64,
    pub since: NaiveDate,
    /// `fee-charge` when the year starts at the fee's posting in the saved history, otherwise
    /// `calendar-year`, because the feed does not report the card's anniversary, or
    /// `card-history` when the saved history starts after January.
    pub basis: &'static str,
    /// The fee year's last day: a year after the fee charge, or December 31 when the
    /// anniversary is unknown.
    pub through: NaiveDate,
    /// Credits still to come by `through` on your purchases: posted purchases waiting for
    /// theirs, then the charges the forecast expects, as each benefit period plans them.
    pub expected: f64,
}

/// The card products a read knows the rules of, from settings.
#[derive(Default)]
pub struct Catalog {
    products: Vec<RuleSet>,
}

impl Catalog {
    /// Compiles every product's rules, or says which is wrong.
    pub fn new(files: &[CardRules]) -> Result<Self, String> {
        Ok(Self {
            products: files
                .iter()
                .cloned()
                .map(RuleSet::new)
                .collect::<Result<_, _>>()?,
        })
    }

    /// The products saved in settings. Settings are checked when saved, so a file that doesn't
    /// compile now (from a build that read patterns differently) is left out, never fatal.
    pub fn from_settings(settings: &crate::settings::FinanceSettings) -> Self {
        Self {
            products: settings
                .card_rules
                .iter()
                .cloned()
                .filter_map(|file| RuleSet::new(file).ok())
                .collect(),
        }
    }

    /// The generic card the browser preview and fixtures read.
    #[cfg(test)]
    pub fn example() -> Self {
        Self::new(&[example_rules()]).expect("the example card's rules compile")
    }

    pub fn products(&self) -> &[RuleSet] {
        &self.products
    }

    /// The rules for a product id, or none when the id names no saved product.
    pub fn product(&self, id: &str) -> Option<&RuleSet> {
        self.products.iter().find(|rules| rules.product == id)
    }

    /// The product whose official name the issuer reports for a card, when exactly one does.
    pub fn named_product(&self, official_name: Option<&str>) -> Option<&RuleSet> {
        let official_name = official_name?;
        let mut named = self
            .products
            .iter()
            .filter(|rules| rules.names(official_name));
        let first = named.next()?;
        named.next().is_none().then_some(first)
    }

    /// The catalog's product, when it has exactly one.
    pub fn only(&self) -> Option<&RuleSet> {
        match self.products.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// Whether any product's rules call the text an issuer statement credit.
    pub fn is_issuer_credit(&self, text: &str) -> bool {
        self.products
            .iter()
            .any(|rules| rules.is_issuer_credit(text))
    }

    /// The benefit a credit belongs to, under the first product that recognizes it.
    pub fn credit_identity(&self, transaction: &Transaction) -> Option<(String, String)> {
        self.products
            .iter()
            .find_map(|rules| rules.credit_identity(transaction))
    }
}

/// The generic card's rules.
#[cfg(test)]
pub fn example_rules() -> CardRules {
    serde_json::from_str(include_str!("example-card.json")).expect("the example card parses")
}

static REFUND: LazyLock<Regex> = LazyLock::new(|| pattern(r"\brefund\b"));

fn merchant_amount(entries: &[MerchantAmount], transaction: &Transaction) -> bool {
    entries.iter().any(|entry| {
        minor_units(transaction.amount) == minor_units(entry.amount)
            && transaction.merchant.trim().to_lowercase() == entry.merchant
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Credit,
    Reversal,
    Purchase,
    Unverified,
}

/// What became of a qualifying purchase's credit, as of the snapshot day.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum PurchaseCredit {
    /// A posted credit was paired with this purchase.
    Credited { amount: f64 },
    /// No credit yet, and one usually posts by `by`.
    Expected { amount: f64, by: NaiveDate },
    /// Later than this benefit's credits usually post, but within the terms' limit.
    Late { amount: f64, by: NaiveDate },
    /// The terms' posting limit passed without a credit.
    NotCredited,
    /// No credit, and the benefit's credited purchases show charges like this don't earn one.
    Unlikely,
    /// Earlier credits already used the window's allowance.
    OverAllowance,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct BenefitActivity {
    pub id: String,
    pub merchant: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub amount: f64,
    pub pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    /// The transaction date: the issuer counts a purchase, and the credit it earns, toward the
    /// window the merchant processed it in, even when it posts in the next one.
    pub date: NaiveDate,
    /// When it reached the account, which times credits: how long they take, and when one is
    /// late or a closed window has settled.
    #[serde(skip)]
    pub posted: NaiveDate,
    /// The feed reported a transaction date apart from the posting date. Without one, `date`
    /// is the posting date and can't place a credit near a window's start.
    #[serde(skip)]
    pub dated: bool,
    pub benefit_id: String,
    pub benefit_name: String,
    pub kind: ActivityKind,
    /// The purchase a credit offsets, the credit a reversal undoes, or, on a purchase, the
    /// credit that offset it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub paired_id: Option<String>,
    /// For a posted credit or reversal, the first day of the window it counts toward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub credit_window: Option<NaiveDate>,
    /// When the feed can't establish the window, the earlier window it may count toward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alternate_credit_window: Option<NaiveDate>,
    /// For a posted qualifying purchase on an issuer-credit benefit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub credit: Option<PurchaseCredit>,
}

impl BenefitActivity {
    fn is_posted_credit(&self) -> bool {
        matches!(self.kind, ActivityKind::Credit | ActivityKind::Reversal) && !self.pending
    }

    fn awaits_credit(&self) -> bool {
        matches!(
            self.credit,
            Some(PurchaseCredit::Expected { .. } | PurchaseCredit::Late { .. })
        )
    }
}

/// An amount the evidence bounds but can't pin down. Equal bounds mean it is known.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS)]
pub struct AmountRange {
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BenefitOption {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
}

/// A benefit's logo, each a fallback for the one before: its domain (the rule's, else the brand
/// table's, else the website of the latest purchase it covered when that purchase has no logo),
/// that purchase's logo, and the brand word to look a logo up by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BenefitLogo {
    pub domain: Option<String>,
    pub url: Option<String>,
    pub brand: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct LastCredit {
    pub amount: f64,
    pub date: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BenefitWindow {
    pub id: String,
    pub name: String,
    /// The benefit's merchant domain, for its logo (see `BenefitLogo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_domain: Option<String>,
    /// The logo of the purchase it last covered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_url: Option<String>,
    /// The brand it names, to look a logo up by when neither of those loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub logo_brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    pub limit: String,
    pub cap: f64,
    pub cadence: Cadence,
    /// The benefit follows the cardmember year, but no annual fee charge is saved to date it,
    /// so its periods follow the calendar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anniversary_unknown: bool,
    pub instruction: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deadline_note: Option<String>,
    /// Some credits may belong to this window or the previous one.
    pub period_uncertain: bool,
    pub snapshot_before_window: bool,
    pub posting_delay_days: i64,
    /// How long this benefit's credits take to post: learned from paired credits when there
    /// is enough evidence, otherwise the usual delay from the rule data.
    pub credit_lag_days: i64,
    pub credit_lag_learned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub renewal_check: Option<String>,
    pub status: &'static str,
    /// A subscription spends it automatically; the only thing to check is that it posted.
    pub recurring: bool,
    /// The saved card history starts after this window began, so credits before this day are
    /// missing and the remaining range runs from zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub history_from: Option<NaiveDate>,
    /// Credits, net of reversals, placed in this window: by purchase date when paired,
    /// otherwise by posting date.
    pub credited_amount: f64,
    /// Credits, net of reversals, that may count toward this window or an adjacent one.
    pub uncertain_credit_amount: f64,
    /// Credits that qualifying purchases in this window are still waiting for.
    pub awaiting_credit_amount: f64,
    pub estimated_credit_amount: Option<f64>,
    /// The usable unused allowance when it is known exactly.
    pub remaining_amount: Option<f64>,
    /// The usable unused allowance, bounded, whenever it is tracked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remaining_range: Option<AmountRange>,
    /// Unused allowance smaller than the smallest qualifying charge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unusable_remainder: Option<f64>,
    pub activity: Vec<BenefitActivity>,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub window_label: String,
    pub days_remaining: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_credit: Option<LastCredit>,
    pub reset: String,
}

/// Windows with allowance left in the order to use them: dated windows first, soonest
/// deadline first, the most left first within a deadline; renewal and purchase benefits, which
/// have no calendar deadline, follow. Subscriptions spend their credits on their own, so they
/// are left out; a benefit spent outside the card, whose use the card can't show, is listed
/// until a charge suggests it ran out.
pub fn use_first(windows: &[BenefitWindow]) -> Vec<String> {
    let left =
        |window: &BenefitWindow| match (&window.remaining_range, window.estimated_credit_amount) {
            (Some(range), _) => range.high,
            (None, Some(0.0)) => window.cap,
            _ => 0.0,
        };
    let mut open: Vec<&BenefitWindow> = windows
        .iter()
        .filter(|window| !window.recurring && left(window) > 0.0)
        .collect();
    open.sort_by(|a, b| {
        b.cadence
            .is_calendar()
            .cmp(&a.cadence.is_calendar())
            .then(a.window_end.cmp(&b.window_end))
            .then(left(b).total_cmp(&left(a)))
            .then_with(|| windows::collate(&a.name, &b.name))
    });
    open.into_iter().map(|window| window.id.clone()).collect()
}

/// Posted credits net of reversals in a range, plus allowances that settled unused in periods
/// that ended in it. The renderer sums the same way over the history it receives.
#[cfg(test)]
pub fn credit_summary(
    activity: &[BenefitActivity],
    history: &[BenefitHistory],
    start: NaiveDate,
    end: NaiveDate,
) -> (f64, f64) {
    let earned = activity
        .iter()
        .filter(|item| item.date >= start && item.date <= end && item.is_posted_credit())
        .map(|item| minor_units(item.amount))
        .sum::<i64>();
    let missed = history
        .iter()
        .flat_map(|benefit| &benefit.periods)
        .filter(|period| period.end >= start && period.end <= end)
        .filter_map(|period| period.unused)
        .map(|range| minor_units(range.low))
        .sum::<i64>();
    (from_minor(earned), from_minor(missed))
}

#[cfg(test)]
mod tests;
