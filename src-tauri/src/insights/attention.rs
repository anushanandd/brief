//! Heads up: the few things that need action now, at most three. A card payment due within a
//! week or overdue, and expected bills that exceed cash. Items carry
//! structured amounts and dates; the renderer writes the sentences so screen privacy can mask
//! every personal number.
//!
//! The inferred needs (a new series, a new price, fees charged, an unusual paycheck or charge)
//! are guesses with a confidence, so none is ever a notification: an inference doesn't
//! interrupt. A free trial converting is not detected: Plaid seldom reports the $0 charges it
//! would rest on, so it would be a new series with no evidence of a trial.
use chrono::NaiveDate;
use serde::Serialize;
use ts_rs::TS;

use super::anomalies::Anomalies;
use super::benefits::{BenefitReview, BenefitWindow, Cadence};
use super::cashline::CashTimeline;
use super::credit::{Credit, CLOSING_NOTICE_DAYS, HIGH_UTILIZATION};
use super::plan::Plan;
use super::statement::StatementCycle;
use crate::finance_contract::CreditLiability;

const MAX_ITEMS: usize = 3;
const PAYMENT_DUE_DAYS: i64 = 7;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Urgent,
    Attention,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Item {
    #[serde(rename_all = "camelCase")]
    PaymentDue {
        account_id: String,
        due: NaiveDate,
        days: i64,
        overdue: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        statement_balance: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        minimum_payment: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    CoverageShort {
        cash: f64,
        expected_outflows: f64,
        shortfall: f64,
    },
    /// Benefit matches on a card that need a decision.
    #[serde(rename_all = "camelCase")]
    BenefitReview { account_id: String, count: usize },
    /// A card credit with allowance left in a window about to end. Subscription credits are
    /// spent on their own, so they never ask.
    #[serde(rename_all = "camelCase")]
    CreditExpiring {
        account_id: String,
        benefit_id: String,
        name: String,
        /// The usable allowance left, as a range when credits can't be placed exactly.
        remaining_low: f64,
        remaining_high: f64,
        window_end: NaiveDate,
        days: i64,
    },
    /// Expected bills take cash below zero later than the 30-day coverage check looks.
    #[serde(rename_all = "camelCase")]
    CashLow {
        date: NaiveDate,
        expected: f64,
        low: f64,
    },
    /// A card closes its statement soon with a high share of its limit in use; paying before
    /// the close lowers what the issuer reports.
    #[serde(rename_all = "camelCase")]
    UtilizationHigh {
        account_id: String,
        utilization: f64,
        balance: f64,
        closes_on: NaiveDate,
        closes_estimated: bool,
        days: i64,
    },
    /// A recent loss sale: buying the ticker again on or before `until` would disallow the
    /// loss.
    #[serde(rename_all = "camelCase")]
    WashWindow {
        ticker: String,
        account_id: String,
        sold_on: NaiveDate,
        until: NaiveDate,
        loss: f64,
    },
    /// Two identical posted charges from one merchant on one day.
    #[serde(rename_all = "camelCase")]
    PossibleDuplicate {
        account_id: String,
        merchant: String,
        amount: f64,
        date: NaiveDate,
        transaction_ids: [String; 2],
    },
    /// A recurring charge or deposit that was due and never posted. Marking the series' last
    /// transaction reviewed dismisses it.
    #[serde(rename_all = "camelCase")]
    MissedBill {
        recurring_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        account_id: Option<String>,
        expected: NaiveDate,
        days: i64,
        amount: f64,
        event: super::forecast::EventKind,
        source_transaction_id: String,
    },
    /// IRA inflows that wait for the user to say which tax year they count toward, if any.
    #[serde(rename_all = "camelCase")]
    ContributionsToConfirm {
        account_id: String,
        count: usize,
        amount: f64,
    },
    /// Contributions past a year's limit, or on pace to pass it. An excess costs a tax until
    /// it's taken out.
    #[serde(rename_all = "camelCase")]
    ContributionLimit {
        /// `workplace` or `ira`.
        plan: &'static str,
        tax_year: i32,
        limit: f64,
        amount: f64,
        /// `amount` is where this year's deferrals are on pace to end, not where they are.
        on_pace: bool,
    },
    /// A pay statement's deposit to an account Brief tracks never arrived there.
    #[serde(rename_all = "camelCase")]
    PayMissing {
        document_id: String,
        employer: String,
        date: NaiveDate,
        amount: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        account_id: Option<String>,
    },
    /// A W-2 states boxes the year's pay statements don't add up to.
    #[serde(rename_all = "camelCase")]
    W2Differs {
        document_id: String,
        employer: String,
        tax_year: i32,
        boxes: usize,
    },
    /// Cash earning next to nothing while another cash account the user holds pays more.
    #[serde(rename_all = "camelCase")]
    IdleCash {
        account_id: String,
        balance: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        annual_rate: Option<f64>,
        better_account_id: String,
        better_rate: f64,
    },
    /// A program rule's note: a monthly condition fell short, or what should follow hasn't.
    #[serde(rename_all = "camelCase")]
    ProgramNote {
        program: String,
        says: String,
        /// The object it's about.
        subject: String,
    },
    // The inferred needs below each carry how sure Brief is (`confidence`, 0 to 1, from the
    // formula on its detector) and the transactions it read (`evidence`). Each is raised only
    // at `RAISE_CONFIDENCE` or above, and marking `transactionId` reviewed dismisses it.
    /// A recurring charge or deposit seen for the first time.
    #[serde(rename_all = "camelCase")]
    NewSeries {
        recurring_id: String,
        title: String,
        event: super::forecast::EventKind,
        frequency: &'static str,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        account_id: Option<String>,
        amount: f64,
        first_seen: NaiveDate,
        transaction_id: String,
        confidence: f64,
        evidence: Vec<String>,
    },
    /// A subscription charged a new price.
    #[serde(rename_all = "camelCase")]
    PriceChanged {
        recurring_id: String,
        title: String,
        previous: f64,
        latest: f64,
        changed_on: NaiveDate,
        transaction_id: String,
        confidence: f64,
        evidence: Vec<String>,
    },
    /// Bank fees, card interest, and foreign transaction fees charged in the last
    /// `FEES_DAYS`, not yet reviewed.
    #[serde(rename_all = "camelCase")]
    FeesCharged {
        since: NaiveDate,
        count: usize,
        /// Their sum, negative.
        amount: f64,
        /// The latest fee.
        transaction_id: String,
        confidence: f64,
        evidence: Vec<String>,
    },
    /// A paycheck outside the range its series' earlier paychecks span.
    #[serde(rename_all = "camelCase")]
    PayDiffers {
        recurring_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        account_id: Option<String>,
        amount: f64,
        low: f64,
        high: f64,
        date: NaiveDate,
        transaction_id: String,
        confidence: f64,
        evidence: Vec<String>,
    },
    /// A charge far above what a merchant steadily charges.
    #[serde(rename_all = "camelCase")]
    UnusualCharge {
        merchant: String,
        amount: f64,
        usual: f64,
        observations: usize,
        transaction_id: String,
        confidence: f64,
        evidence: Vec<String>,
    },
}

/// The least confidence an inferred need is raised at. Below it, the finding shows only on its
/// report or card.
pub const RAISE_CONFIDENCE: f64 = 0.6;
/// How far back fees and interest charged count.
pub const FEES_DAYS: i64 = 30;

impl Item {
    /// How sure Brief is of an inferred need; a need read from records is certain.
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Self::NewSeries { confidence, .. }
            | Self::PriceChanged { confidence, .. }
            | Self::FeesCharged { confidence, .. }
            | Self::PayDiffers { confidence, .. }
            | Self::UnusualCharge { confidence, .. } => Some(*confidence),
            _ => None,
        }
    }
}

