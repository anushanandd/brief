//! Card benefit rules. The rule data is versioned in `amex-platinum.json`; this module only
//! interprets it. Matching is deliberately conservative: unknown issuer descriptors stay
//! unverified rather than being inferred as credits from merchant names.
//!
//! The card feed never links a credit to the purchase that earned it, so `attribution`
//! pairs them from amounts and dates and places each credit in the window of its purchase.
//! A credit that can't be placed counts as a range across the two windows it may belong to.
mod attribution;
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
pub use history::{card_years, BenefitHistory, BenefitReview, CardBenefitYear, PlannedCharge};
use windows::benefit_window;

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

#[derive(Debug, Deserialize)]
struct MerchantAmount {
    merchant: String,
    amount: f64,
}

#[derive(Debug, Deserialize)]
struct DatedRequirement {
    pattern: String,
    date: NaiveDate,
}

#[derive(Clone, Debug, Deserialize)]
struct AllowanceChange {
    from: NaiveDate,
    cap: f64,
    limit: String,
    cadence: Cadence,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleData {
    id: String,
    name: String,
    limit: String,
    cap: f64,
    cadence: Cadence,
    matches: String,
    #[serde(default)]
    credit_matches: Option<String>,
    #[serde(default)]
    excludes: Option<String>,
    #[serde(default)]
    merchant_amount_matches: Vec<MerchantAmount>,
    #[serde(default)]
    exclude_merchant_amounts: Vec<MerchantAmount>,
    #[serde(default)]
    match_requires_before: Option<DatedRequirement>,
    instruction: String,
    #[serde(default)]
    effective_from: Option<NaiveDate>,
    #[serde(default)]
    effective_until: Option<NaiveDate>,
    url: String,
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    deadline_note: Option<String>,
    #[serde(default)]
    allowance_changes: Vec<AllowanceChange>,
    #[serde(default = "yes")]
    issuer_credits: bool,
    #[serde(default = "yes")]
    track_remaining: bool,
    #[serde(default)]
    month_caps: BTreeMap<u32, f64>,
    #[serde(default)]
    estimate_from_purchases: bool,
    /// The longest wait the terms allow between a purchase and its credit.
    #[serde(default = "default_posting_delay")]
    posting_delay_days: i64,
    /// How long credits usually take to post, used until this card's own credits show it.
    #[serde(default = "default_typical_delay")]
    typical_delay_days: i64,
    /// Spent automatically by a subscription, so there is nothing to do before the deadline
    /// except check that the credit posted.
    #[serde(default)]
    recurring: bool,
    /// The smallest qualifying charge. A smaller remainder can't be used.
    #[serde(default)]
    minimum_charge: Option<f64>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleFile {
    /// Identifies the card product these rules describe.
    product: String,
    /// The card's name as shown when choosing a product.
    name: String,
    version: String,
    terms_reviewed: String,
    /// The Basic Card's annual fee and the descriptor its charge posts under.
    annual_fee: Option<f64>,
    annual_fee_matches: Option<String>,
    /// The issuer's name for the product as Plaid reports it in `official_name`, which
    /// recognizes the card without the user assigning it, and the names of other products
    /// that would otherwise match.
    official_names: Option<String>,
    official_name_excludes: Option<String>,
    benefits: Vec<RuleData>,
}

struct Rule {
    data: RuleData,
    matches: Regex,
    credit_matches: Option<Regex>,
    excludes: Option<Regex>,
    requires_before: Option<(Regex, NaiveDate)>,
    zone: Option<NamedZone>,
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

    /// The window containing `day`, with the allowance in effect on `day`.
    fn period(&self, day: NaiveDate) -> Period {
        let definition = &self.data;
        let allowance = self.allowance(day);
        let cadence = allowance.map_or(definition.cadence, |change| change.cadence);
        let (mut start, end) = benefit_window(day, cadence);
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
    Regex::new(&format!("(?i){source}")).expect("benefit patterns are valid")
}

pub struct RuleSet {
    pub product: String,
    pub name: String,
    pub version: String,
    pub terms_reviewed: String,
    annual_fee: Option<f64>,
    annual_fee_matches: Option<Regex>,
    official_names: Option<Regex>,
    official_name_excludes: Option<Regex>,
    rules: Vec<Rule>,
}

impl RuleSet {
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

pub static PLATINUM: LazyLock<RuleSet> = LazyLock::new(|| {
    let file: RuleFile = serde_json::from_str(include_str!("amex-platinum.json"))
        .expect("benefit rule data is valid");
    RuleSet {
        product: file.product,
        name: file.name,
        version: file.version,
        terms_reviewed: file.terms_reviewed,
        annual_fee: file.annual_fee,
        annual_fee_matches: file.annual_fee_matches.as_deref().map(pattern),
        official_names: file.official_names.as_deref().map(pattern),
        official_name_excludes: file.official_name_excludes.as_deref().map(pattern),
        rules: file
            .benefits
            .into_iter()
            .map(|data| Rule {
                matches: pattern(&data.matches),
                credit_matches: data.credit_matches.as_deref().map(pattern),
                excludes: data.excludes.as_deref().map(pattern),
                requires_before: data
                    .match_requires_before
                    .as_ref()
                    .map(|requirement| (pattern(&requirement.pattern), requirement.date)),
                zone: data
                    .time_zone
                    .as_deref()
                    .map(|name| NamedZone::parse(name).expect("benefit time zones are IANA names")),
                data,
            })
            .collect(),
    }
});

/// Every card product with rule data. Adding a card is adding its rule file here.
pub fn products() -> Vec<&'static RuleSet> {
    vec![&*PLATINUM]
}

/// The rule data for a product id, or none when the id names no known card.
pub fn product(id: &str) -> Option<&'static RuleSet> {
    products().into_iter().find(|rules| rules.product == id)
}

/// The product whose official name the issuer reports for a card, when exactly one does.
pub fn named_product(official_name: Option<&str>) -> Option<&'static RuleSet> {
    let official_name = official_name?;
    let mut named = products()
        .into_iter()
        .filter(|rules| rules.names(official_name));
    let first = named.next()?;
    named.next().is_none().then_some(first)
}

/// The product a card is tracked as before the user assigns one.
pub const DEFAULT_PRODUCT: &str = "amex-platinum";

static REFUND: LazyLock<Regex> = LazyLock::new(|| pattern(r"\brefund\b"));
/// The issuer's own statement-credit wording, which also covers benefits added after the
/// rule data was reviewed.
static ISSUER_CREDIT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"^\s*(?:amex|platinum)\b.*\b(?:credit|reimbursement)\b"));

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
    pub limit: String,
    pub cap: f64,
    pub cadence: Cadence,
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
/// are left out; Uber Cash, whose use the card can't show, is listed until a charge suggests it
/// ran out.
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
