//! Checks that saved activity accounts for the balances providers report. Each refresh records,
//! for every Plaid cash and card account, how much of its balance change that refresh's
//! transaction changes explain. A timing difference (a balance updated before its transaction
//! arrives) reverses on a later refresh; missing or duplicated activity leaves a difference
//! that stays. A card payment from a tracked bank account that no tracked card received is the
//! other common sign of a gap.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};

use super::{issue, Category, HealthIssue, HealthItem, Severity};
use crate::finance_contract::{AccountMovement, Snapshot, Transaction};

/// A difference counts once it has stayed through two refreshes and is above this many dollars,
/// or half a percent of the balance when that's larger.
const MINIMUM_GAP: f64 = 5.0;
const GAP_SHARE: f64 = 0.005;
/// Only a difference that grew within this many days is reported, so an old one expires.
const RECENT_DAYS: i64 = 30;
/// Card payments from this many days back are checked.
const PAYMENT_DAYS: i64 = 60;
const CARD_PAYMENT: &str = "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT";

pub(super) fn ledger_issues(snapshot: &Snapshot, now: DateTime<Utc>) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    let gaps = balance_gaps(snapshot, now);
    if !gaps.is_empty() {
        issues.push(
            issue(
                "unexplained-balance-change",
                Severity::Info,
                Category::Consistency,
                "Balances moved without matching activity",
                "A bank balance changed by more than its new transactions explain; activity may be missing or counted twice",
            )
            .evidence(
                gaps.iter()
                    .map(|gap| format!("{}: {:+.2} unmatched", gap.name, gap.amount))
                    .collect(),
            )
            .affecting(
                gaps.iter()
                    .map(|gap| HealthItem {
                        id: gap.account_id.clone(),
                        name: gap.name.clone(),
                    })
                    .collect(),
            )
            .action("Review accounts", "/accounts"),
        );
    }
    let unmatched = unmatched_card_payments(snapshot, now);
    if !unmatched.is_empty() {
        issues.push(
            issue(
                "unmatched-card-payments",
                Severity::Info,
                Category::Consistency,
                "Card payments no tracked card received",
                "Mark one reviewed if it paid a card Brief doesn't track",
            )
            .evidence(vec![super::count_of(unmatched.len(), "payment")])
            .affecting(
                unmatched
                    .iter()
                    .map(|transaction| HealthItem {
                        id: transaction.id.clone(),
                        name: format!("{} on {}", transaction.merchant, transaction.posted_date()),
                    })
                    .collect(),
            )
            .action("Review accounts", "/accounts"),
        );
    }
    issues.extend(statement_issues(snapshot));
    issues.extend(issuer_detail_issues(snapshot, now));
    issues
}

/// A card's issuer details (store addresses, fuller names) come only from its imported
/// exports, so they stop at the latest one. Past a statement cycle, newer purchases are listed
/// until the next export is imported.
fn issuer_detail_issues(snapshot: &Snapshot, now: DateTime<Utc>) -> Vec<HealthIssue> {
    let today = now.date_naive();
    let purchase = |transaction: &&Transaction| {
        transaction.amount < 0.0
            && matches!(
                transaction.payment_channel.as_deref(),
                Some("in store" | "online")
            )
    };
    let mut latest = BTreeMap::<&str, NaiveDate>::new();
    for transaction in &snapshot.transactions {
        let (Some(account), Some(date)) = (
            transaction.account_id.as_deref(),
            NaiveDate::parse_from_str(transaction.posted_date(), "%Y-%m-%d").ok(),
        ) else {
            continue;
        };
        if transaction.issuer.is_some() {
            let entry = latest.entry(account).or_insert(date);
            *entry = (*entry).max(date);
        }
    }
    let behind = latest
        .iter()
        .filter(|(_, through)| today - **through > Duration::days(ISSUER_DETAIL_DAYS))
        .filter_map(|(account, through)| {
            let since = snapshot
                .transactions
                .iter()
                .filter(purchase)
                .filter(|transaction| transaction.account_id.as_deref() == Some(account))
                .filter(|transaction| transaction.posted_date() > through.to_string().as_str())
                .count();
            let name = snapshot
                .accounts
                .iter()
                .find(|candidate| candidate.id == *account)
                .map_or("Card", |candidate| candidate.name.as_str());
            (since > 0).then(|| {
                (
                    HealthItem {
                        id: (*account).to_owned(),
                        name: name.to_owned(),
                    },
                    format!(
                        "{name}: {} since {through}",
                        super::count_of(since, "purchase")
                    ),
                )
            })
        })
        .collect::<Vec<_>>();
    if behind.is_empty() {
        return Vec::new();
    }
    vec![issue(
        "issuer-details-behind",
        Severity::Info,
        Category::Freshness,
        "Store addresses stop at your last card export",
        "Import the card's latest activity export to add the issuer's addresses and fuller names",
    )
    .evidence(behind.iter().map(|(_, line)| line.clone()).collect())
    .affecting(behind.into_iter().map(|(item, _)| item).collect())
    .action("Import a statement", "/settings/accounts#statement-history")]
}