/// How sure the policy is that a transaction is a fee or interest charge: your own category is
/// certain; Plaid's carries its stated confidence (VERY_HIGH is above 98%, HIGH above 90%, by
/// Plaid's definitions), and a descriptor that says "fee" or "interest charge" makes it at least
/// 0.9.
pub fn fee_confidence(transaction: &crate::finance_contract::Transaction) -> f64 {
    if transaction.category_source == Some(crate::transaction_policy::CategorySource::Manual) {
        return 1.0;
    }
    // Plaid's confidence was for Plaid's category, not the one Brief suggested.
    if let Some(suggestion) = &transaction.category_suggestion {
        return suggestion.confidence;
    }
    let stated: f64 = match transaction.category_confidence.as_deref() {
        Some("VERY_HIGH") => 0.98,
        Some("HIGH") => 0.9,
        Some("MEDIUM") => 0.7,
        Some("LOW") => 0.4,
        _ => 0.5,
    };
    let text = format!(
        "{} {}",
        transaction.merchant,
        transaction.description.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let named = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == "fee")
        || ["interest charge", "finance charge", "foreign transaction"]
            .iter()
            .any(|phrase| text.contains(phrase));
    if named {
        stated.max(0.9)
    } else {
        stated
    }
}

/// What the detectors found, gathered for `inferred`.
pub struct Signals<'a> {
    pub today: NaiveDate,
    pub transactions: &'a [crate::finance_contract::Transaction],
    pub new_series: &'a [super::forecast::NewSeries],
    pub pay: &'a [super::forecast::PayChange],
    pub subscriptions: &'a super::subscriptions::Subscriptions,
    pub anomalies: &'a Anomalies,
}

