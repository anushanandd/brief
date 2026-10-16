//! What one refresh changed in saved transactions, and how much of each Plaid cash and card
//! account's balance change those changes account for.
//!
//! Plaid replaces a pending charge with a new posted one and deletes what disappears, so the
//! tip on a meal, a dropped hold, and an amended charge leave no trace once a refresh commits.
//! Comparing the previous snapshot with the new one keeps them. The same comparison checks each
//! balance: a balance that moves with no transaction to match, refresh after refresh, points
//! to missing or duplicated activity.
use super::*;

/// Activity dated this many days before the previous refresh is old activity arriving late
/// (Plaid's historical pull, a statement import), already in the previous balance.
pub(super) const LATE_ACTIVITY_DAYS: i64 = 10;

/// The most changes one refresh lists; the rest are still counted in the balance check.
const MAX_LISTED: usize = 60;

pub(super) struct TransactionDiff {
    pub changes: Vec<TransactionChange>,
    /// New transactions dated before the late-activity cutoff.
    pub backfilled: usize,
    /// For each checked account, the balance effect of this refresh's transaction changes.
    pub explained: BTreeMap<String, Decimal>,
}

/// Compares the previous snapshot's transactions with this refresh's in the accounts that
/// were in both snapshots and, when the refresh names them, were refreshed.
pub(super) fn transaction_diff(
    previous: &Snapshot,
    current: &[Transaction],
    checked_accounts: &BTreeSet<String>,
    observed_at: &str,
) -> TransactionDiff {
    let cutoff = previous_refresh_date(previous)
        .map(|date| date - Duration::days(LATE_ACTIVITY_DAYS))
        .unwrap_or(NaiveDate::MIN);
    let in_scope = |transaction: &&Transaction| {
        transaction
            .account_id
            .as_deref()
            .is_some_and(|account| checked_accounts.contains(account))
    };
    let recent = |transaction: &Transaction| {
        NaiveDate::parse_from_str(transaction.posted_date(), "%Y-%m-%d")
            .is_ok_and(|date| date >= cutoff)
    };
    let before = previous
        .transactions
        .iter()
        .filter(in_scope)
        .map(|transaction| (transaction.id.as_str(), transaction))
        .collect::<BTreeMap<_, _>>();
    let now = current
        .iter()
        .filter(in_scope)
        .map(|transaction| (transaction.id.as_str(), transaction))
        .collect::<BTreeMap<_, _>>();
    let replaced = now
        .values()
        .filter_map(|transaction| transaction.pending_transaction_id.as_deref())
        .collect::<BTreeSet<_>>();

    let mut explained = checked_accounts
        .iter()
        .map(|account| (account.clone(), Decimal::ZERO))
        .collect::<BTreeMap<_, _>>();
    let mut backfilled = 0;
    let mut explain = |transaction: &Transaction, amount: Decimal| {
        if let Some(total) = transaction
            .account_id
            .as_ref()
            .and_then(|account| explained.get_mut(account))
        {
            *total += amount;
        }
    };
    let mut changes = Vec::new();
    for (id, transaction) in &now {
        let amount = decimal_f64(transaction.amount).unwrap_or_default();
        match before.get(id) {
            Some(prior) => {
                let prior_amount = decimal_f64(prior.amount).unwrap_or_default();
                match (prior.pending, transaction.pending) {
                    (false, false) if prior_amount != amount => {
                        explain(transaction, amount - prior_amount);
                        changes.push(change(
                            transaction,
                            TransactionChangeKind::Amended,
                            Some(prior.amount),
                        ));
                    }
                    // The same transaction posting under its own ID.
                    (true, false) => {
                        explain(transaction, amount);
                        if prior_amount != amount {
                            changes.push(change(
                                transaction,
                                TransactionChangeKind::Settled,
                                Some(prior.amount),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            None => {
                let settled = transaction
                    .pending_transaction_id
                    .as_deref()
                    .and_then(|pending| before.get(pending));
                if !transaction.pending {
                    if recent(transaction) || settled.is_some() {
                        explain(transaction, amount);
                    }
                }
                match settled {
                    Some(pending) => {
                        if decimal_f64(pending.amount).unwrap_or_default() != amount {
                            changes.push(change(
                                transaction,
                                TransactionChangeKind::Settled,
                                Some(pending.amount),
                            ));
                        }
                    }
                    None if recent(transaction) => {
                        changes.push(change(transaction, TransactionChangeKind::Added, None));
                    }
                    None => backfilled += 1,
                }
            }
        }
    }
    for (id, prior) in &before {
        if now.contains_key(id) || replaced.contains(id) {
            continue;
        }
        if !prior.pending && recent(prior) {
            explain(prior, -decimal_f64(prior.amount).unwrap_or_default());
        }
        if prior.pending || recent(prior) {
            changes.push(change(prior, TransactionChangeKind::Removed, None));
        }
    }
    // The changes worth reading first: settled, removed, and amended, then the newest additions.
    changes.sort_by(|left, right| {
        (left.kind == TransactionChangeKind::Added)
            .cmp(&(right.kind == TransactionChangeKind::Added))
            .then_with(|| right.date.cmp(&left.date))
            .then_with(|| left.transaction_id.cmp(&right.transaction_id))
    });
    changes.truncate(MAX_LISTED);
    for change in &mut changes {
        change.id = format!("{observed_at}:{}", change.id);
        change.observed_at = observed_at.into();
    }
    TransactionDiff {
        changes,
        backfilled,
        explained,
    }
}

fn change(
    transaction: &Transaction,
    kind: TransactionChangeKind,
    previous_amount: Option<f64>,
) -> TransactionChange {
    let label = match kind {
        TransactionChangeKind::Added => "added",
        TransactionChangeKind::Settled => "settled",
        TransactionChangeKind::Removed => "removed",
        TransactionChangeKind::Amended => "amended",
    };
    TransactionChange {
        id: format!("{label}:{}", transaction.id),
        observed_at: String::new(),
        kind,
        transaction_id: transaction.id.clone(),
        account_id: transaction.account_id.clone().unwrap_or_default(),
        merchant: transaction.merchant.clone(),
        date: transaction.posted_date().to_owned(),
        amount: transaction.amount,
        previous_amount,
        pending: transaction.pending,
    }
}

/// The calendar day of the previous refresh.
fn previous_refresh_date(previous: &Snapshot) -> Option<NaiveDate> {
    previous
        .calendar_date
        .as_deref()
        .or_else(|| previous.updated_at.get(..10))
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

/// The saved transaction changes with this refresh's, the latest 500.
pub(super) fn merge_transaction_changes(
    previous: &Snapshot,
    change: Option<&Change>,
) -> Vec<TransactionChange> {
    let mut changes = previous.transaction_changes.clone();
    if let Some(change) = change {
        let known = changes
            .iter()
            .map(|change| change.id.clone())
            .collect::<BTreeSet<_>>();
        changes.extend(
            change
                .transaction_changes
                .iter()
                .filter(|change| !known.contains(&change.id))
                .cloned(),
        );
    }
    changes.sort_by(|left, right| left.observed_at.cmp(&right.observed_at));
    let skip = changes.len().saturating_sub(500);
    changes.split_off(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(transactions: Vec<Transaction>) -> Snapshot {
        let mut snapshot = crate::storage::empty_state().unwrap().snapshot;
        snapshot.updated_at = "2026-10-10T15:00:00Z".into();
        snapshot.calendar_date = Some("2026-10-10".into());
        snapshot.transactions = transactions;
        snapshot
    }

    fn transaction(id: &str, account: &str, date: &str, amount: f64, pending: bool) -> Transaction {
        Transaction {
            id: id.into(),
            merchant: format!("Merchant {id}"),
            date: date.into(),
            posted_on: Some(date.into()),
            amount,
            account_id: Some(account.into()),
            pending,
            ..Transaction::default()
        }
    }

    fn checked() -> BTreeSet<String> {
        ["plaid:card", "plaid:checking"].map(String::from).into()
    }

    #[test]
    fn tips_dropped_holds_and_amendments_are_kept_and_explain_the_balance() {
        let previous = snapshot(vec![
            transaction("meal-pending", "plaid:card", "2026-10-09", -40.0, true),
            transaction("hold", "plaid:checking", "2026-10-09", -1.0, true),
            transaction("amended", "plaid:checking", "2026-10-08", -20.0, false),
            transaction("same", "plaid:checking", "2026-10-08", -5.0, false),
        ]);
        let mut meal = transaction("meal", "plaid:card", "2026-10-11", -48.0, false);
        meal.pending_transaction_id = Some("meal-pending".into());
        let current = vec![
            meal,
            transaction("amended", "plaid:checking", "2026-10-08", -25.0, false),
            transaction("same", "plaid:checking", "2026-10-08", -5.0, false),
            transaction("paycheck", "plaid:checking", "2026-10-11", 2_000.0, false),
            transaction("old", "plaid:checking", "2026-08-01", -300.0, false),
            transaction("elsewhere", "plaid:brokerage", "2026-10-11", -9.0, false),
        ];
        let diff = transaction_diff(&previous, &current, &checked(), "2026-10-11T15:00:00Z");

        let kinds = diff
            .changes
            .iter()
            .map(|change| {
                (
                    change.transaction_id.as_str(),
                    change.kind,
                    change.previous_amount,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                ("meal", TransactionChangeKind::Settled, Some(-40.0)),
                ("hold", TransactionChangeKind::Removed, None),
                ("amended", TransactionChangeKind::Amended, Some(-20.0)),
                ("paycheck", TransactionChangeKind::Added, None),
            ]
        );
        assert_eq!(diff.backfilled, 1);
        assert_eq!(diff.explained["plaid:card"], Decimal::new(-48, 0));
        assert_eq!(diff.explained["plaid:checking"], Decimal::new(1_995, 0));
        assert!(diff.changes.iter().all(|change| {
            change.observed_at == "2026-10-11T15:00:00Z"
                && change.id.starts_with("2026-10-11T15:00:00Z:")
        }));
    }

    #[test]
    fn a_posted_charge_that_disappears_takes_its_amount_back_out() {
        let previous = snapshot(vec![transaction(
            "reversed",
            "plaid:checking",
            "2026-10-09",
            -60.0,
            false,
        )]);
        let diff = transaction_diff(&previous, &[], &checked(), "2026-10-11T15:00:00Z");
        assert_eq!(diff.changes[0].kind, TransactionChangeKind::Removed);
        assert_eq!(diff.explained["plaid:checking"], Decimal::new(60, 0));
    }

    #[test]
    fn the_history_keeps_each_change_once_and_the_latest_five_hundred() {
        let mut previous = snapshot(Vec::new());
        let added = |index: usize| TransactionChange {
            id: format!("2026-10-01T00:00:00Z:added:{index}"),
            observed_at: "2026-10-01T00:00:00Z".into(),
            kind: TransactionChangeKind::Added,
            transaction_id: index.to_string(),
            account_id: "plaid:checking".into(),
            merchant: "Store".into(),
            date: "2026-10-01".into(),
            amount: -1.0,
            previous_amount: None,
            pending: false,
        };
        previous.transaction_changes = (0..499).map(added).collect();
        let change = Change {
            observed_at: "2026-10-02T00:00:00Z".into(),
            previous_updated_at: String::new(),
            comparison_complete: None,
            previous_net_worth: 0.0,
            net_worth_change: 0.0,
            account_changes: Vec::new(),
            new_transaction_ids: Vec::new(),
            transaction_changes: vec![added(0), added(500), added(501)],
            backfilled: 0,
        };
        let merged = merge_transaction_changes(&previous, Some(&change));
        assert_eq!(merged.len(), 500);
        assert_eq!(merged.last().unwrap().transaction_id, "501");
        assert_eq!(merged.first().unwrap().transaction_id, "1");
    }
}
