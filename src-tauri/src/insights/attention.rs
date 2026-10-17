//! Heads up: the few things that need action now, at most three. A card payment due within a
//! week or overdue, and expected bills that exceed cash. Items carry
//! structured amounts and dates; the renderer writes the sentences so screen privacy can mask
//! every personal number.
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