/// The inferred needs: a new recurring series, a subscription's new price, fees and interest
/// charged, a paycheck outside its band, and an unusual charge. Each is raised at
/// `RAISE_CONFIDENCE` or above and leaves once its transaction is marked reviewed. Bills and
/// card closings appear only in Coming up.
pub fn inferred(signals: Signals<'_>) -> Vec<AttentionItem> {
    let today = signals.today;
    let reviewed = |id: &str| {
        signals
            .transactions
            .iter()
            .any(|transaction| transaction.id == id && transaction.reviewed == Some(true))
    };
    let attention = |id: String, item: Item| AttentionItem {
        id,
        severity: Severity::Attention,
        item,
    };
    let mut items = Vec::new();
    for series in signals.new_series {
        if reviewed(&series.transaction_id) {
            continue;
        }
        items.push(attention(
            format!("new-series:{}", series.recurring_id),
            Item::NewSeries {
                recurring_id: series.recurring_id.clone(),
                title: series.title.clone(),
                event: series.kind,
                frequency: series.frequency,
                account_id: series.account_id.clone(),
                amount: series.amount,
                first_seen: series.first_seen,
                transaction_id: series.transaction_id.clone(),
                confidence: series.confidence,
                evidence: series.evidence.clone(),
            },
        ));
    }
    for subscription in &signals.subscriptions.items {
        let Some(change) = &subscription.price_change else {
            continue;
        };
        if (today - change.changed_on).num_days() > super::anomalies::RECENT_DAYS
            || reviewed(&change.transaction_id)
        {
            continue;
        }
        items.push(attention(
            format!("price-changed:{}", change.transaction_id),
            Item::PriceChanged {
                recurring_id: subscription.recurring_id.clone(),
                title: subscription.title.clone(),
                previous: change.previous,
                latest: change.latest,
                changed_on: change.changed_on,
                transaction_id: change.transaction_id.clone(),
                confidence: change.confidence(),
                evidence: change
                    .steady
                    .iter()
                    .chain(std::iter::once(&change.transaction_id))
                    .cloned()
                    .collect(),
            },
        ));
    }
    let since = today - chrono::Duration::days(FEES_DAYS);
    let mut fees = signals
        .transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction.amount < 0.0
                && transaction.reviewed != Some(true)
                && transaction.kind() == Some(crate::transaction_policy::Kind::Fee)
                && fee_confidence(transaction) >= RAISE_CONFIDENCE
        })
        .filter_map(|transaction| {
            let date = super::calendar::transaction_date(transaction.posted_date())?;
            (date >= since && date <= today).then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    fees.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));
    if let Some((_, latest)) = fees.last() {
        items.push(attention(
            // Named by the latest fee, so a new one raises it again.
            format!("fees:{}", latest.id),
            Item::FeesCharged {
                since,
                count: fees.len(),
                amount: crate::numeric::from_minor(
                    fees.iter()
                        .map(|(_, fee)| crate::numeric::minor_units(fee.amount))
                        .sum(),
                ),
                transaction_id: latest.id.clone(),
                confidence: fees
                    .iter()
                    .map(|(_, fee)| fee_confidence(fee))
                    .fold(1.0, f64::min),
                evidence: fees.iter().map(|(_, fee)| fee.id.clone()).collect(),
            },
        ));
    }
    for pay in signals.pay {
        if reviewed(&pay.transaction_id) {
            continue;
        }
        items.push(attention(
            format!("pay-differs:{}", pay.transaction_id),
            Item::PayDiffers {
                recurring_id: pay.recurring_id.clone(),
                title: pay.title.clone(),
                account_id: pay.account_id.clone(),
                amount: pay.amount,
                low: pay.low,
                high: pay.high,
                date: pay.date,
                transaction_id: pay.transaction_id.clone(),
                confidence: pay.confidence,
                evidence: pay.evidence.clone(),
            },
        ));
    }
    // Anomalies already leave out reviewed charges.
    for charge in &signals.anomalies.unusual {
        items.push(attention(
            format!("unusual:{}", charge.transaction_id),
            Item::UnusualCharge {
                merchant: charge.merchant.clone(),
                amount: charge.amount,
                usual: charge.usual,
                observations: charge.observations,
                transaction_id: charge.transaction_id.clone(),
                confidence: charge.confidence(),
                evidence: charge
                    .earlier
                    .iter()
                    .chain(std::iter::once(&charge.transaction_id))
                    .cloned()
                    .collect(),
            },
        ));
    }
    items.retain(|item| {
        item.item
            .confidence()
            .is_none_or(|confidence| confidence >= RAISE_CONFIDENCE)
    });
    items
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub id: String,
    pub severity: Severity,
    #[serde(flatten)]
    pub item: Item,
}