/// About one statement cycle and the days an export takes to post.
const ISSUER_DETAIL_DAYS: i64 = 40;

/// Where imported statements and Plaid disagree, and transactions whose own details do.
fn statement_issues(snapshot: &Snapshot) -> Vec<HealthIssue> {
    let account = |id: &str| {
        snapshot
            .accounts
            .iter()
            .find(|account| account.id == id)
            .map_or("Account", |account| account.name.as_str())
            .to_owned()
    };
    let item = |id: &str| {
        let transaction = snapshot
            .transactions
            .iter()
            .find(|transaction| transaction.id == id)?;
        Some((
            transaction,
            format!("{} on {}", transaction.merchant, transaction.posted_date()),
        ))
    };
    let checked = &snapshot.statement_reconciliation;
    let mut issues = Vec::new();

    let missing = checked
        .iter()
        .flat_map(|checked| {
            checked
                .missing
                .iter()
                .filter(|row| !row.confirmed)
                .map(|row| HealthItem {
                    id: row.row_id.clone(),
                    name: format!(
                        "{} on {} · {:+.2} · {}",
                        row.description,
                        row.date,
                        row.amount,
                        account(&checked.account_id)
                    ),
                })
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        issues.push(
            issue(
                "statement-rows-missing",
                Severity::Warning,
                Category::Consistency,
                "Statements list activity Plaid never reported",
                "Add each row to history once you're sure Plaid missed it",
            )
            .evidence(vec![super::count_of(missing.len(), "row")])
            .affecting(missing)
            .action("Review statements", "/settings/accounts#statement-history"),
        );
    }

    let extra = checked
        .iter()
        .flat_map(|checked| &checked.extra)
        .filter_map(|id| {
            item(id).map(|(transaction, name)| HealthItem {
                id: id.clone(),
                name: format!("{name} · {:+.2}", transaction.amount),
            })
        })
        .collect::<Vec<_>>();
    if !extra.is_empty() {
        issues.push(
            issue(
                "statement-extra-activity",
                Severity::Warning,
                Category::Consistency,
                "Plaid reports activity no statement lists",
                "It may be counted twice, or the issuer may have reversed it",
            )
            .evidence(vec![super::count_of(extra.len(), "transaction")])
            .affecting(extra)
            .action("Review activity", "/activities?evidence=unlisted"),
        );
    }

    let differs = checked
        .iter()
        .flat_map(|checked| &checked.amount_differs)
        .filter_map(|difference| {
            item(&difference.transaction_id).map(|(transaction, name)| HealthItem {
                id: difference.transaction_id.clone(),
                name: format!(
                    "{name}: Plaid {:+.2}, statement {:+.2}",
                    transaction.amount, difference.statement_amount
                ),
            })
        })
        .collect::<Vec<_>>();
    if !differs.is_empty() {
        issues.push(
            issue(
                "statement-amount-differs",
                Severity::Warning,
                Category::Consistency,
                "Plaid and a statement disagree on amounts",
                "The statement is the issuer's final record",
            )
            .evidence(vec![super::count_of(differs.len(), "transaction")])
            .affecting(differs)
            .action("Review activity", "/activities?evidence=conflict"),
        );
    }

    let places = snapshot
        .transactions
        .iter()
        // Marking one reviewed accepts the place Brief shows.
        .filter(|transaction| transaction.reviewed != Some(true))
        .filter(|transaction| {
            transaction
                .quality
                .as_ref()
                .and_then(|quality| quality.location)
                .is_some_and(|location| location.grade == crate::quality::Grade::Conflict)
        })
        .map(|transaction| HealthItem {
            id: transaction.id.clone(),
            name: format!("{} on {}", transaction.merchant, transaction.posted_date()),
        })
        .collect::<Vec<_>>();
    if !places.is_empty() {
        issues.push(
            issue(
                "transaction-places-disagree",
                Severity::Info,
                Category::Classification,
                "Bank text and Plaid name different places",
                "Brief shows the place in the bank's text; Plaid's stays in Bank details",
            )
            .evidence(vec![super::count_of(places.len(), "transaction")])
            .affecting(places)
            .action("Review activity", "/activities?evidence=conflict"),
        );
    }
    issues
}

struct Gap {
    account_id: String,
    name: String,
    amount: f64,
}

/// Accounts whose unexplained balance change has stayed through the last two refreshes and
/// grew within the last month.
fn balance_gaps(snapshot: &Snapshot, now: DateTime<Utc>) -> Vec<Gap> {
    let mut checked = BTreeMap::<&str, Vec<&AccountMovement>>::new();
    for movement in &snapshot.account_movements {
        if movement.explained.is_some() {
            checked
                .entry(movement.account_id.as_str())
                .or_default()
                .push(movement);
        }
    }
    let recent_from = now - Duration::days(RECENT_DAYS);
    let mut gaps = Vec::new();
    for account in &snapshot.accounts {
        let Some(movements) = checked.get_mut(account.id.as_str()) else {
            continue;
        };
        if movements.len() < 2 {
            continue;
        }
        movements.sort_by(|left, right| left.observed_at.cmp(&right.observed_at));
        let mut total = 0.0;
        let mut before_recent = 0.0;
        let mut totals = Vec::with_capacity(movements.len());
        for movement in movements.iter() {
            total += movement.change - movement.explained.unwrap_or_default();
            totals.push(total);
            if parsed(&movement.observed_at).is_some_and(|at| at < recent_from) {
                before_recent = total;
            }
        }
        let threshold = MINIMUM_GAP.max(account.value.unwrap_or_default().abs() * GAP_SHARE);
        let (latest, prior) = (totals[totals.len() - 1], totals[totals.len() - 2]);
        let persistent = latest.abs() > threshold
            && prior.abs() > threshold
            && latest.signum() == prior.signum();
        let grew_recently = (latest - before_recent).abs() > threshold;
        if persistent && grew_recently {
            gaps.push(Gap {
                account_id: account.id.clone(),
                name: account.name.clone(),
                amount: (latest * 100.0).round() / 100.0,
            });
        }
    }
    gaps
}

/// Posted, unreviewed card payments out of a tracked bank account, settled long enough ago that
/// the card should show them, that the policy paired with no credit on a tracked card. Only
/// checked when a card is tracked.
fn unmatched_card_payments(snapshot: &Snapshot, now: DateTime<Utc>) -> Vec<&Transaction> {
    let window = crate::transaction_policy::TRANSFER_SETTLEMENT_DAYS;
    let kind_of = |account_id: Option<&str>| {
        account_id.and_then(|id| {
            snapshot
                .accounts
                .iter()
                .find(|account| account.id == id)
                .map(|account| account.r#type.as_str())
        })
    };
    let card_legs = snapshot
        .transactions
        .iter()
        .filter(|transaction| kind_of(transaction.account_id.as_deref()) == Some("credit"))
        .map(|transaction| transaction.id.as_str())
        .collect::<BTreeSet<_>>();
    if !snapshot
        .accounts
        .iter()
        .any(|account| account.r#type == "credit")
    {
        return Vec::new();
    }
    let today = now.date_naive();
    snapshot
        .transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction.amount < 0.0
                && transaction.reviewed != Some(true)
                && transaction.category_detail.as_deref() == Some(CARD_PAYMENT)
                && kind_of(transaction.account_id.as_deref()) == Some("cash")
        })
        .filter(|transaction| {
            let Some(paid) = day(transaction) else {
                return false;
            };
            let settled = today - paid > Duration::days(window);
            let recent = today - paid <= Duration::days(PAYMENT_DAYS);
            let received = transaction
                .transfer_pair_id
                .as_deref()
                .is_some_and(|id| card_legs.contains(id));
            settled && recent && !received
        })
        .collect()
}

fn day(transaction: &Transaction) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(transaction.posted_date(), "%Y-%m-%d").ok()
}

