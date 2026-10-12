//! Refunds a purchase most plausibly explains, so the transaction inspector can offer to link
//! them. A suggestion is made only when exactly one earlier purchase fits.
use chrono::NaiveDate;
use serde::Serialize;

use super::calendar::{self, Zone};
use crate::finance_contract::Transaction;
use crate::rules::normalize_merchant;
use crate::transaction_policy::Kind;

/// Refunds this recent are matched.
const WINDOW_DAYS: i64 = 90;

/// A refund and the one earlier purchase it most plausibly repays.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefundSuggestion {
    pub refund_id: String,
    pub purchase_id: String,
}

/// A refund is matched to purchases at the same merchant and account this far back.
const REFUND_LOOKBACK_DAYS: i64 = 120;

/// Unlinked refunds in the window with exactly one purchase they could repay: the only
/// earlier purchase at that merchant large enough, or the only one of the same amount.
fn matches(
    posted: &[(NaiveDate, &Transaction)],
    since: NaiveDate,
    merchant: impl Fn(&Transaction) -> String,
) -> Vec<RefundSuggestion> {
    let linked = posted
        .iter()
        .filter_map(|(_, transaction)| transaction.refund_of.as_deref())
        .collect::<std::collections::HashSet<_>>();
    posted
        .iter()
        .filter(|(date, refund)| {
            *date > since
                && refund.amount > 0.0
                && refund.refund_of.is_none()
                && refund.reviewed != Some(true)
                && refund.kind() == Some(Kind::Reimbursement)
        })
        .filter_map(|(date, refund)| {
            let key = merchant(refund);
            if key.is_empty() {
                return None;
            }
            let candidates = posted
                .iter()
                .filter(|(bought, purchase)| {
                    bought <= date
                        && (*date - *bought).num_days() <= REFUND_LOOKBACK_DAYS
                        && matches!(purchase.kind(), Some(Kind::Expense | Kind::Owed))
                        && purchase.amount < 0.0
                        && -purchase.amount >= refund.amount
                        && purchase.account_id == refund.account_id
                        && !linked.contains(purchase.id.as_str())
                        && merchant(purchase) == key
                })
                .map(|(_, purchase)| *purchase)
                .collect::<Vec<_>>();
            let exact = candidates
                .iter()
                .filter(|purchase| -purchase.amount == refund.amount)
                .collect::<Vec<_>>();
            let purchase = match (exact.as_slice(), candidates.as_slice()) {
                ([only], _) => only,
                ([], [only]) => only,
                _ => return None,
            };
            Some(RefundSuggestion {
                refund_id: refund.id.clone(),
                purchase_id: purchase.id.clone(),
            })
        })
        .collect()
}

/// Suggestions for unlinked refunds posted in the last 90 days.
pub fn suggestions(
    transactions: &[Transaction],
    reference: &str,
    today: NaiveDate,
    zone: &dyn Zone,
) -> Vec<RefundSuggestion> {
    let posted = calendar::posted_through(transactions, reference, today, zone);
    matches(
        &posted,
        today - chrono::Duration::days(WINDOW_DAYS),
        |transaction: &Transaction| normalize_merchant(&transaction.merchant),
    )
}