#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub today: NaiveDate,
    pub spending_account_id: Option<&'a str>,
    pub statement: Option<&'a StatementCycle>,
    pub plan: &'a Plan,
}

/// What the Today page leads with: everything that needs action, most urgent first. Unlike
/// `attention`, the list is not capped.
#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub needs: Vec<AttentionItem>,
}

/// Days before a window closes that an unused credit is worth raising: a week for monthly
/// credits, three weeks for anything longer.
fn expiry_notice(cadence: Cadence) -> Option<i64> {
    match cadence {
        Cadence::Monthly => Some(7),
        Cadence::Quarterly | Cadence::Semiannual | Cadence::Annual => Some(21),
        Cadence::Renewal | Cadence::Purchase => None,
    }
}

pub fn needs(
    context: Context<'_>,
    cards: &[(&str, &[BenefitWindow], &[BenefitReview])],
    hidden_benefit_ids: &[String],
    cash: &CashTimeline,
    credit: &Credit,
    anomalies: &Anomalies,
) -> Vec<AttentionItem> {
    let today = context.today;
    let mut items = attention(context);
    let covered = items
        .iter()
        .any(|item| matches!(item.item, Item::CoverageShort { .. }));
    if let Some(lowest) = cash
        .lowest
        .as_ref()
        .filter(|day| day.expected < 0.0 && !covered)
    {
        items.push(AttentionItem {
            id: format!("cash-low:{}", lowest.date),
            severity: Severity::Attention,
            item: Item::CashLow {
                date: lowest.date,
                expected: lowest.expected,
                low: lowest.low,
            },
        });
    }
    for (account_id, _, review) in cards {
        let open = review
            .iter()
            .filter(|item| {
                item.benefit_id
                    .as_ref()
                    .is_none_or(|id| !hidden_benefit_ids.contains(id))
            })
            .collect::<Vec<_>>();
        // Named by the newest item, so a new match raises it again.
        if let Some(newest) = open.first() {
            items.push(AttentionItem {
                id: format!("benefit-review:{account_id}:{}", newest.transaction_id),
                severity: Severity::Attention,
                item: Item::BenefitReview {
                    account_id: (*account_id).to_owned(),
                    count: open.len(),
                },
            });
        }
    }
    for (account_id, window) in cards.iter().flat_map(|(account_id, windows, _)| {
        windows.iter().map(move |window| (*account_id, window))
    }) {
        let days = (window.window_end - today).num_days();
        let range = window
            .remaining_range
            .as_ref()
            .map(|range| (range.low, range.high))
            .or(window.remaining_amount.map(|amount| (amount, amount)));
        let (Some(notice), Some((low, high))) = (expiry_notice(window.cadence), range) else {
            continue;
        };
        // A subscription spends its credit on its own, so only deadlines that need action count.
        if !window.recurring
            && high > 0.0
            && (0..=notice).contains(&days)
            && !hidden_benefit_ids.contains(&window.id)
        {
            items.push(AttentionItem {
                id: format!(
                    "credit-expiring:{account_id}:{}:{}",
                    window.id, window.window_end
                ),
                severity: Severity::Attention,
                item: Item::CreditExpiring {
                    account_id: account_id.to_owned(),
                    benefit_id: window.id.clone(),
                    name: window.name.clone(),
                    remaining_low: low,
                    remaining_high: high,
                    window_end: window.window_end,
                    days,
                },
            });
        }
    }
    for card in &credit.cards {
        let (Some(utilization), Some(closes_on)) = (card.utilization, card.closes_on) else {
            continue;
        };
        let days = (closes_on - today).num_days();
        if utilization >= HIGH_UTILIZATION && (0..=CLOSING_NOTICE_DAYS).contains(&days) {
            items.push(AttentionItem {
                id: format!("utilization:{}:{closes_on}", card.account_id),
                severity: Severity::Attention,
                item: Item::UtilizationHigh {
                    account_id: card.account_id.clone(),
                    utilization,
                    balance: card.balance,
                    closes_on,
                    closes_estimated: card.closes_estimated,
                    days,
                },
            });
        }
    }
    for duplicate in &anomalies.duplicates {
        items.push(AttentionItem {
            id: duplicate.id.clone(),
            severity: Severity::Attention,
            item: Item::PossibleDuplicate {
                account_id: duplicate.account_id.clone(),
                merchant: duplicate.merchant.clone(),
                amount: duplicate.amount,
                date: duplicate.date,
                transaction_ids: duplicate.transaction_ids.clone(),
            },
        });
    }
    items.sort_by_key(|item| item.severity);
    items
}