fn parsed(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance_contract::Account;

    fn snapshot() -> Snapshot {
        let mut snapshot = crate::storage::empty_state().unwrap().snapshot;
        snapshot.accounts = vec![
            Account {
                id: "plaid:checking".into(),
                name: "Checking".into(),
                r#type: "cash".into(),
                value: Some(2_000.0),
                ..Account::default()
            },
            Account {
                id: "plaid:card".into(),
                name: "Card".into(),
                r#type: "credit".into(),
                value: Some(-500.0),
                ..Account::default()
            },
        ];
        snapshot
    }

    fn movement(at: &str, account: &str, change: f64, explained: f64) -> AccountMovement {
        AccountMovement {
            id: format!("{at}:{account}"),
            observed_at: at.into(),
            account_id: account.into(),
            name: account.into(),
            change,
            explained: Some(explained),
        }
    }

    fn now() -> DateTime<Utc> {
        parsed("2026-10-12T12:00:00Z").unwrap()
    }

    #[test]
    fn a_timing_difference_that_reverses_is_not_a_gap() {
        let mut snapshot = snapshot();
        snapshot.account_movements = vec![
            movement("2026-10-09T12:00:00Z", "plaid:checking", -80.0, 0.0),
            movement("2026-10-10T12:00:00Z", "plaid:checking", 0.0, -80.0),
            movement("2026-10-11T12:00:00Z", "plaid:checking", -20.0, -20.0),
        ];
        assert!(ledger_issues(&snapshot, now()).is_empty());
    }

    #[test]
    fn a_difference_that_stays_through_two_refreshes_is_reported() {
        let mut snapshot = snapshot();
        snapshot.account_movements = vec![
            movement("2026-10-09T12:00:00Z", "plaid:checking", -80.0, 0.0),
            movement("2026-10-10T12:00:00Z", "plaid:checking", -20.0, -20.0),
        ];
        let issues = ledger_issues(&snapshot, now());
        assert_eq!(issues[0].id, "unexplained-balance-change");
        assert_eq!(issues[0].evidence, vec!["Checking: -80.00 unmatched"]);
        assert_eq!(issues[0].affected_items[0].id, "plaid:checking");

        // One refresh isn't enough, and a difference within the threshold is noise.
        snapshot.account_movements.pop();
        assert!(ledger_issues(&snapshot, now()).is_empty());
        snapshot.account_movements = vec![
            movement("2026-10-09T12:00:00Z", "plaid:checking", -4.0, 0.0),
            movement("2026-10-10T12:00:00Z", "plaid:checking", -20.0, -20.0),
        ];
        assert!(ledger_issues(&snapshot, now()).is_empty());
    }

    #[test]
    fn a_difference_from_before_the_last_month_has_expired() {
        let mut snapshot = snapshot();
        snapshot.account_movements = vec![
            movement("2026-08-01T12:00:00Z", "plaid:checking", -80.0, 0.0),
            movement("2026-10-10T12:00:00Z", "plaid:checking", -20.0, -20.0),
            movement("2026-10-11T12:00:00Z", "plaid:checking", -5.0, -5.0),
        ];
        assert!(ledger_issues(&snapshot, now()).is_empty());
    }

    fn payment(id: &str, account: &str, date: &str, amount: f64) -> Transaction {
        Transaction {
            id: id.into(),
            merchant: "Card payment".into(),
            date: date.into(),
            posted_on: Some(date.into()),
            amount,
            account_id: Some(account.into()),
            category_detail: Some(CARD_PAYMENT.into()),
            ..Transaction::default()
        }
    }

    #[test]
    fn a_card_payment_no_tracked_card_received_is_reported_until_reviewed() {
        let mut snapshot = snapshot();
        let paired = |mut transaction: Transaction, pair: &str| {
            transaction.transfer_pair_id = Some(pair.into());
            transaction
        };
        snapshot.transactions = vec![
            paired(
                payment("paid", "plaid:checking", "2026-09-20", -300.0),
                "received",
            ),
            paired(
                payment("received", "plaid:card", "2026-09-22", 300.0),
                "paid",
            ),
            payment("unmatched", "plaid:checking", "2026-09-25", -125.0),
            payment("recent", "plaid:checking", "2026-10-10", -50.0),
            payment("old", "plaid:checking", "2026-06-01", -75.0),
        ];
        let issues = ledger_issues(&snapshot, now());
        assert_eq!(issues[0].id, "unmatched-card-payments");
        assert_eq!(
            issues[0]
                .affected_items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["unmatched"]
        );
        snapshot.transactions[2].reviewed = Some(true);
        assert!(ledger_issues(&snapshot, now()).is_empty());

        // Without a tracked card there is nothing to receive a payment.
        snapshot.transactions[2].reviewed = None;
        snapshot.accounts.pop();
        assert!(ledger_issues(&snapshot, now()).is_empty());
    }

    #[test]
    fn purchases_after_the_last_card_export_are_listed_once_a_cycle_has_passed() {
        let purchase = |id: &str, date: &str, issuer: bool| Transaction {
            id: id.into(),
            merchant: "Example Shop".into(),
            date: date.into(),
            amount: -12.0,
            account_id: Some("plaid:card".into()),
            payment_channel: Some("in store".into()),
            issuer: issuer.then(Default::default),
            ..Transaction::default()
        };
        let mut snapshot = snapshot();
        snapshot.transactions = vec![
            purchase("old", "2026-08-20", true),
            purchase("new", "2026-09-28", false),
        ];
        let issues = issuer_detail_issues(&snapshot, now());
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].evidence,
            vec!["Card: 1 purchase since 2026-08-20"]
        );
        // Within a cycle of the export, nothing is behind yet.
        snapshot.transactions[0].date = "2026-09-20".into();
        assert!(issuer_detail_issues(&snapshot, now()).is_empty());
        // A card without exports has nothing to be behind.
        snapshot.transactions[0].issuer = None;
        assert!(issuer_detail_issues(&snapshot, now()).is_empty());
    }
}
