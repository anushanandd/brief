//! Card benefit rules. The rule data is versioned in `amex-platinum.json`; this module only
//! interprets it. Matching is deliberately conservative: unknown issuer descriptors stay
//! unverified rather than being inferred as credits from merchant names.
//!
//! The card feed never links a credit to the purchase that earned it, so `attribution`
//! pairs them from amounts and dates and places each credit in the window of its purchase.
//! A credit that can't be placed counts as a range across the two windows it may belong to.
mod attribution;
mod windows;

use std::{collections::BTreeMap, sync::LazyLock};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::calendar::{self, NamedZone, Zone};
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units};
use attribution::CreditLag;
use windows::benefit_window;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
    fn is_calendar(self) -> bool {
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
    rules: Vec<Rule>,
}

/// Issuer-posted credits set against the card's annual fee for the current membership year.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeeRecovery {
    pub annual_fee: f64,
    /// Posted credits net of reversals since `since`, for benefits that are not hidden and
    /// purchases that were yours.
    pub recovered: f64,
    /// Credits in the same period on purchases made for someone else: the allowance was used,
    /// but the saving went to them.
    pub for_others: f64,
    pub since: NaiveDate,
    /// `fee-charge` when the year starts at the fee's posting in the saved history, otherwise
    /// `calendar-year`, because the feed does not report the card's anniversary.
    pub basis: &'static str,
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
                    .map(|name| NamedZone::parse(name).expect("benefit time zones are supported")),
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Credit,
    Reversal,
    Purchase,
    Unverified,
}

/// What became of a qualifying purchase's credit, as of the snapshot day.
#[derive(Clone, Debug, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenefitActivity {
    pub id: String,
    pub merchant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub amount: f64,
    pub pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub date: NaiveDate,
    pub benefit_id: String,
    pub benefit_name: String,
    pub kind: ActivityKind,
    /// The purchase a credit offsets, the credit a reversal undoes, or, on a purchase, the
    /// credit that offset it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_id: Option<String>,
    /// For a posted credit or reversal, the first day of the window it counts toward.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_window: Option<NaiveDate>,
    /// When the feed can't establish the window, the earlier window it may count toward.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternate_credit_window: Option<NaiveDate>,
    /// For a posted qualifying purchase on an issuer-credit benefit.
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct AmountRange {
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenefitOption {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LastCredit {
    pub amount: f64,
    pub date: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenefitWindow {
    pub id: String,
    pub name: String,
    pub limit: String,
    pub cap: f64,
    pub cadence: Cadence,
    pub instruction: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_note: Option<String>,
    /// Some credits may belong to this window or the previous one.
    pub period_uncertain: bool,
    pub snapshot_before_window: bool,
    pub posting_delay_days: i64,
    /// How long this benefit's credits take to post: learned from paired credits when there
    /// is enough evidence, otherwise the terms' limit.
    pub credit_lag_days: i64,
    pub credit_lag_learned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_check: Option<String>,
    pub status: &'static str,
    /// The card has qualifying purchases or credits for this benefit in its history.
    pub engaged: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_range: Option<AmountRange>,
    /// Unused allowance smaller than the smallest qualifying charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unusable_remainder: Option<f64>,
    pub activity: Vec<BenefitActivity>,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub window_label: String,
    pub days_remaining: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_credit: Option<LastCredit>,
    pub reset: String,
}

/// Whether a closed window's unused allowance is known.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeState {
    /// Every credit for the window should have posted.
    Settled,
    /// Credits for the window may still post.
    Settling,
    /// The saved history starts after the window began.
    PartialHistory,
    /// Use doesn't appear on the card, as with Uber Cash spent before a charge.
    NotVisible,
    /// The rules don't estimate this benefit's unused allowance.
    NotTracked,
}

/// What a closed window's unused allowance counts as: missed when the card used the benefit,
/// unclaimed when it never did, still settling while credits may post, or unknown when the card
/// history can't say.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Missed,
    Unclaimed,
    Settling,
    Unknown,
}

impl Verdict {
    fn of(state: OutcomeState, engaged: bool) -> Option<Self> {
        match state {
            OutcomeState::Settled if engaged => Some(Self::Missed),
            OutcomeState::Settled => Some(Self::Unclaimed),
            OutcomeState::Settling => Some(Self::Settling),
            OutcomeState::PartialHistory | OutcomeState::NotVisible if engaged => {
                Some(Self::Unknown)
            }
            _ => None,
        }
    }
}

/// Windows with allowance left in the order to use them: dated windows first, soonest
/// deadline first, the most left first within a deadline; renewal and purchase benefits, which
/// have no calendar deadline, follow.
pub fn use_first(windows: &[BenefitWindow]) -> Vec<String> {
    let left = |window: &BenefitWindow| {
        window
            .remaining_range
            .as_ref()
            .map_or(0.0, |range| range.high)
    };
    let mut open: Vec<&BenefitWindow> =
        windows.iter().filter(|window| left(window) > 0.0).collect();
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

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowOutcome {
    pub window_end: NaiveDate,
    pub benefit_id: String,
    pub state: OutcomeState,
    /// The usable allowance left unused: the low bound when credits can't be placed.
    pub unused: Option<f64>,
    pub unused_high: Option<f64>,
    /// The card has used this benefit, so unused allowance is missed rather than unclaimed.
    pub engaged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
}

/// Posted credits net of reversals in a range, plus engaged allowances that expired unused.
/// The renderer sums the same way over the outcomes it receives.
#[cfg(test)]
pub fn credit_summary(
    activity: &[BenefitActivity],
    outcomes: &[WindowOutcome],
    start: NaiveDate,
    end: NaiveDate,
) -> (f64, f64) {
    let earned = activity
        .iter()
        .filter(|item| item.date >= start && item.date <= end && item.is_posted_credit())
        .map(|item| minor_units(item.amount))
        .sum::<i64>();
    let missed = outcomes
        .iter()
        .filter(|outcome| {
            outcome.engaged && outcome.window_end >= start && outcome.window_end <= end
        })
        .filter_map(|outcome| outcome.unused)
        .map(minor_units)
        .sum::<i64>();
    (from_minor(earned), from_minor(missed))
}

#[cfg(test)]
mod tests;