/// Whether the statement's payment is still owed. The issuer keeps reporting the statement's
/// minimum after it is paid, so a payment on or after the statement date that covers the
/// minimum settles it.
pub fn payment_owed(liability: &CreditLiability) -> bool {
    let paid = match (
        liability.last_payment_date.as_deref(),
        liability.last_statement_date.as_deref(),
    ) {
        (Some(paid_on), Some(statement)) => {
            paid_on >= statement
                && liability
                    .last_payment_amount
                    .zip(liability.minimum_payment)
                    .is_some_and(|(paid, minimum)| paid >= minimum)
        }
        _ => false,
    };
    !paid
        && liability
            .minimum_payment
            .is_none_or(|minimum| minimum > 0.0)
}

pub fn attention(context: Context<'_>) -> Vec<AttentionItem> {
    let today = context.today;
    let mut items = Vec::new();

    if let (Some(account_id), Some(liability)) = (
        context.spending_account_id,
        context.statement.and_then(|cycle| cycle.liability.as_ref()),
    ) {
        if let Some(due) = liability
            .next_payment_due_date
            .as_deref()
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        {
            let days = (due - today).num_days();
            let overdue = liability.is_overdue == Some(true) || days < 0;
            if payment_owed(liability) && (overdue || days <= PAYMENT_DUE_DAYS) {
                items.push(AttentionItem {
                    id: format!("payment-due:{account_id}:{due}"),
                    severity: if overdue || days <= 3 {
                        Severity::Urgent
                    } else {
                        Severity::Attention
                    },
                    item: Item::PaymentDue {
                        account_id: account_id.to_owned(),
                        due,
                        days,
                        overdue,
                        statement_balance: liability.last_statement_balance,
                        minimum_payment: liability.minimum_payment,
                    },
                });
            }
        }
    }

    let coverage = &context.plan.coverage;
    if !coverage.cash_incomplete
        && coverage.expected_outflows > 0.0
        && coverage.after_outflows < 0.0
    {
        items.push(AttentionItem {
            id: format!("coverage:{}", context.plan.month_start),
            severity: Severity::Urgent,
            item: Item::CoverageShort {
                cash: coverage.cash,
                expected_outflows: coverage.expected_outflows,
                shortfall: -coverage.after_outflows,
            },
        });
    }

    // Stable ranking: severity, then the order above, which is most consequential first.
    items.sort_by_key(|item| item.severity);
    items.truncate(MAX_ITEMS);
    items
}
