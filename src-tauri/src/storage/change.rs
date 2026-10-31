//! Every write to the finance state as one `Change`, and the one function that applies it.
//!
//! A write builds its change, applies it here, and commits the result with the change in one
//! SQLite transaction (`Database::commit`); replaying the log applies the same changes with
//! the same function, so the saved state and the log can't drift apart by construction. A
//! change carries everything its result depends on: a refresh carries what the providers
//! returned and the clock it was projected at, never a reference to the network. Results
//! depend only on the state before and the change, never on the clock or the machine.
use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{validate_snapshot, validate_state, Annotation, FinanceState};
use crate::{
    finance_contract::Snapshot,
    providers::{PlaidCache, ProviderDataCache, ProviderSync},
    settings::FinanceSettings,
    statements::StatementFile,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", content = "body", rename_all = "camelCase")]
pub enum Change {
    /// The whole state, where replay can start.
    Checkpoint(Checkpoint),
    /// What a refresh fetched, projected over the state before it.
    Refresh(Box<Refresh>),
    /// The imported statements, after an import, a removal, or a confirmed row; saved
    /// provider data is projected again with them.
    Statements(Vec<StatementFile>),
    /// What Wallet held when read, with what the user confirmed, or none once it is turned
    /// off; saved provider data is projected again with it. The read happens before this
    /// change, so replaying it never touches Wallet.
    Wallet(Option<Box<crate::providers::wallet::WalletData>>),
    /// Annotations set, whole, and removed, by transaction id.
    Annotations {
        set: BTreeMap<String, Annotation>,
        removed: Vec<String>,
    },
    Settings(Box<FinanceSettings>),
    AccountLinks(BTreeMap<String, String>),
    /// What Brief read from documents, whole; the originals live in their own database.
    Documents(Vec<crate::documents::Document>),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub reason: Reason,
    pub state: Box<FinanceState>,
}

/// Why a checkpoint holds a whole state rather than a change.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The state the log began from: what an existing database already held, or a new one.
    Start,
    /// A restored backup, or a retained copy chosen in recovery.
    Restore,
    /// A new calculation version projected saved data again; earlier changes replay as the
    /// new code would project them, so replay starts here.
    Upgrade,
    Erase,
    /// Written every `CHECKPOINT_INTERVAL` changes so replay stays short.
    Periodic,
}

/// Changes after a checkpoint before the next periodic one.
pub(crate) const CHECKPOINT_INTERVAL: u64 = 50;

/// Wallet's key in the saved provider statuses.
pub(crate) const WALLET_PROVIDER: &str = "wallet";

/// What a refresh fetched and when it was projected.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refresh {
    /// The projection's clock: the snapshot's `updatedAt`.
    pub committed_at: DateTime<Utc>,
    /// The local calendar day at `committed_at`, so a time zone can't move a month.
    pub calendar_date: NaiveDate,
    pub balances_fresh: bool,
    pub refreshed_account_ids: Vec<String>,
    /// Alpaca's benchmark history, or the saved one when Alpaca wasn't asked or failed.
    pub benchmark: Value,
    /// Notices from fetching, shown with the snapshot.
    pub warnings: Vec<String>,
    pub plaid_cache: PlaidCache,
    pub provider_data: ProviderDataCache,
}

impl Change {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Checkpoint(_) => crate::database::CHECKPOINT,
            Self::Refresh(_) => "refresh",
            Self::Statements(_) => "statements",
            Self::Wallet(_) => "wallet",
            Self::Annotations { .. } => "annotations",
            Self::Settings(_) => "settings",
            Self::AccountLinks(_) => "accountLinks",
            Self::Documents(_) => "documents",
        }
    }

    /// The change as stored, without its kind.
    pub(crate) fn body(&self) -> Result<Value, String> {
        let mut value = serde_json::to_value(self).map_err(|error| error.to_string())?;
        Ok(value
            .get_mut("body")
            .map(Value::take)
            .unwrap_or(Value::Null))
    }

    pub(crate) fn from_stored(kind: &str, body: Value) -> Result<Self, String> {
        serde_json::from_value(serde_json::json!({ "kind": kind, "body": body }))
            .map_err(|error| format!("The change log holds an unreadable {kind} change: {error}"))
    }

    /// The change from `before` to `after` in the annotations.
    pub(crate) fn annotations(
        before: &BTreeMap<String, Annotation>,
        after: &BTreeMap<String, Annotation>,
    ) -> Self {
        Self::Annotations {
            set: after
                .iter()
                .filter(|(id, annotation)| before.get(*id) != Some(annotation))
                .map(|(id, annotation)| (id.clone(), annotation.clone()))
                .collect(),
            removed: before
                .keys()
                .filter(|id| !after.contains_key(*id))
                .cloned()
                .collect(),
        }
    }
}

/// The state after `change`.
pub(crate) fn apply(state: &FinanceState, change: &Change) -> Result<FinanceState, String> {
    match change {
        Change::Checkpoint(checkpoint) => validate_state((*checkpoint.state).clone()),
        Change::Refresh(refresh) => {
            let snapshot = project_refresh(
                &state.snapshot,
                &state.statements,
                &state.account_links,
                refresh,
            )?;
            finish_refresh(state, refresh, snapshot)
        }
        Change::Statements(statements) => reproject(state, statements.clone()),
        Change::Wallet(wallet) => {
            let mut next = state.clone();
            next.provider_data.wallet = wallet.as_deref().cloned();
            match wallet {
                Some(wallet) => {
                    let status = next
                        .provider_data
                        .sync_status
                        .entry(WALLET_PROVIDER.into())
                        .or_default();
                    status.updated_at = Some(wallet.read_at.clone());
                    status.error = None;
                }
                None => {
                    next.provider_data.sync_status.remove(WALLET_PROVIDER);
                }
            }
            let statements = next.statements.clone();
            reproject(&next, statements)
        }
        Change::Annotations { set, removed } => {
            let mut next = state.clone();
            for id in removed {
                next.annotations.remove(id);
            }
            next.annotations
                .extend(set.iter().map(|(id, value)| (id.clone(), value.clone())));
            Ok(next)
        }
        Change::Settings(settings) => Ok(FinanceState {
            settings: (**settings).clone(),
            ..state.clone()
        }),
        Change::AccountLinks(links) => {
            let mut next = state.clone();
            next.account_links = links.clone();
            // The snapshot's copy is read from the same table.
            next.snapshot.account_links = links.clone();
            Ok(next)
        }
        Change::Documents(documents) => {
            for document in documents {
                document.validate()?;
            }
            Ok(FinanceState {
                documents: documents.clone(),
                ..state.clone()
            })
        }
    }
}

/// Why replay stopped short of the saved state.
#[derive(Debug, PartialEq)]
pub(crate) enum ReplayError {
    /// Applying this changeset gave a state other than the one it recorded.
    Diverged { seq: i64, kind: String },
    /// The log couldn't be read or applied.
    Failed(String),
}

impl From<String> for ReplayError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

/// The state the log replays to from its latest checkpoint, and how many changesets that
/// took; `None` before the log begins. Each changeset's result is checked against the state
/// it recorded, so a difference is caught where it starts.
pub(crate) fn replay(
    database: &crate::database::Database,
) -> Result<(Option<FinanceState>, usize), ReplayError> {
    replay_with(database, true).map(|(state, count, _)| (state, count))
}

/// Replays from the latest checkpoint with this code. `strict` stops at the first changeset
/// whose result differs from the state it recorded; otherwise replay goes on, and says
/// whether any differed.
fn replay_with(
    database: &crate::database::Database,
    strict: bool,
) -> Result<(Option<FinanceState>, usize, bool), ReplayError> {
    let changesets = database.since_checkpoint()?;
    let mut state: Option<FinanceState> = None;
    let mut differed = false;
    for changeset in &changesets {
        let change = database.change(changeset)?;
        let next = match (&change, &state) {
            (Change::Checkpoint(checkpoint), _) => validate_state((*checkpoint.state).clone())?,
            (_, Some(state)) => apply(state, &change)?,
            (_, None) => {
                return Err(ReplayError::Failed(
                    "The change log has no checkpoint".into(),
                ))
            }
        };
        if state_hash(&next)? != changeset.state {
            if strict {
                return Err(ReplayError::Diverged {
                    seq: changeset.seq,
                    kind: changeset.kind.clone(),
                });
            }
            differed = true;
        }
        state = Some(next);
    }
    Ok((state, changesets.len(), differed))
}

/// What opening a database must do to its tables so they match the log.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tables {
    /// They hash to the log's latest state.
    Current,
    /// Rewrite them from the replayed state.
    Rebuild,
    /// Replay under this code gave other results than the log recorded (a new field, or a
    /// calculation that changed), so save the replayed state as a checkpoint, which also
    /// rewrites them; later opens compare against it.
    Checkpoint,
}

/// The state the log holds, which is what Brief reads, and what the saved tables need. The
/// tables are the log's cache: they stand when they hash to the state the latest changeset
/// recorded, which costs one hash and no replay. Otherwise the log is replayed from its
/// latest checkpoint with this code, so tables that strayed never reach a reader, and an
/// update that changed how a state is saved doesn't stop Brief from opening. A database
/// whose log hasn't begun keeps its tables, and the log begins from them.
pub(crate) fn authoritative(
    database: &crate::database::Database,
) -> Result<(FinanceState, Tables), String> {
    let saved = database.load()?;
    let Some(logged) = database.logged_state()? else {
        return saved
            .map(|saved| (saved, Tables::Current))
            .ok_or_else(|| "The finance database has no committed state".to_string());
    };
    if let Some(saved) = saved {
        if state_hash(&saved)? == logged {
            return Ok((saved, Tables::Current));
        }
    }
    match replay_with(database, false) {
        Ok((Some(state), _, differed)) => Ok((
            state,
            if differed {
                Tables::Checkpoint
            } else {
                Tables::Rebuild
            },
        )),
        Ok((None, _, _)) => Err("The change log is empty".into()),
        Err(ReplayError::Failed(error)) => Err(error),
        Err(ReplayError::Diverged { seq, kind }) => Err(format!(
            "Replaying changeset {seq} ({kind}) gave another result"
        )),
    }
}

/// Whether the log replays to the saved state.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "LogOutcome")]
pub enum Outcome {
    Matches,
    Differs,
    Failed,
}

/// The latest replay check, for Diagnostics and Data health.
#[derive(Clone, Debug, Serialize, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LogCheck {
    pub checked_at: String,
    pub outcome: Outcome,
    /// Changesets replayed from the latest checkpoint.
    pub replayed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
}

/// Replays the log and compares the result with the saved tables. Reads through its own
/// connection, so it can run beside the app; a commit that lands meanwhile is not a
/// difference.
pub(crate) fn check(database: &crate::database::Database, now: DateTime<Utc>) -> LogCheck {
    let result = |outcome, replayed, detail: Option<String>| LogCheck {
        checked_at: now.to_rfc3339(),
        outcome,
        replayed,
        detail,
    };
    let (replayed, count) = match replay(database) {
        Ok(replayed) => replayed,
        Err(ReplayError::Diverged { seq, kind }) => {
            return result(
                Outcome::Differs,
                0,
                Some(format!(
                    "Replaying changeset {seq} ({kind}) gives another result"
                )),
            )
        }
        Err(ReplayError::Failed(error)) => return result(Outcome::Failed, 0, Some(error)),
    };
    let saved = (|| {
        let before = database.logged_state()?;
        let saved = database
            .load()?
            .map(|state| state_hash(&state))
            .transpose()?;
        let after = database.logged_state()?;
        Ok::<_, String>(saved.is_some_and(|saved| {
            before.as_deref() == Some(saved.as_str()) || after.as_deref() == Some(&saved)
        }))
    })();
    match (replayed, saved) {
        (None, _) => result(Outcome::Failed, 0, Some("The change log is empty".into())),
        (Some(_), Ok(true)) => result(Outcome::Matches, count, None),
        (Some(_), Ok(false)) => result(
            Outcome::Differs,
            count,
            Some("The saved tables differ from the change log; the next launch rebuilds them from it".into()),
        ),
        (Some(_), Err(error)) => result(Outcome::Failed, count, Some(error)),
    }
}

/// The SHA-256 of a state as saved, which each changeset records and replay checks.
pub(crate) fn state_hash(state: &FinanceState) -> Result<String, String> {
    serde_json::to_vec(state)
        .map(|bytes| crate::database::sha256(&bytes))
        .map_err(|error| error.to_string())
}

/// A refresh's snapshot, projected from what it fetched over the snapshot before it. Run
/// outside the storage lock; `finish_refresh` completes the state.
pub(crate) fn project_refresh(
    previous: &Snapshot,
    statements: &[StatementFile],
    account_links: &BTreeMap<String, String>,
    refresh: &Refresh,
) -> Result<Snapshot, String> {
    let data = &refresh.provider_data;
    let sync = ProviderSync {
        balances_fresh: refresh.balances_fresh,
        refreshed_account_ids: refresh.refreshed_account_ids.clone(),
        previous_snapshot: previous.clone(),
        plaid: data.plaid.clone().unwrap_or_default(),
        snaptrade: data.snaptrade.clone().unwrap_or_default(),
        benchmark_history: refresh.benchmark.clone(),
        security_history: data.security_history.clone(),
        statements: statements.to_vec(),
        corporate_actions: data.corporate_actions.clone(),
        wallet: data.wallet.clone(),
    };
    crate::financial_engine::project(
        &sync,
        account_links,
        refresh.committed_at,
        refresh.calendar_date,
    )
}

/// The state a refresh commits: its caches and its projected snapshot, one revision on.
pub(crate) fn finish_refresh(
    state: &FinanceState,
    refresh: &Refresh,
    mut snapshot: Snapshot,
) -> Result<FinanceState, String> {
    validate_snapshot(&snapshot)?;
    let revision = state
        .revision
        .checked_add(1)
        .ok_or("Snapshot revision overflow")?;
    snapshot.revision = Some(revision);
    let mut warnings = refresh.warnings.clone();
    warnings.append(&mut snapshot.sync_warnings);
    warnings.sort();
    warnings.dedup();
    snapshot.sync_warnings = warnings;
    snapshot.provider_status = Some(refresh.provider_data.sync_status.clone());
    Ok(FinanceState {
        schema_version: 1,
        revision,
        snapshot,
        plaid_cache: refresh.plaid_cache.clone(),
        provider_data: refresh.provider_data.clone(),
        ..state.clone()
    })
}

/// Saved provider data projected again, without the network, with these statements, at the
/// clock and calendar day the snapshot was committed with.
pub(crate) fn reproject(
    state: &FinanceState,
    statements: Vec<StatementFile>,
) -> Result<FinanceState, String> {
    let previous = &state.snapshot;
    let updated_at = DateTime::parse_from_rfc3339(&previous.updated_at).ok();
    let now = updated_at
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(DateTime::UNIX_EPOCH);
    let calendar_date = previous
        .calendar_date
        .as_deref()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .or_else(|| updated_at.map(|value| value.date_naive()))
        .unwrap_or_else(|| now.date_naive());
    let sync = ProviderSync {
        balances_fresh: false,
        refreshed_account_ids: Vec::new(),
        previous_snapshot: previous.clone(),
        plaid: state.provider_data.plaid.clone().unwrap_or_default(),
        snaptrade: state.provider_data.snaptrade.clone().unwrap_or_default(),
        benchmark_history: serde_json::to_value(&previous.benchmark_history)
            .map_err(|error| error.to_string())?,
        security_history: state.provider_data.security_history.clone(),
        corporate_actions: state.provider_data.corporate_actions.clone(),
        statements: statements.clone(),
        wallet: state.provider_data.wallet.clone(),
    };
    let mut snapshot =
        crate::financial_engine::project(&sync, &state.account_links, now, calendar_date)?;
    let revision = state
        .revision
        .checked_add(1)
        .ok_or("Snapshot revision overflow")?;
    snapshot.revision = Some(revision);
    snapshot.provider_status = Some(state.provider_data.sync_status.clone());
    if snapshot.last_change.is_none() {
        snapshot.last_change = previous.last_change.clone();
    }
    // Statement warnings come from this projection; earlier ones may no longer apply.
    let mut warnings = previous
        .sync_warnings
        .iter()
        .filter(|warning| !warning.starts_with("Statement"))
        .cloned()
        .chain(std::mem::take(&mut snapshot.sync_warnings))
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();
    snapshot.sync_warnings = warnings;
    validate_state(FinanceState {
        revision,
        snapshot,
        statements,
        ..state.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::wallet::{WalletData, WalletRow};

    fn state() -> FinanceState {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "updatedAt": "2026-10-30T12:00:00Z",
            "calendarDate": "2026-10-30",
            "netWorth": 0.0,
            "accounts": [{
                "id": "all", "name": "All accounts", "institution": "Brief",
                "type": "combined", "value": 0.0
            }],
            "holdings": [],
            "transactions": [],
            "spending": {"monthTotal": 0.0, "categories": []},
            "netWorthHistory": []
        }))
        .unwrap();
        FinanceState {
            snapshot,
            ..crate::storage::empty_state().unwrap()
        }
    }

    fn wallet() -> WalletData {
        WalletData {
            read_at: "2026-10-30T12:05:00Z".into(),
            rows: vec![WalletRow {
                id: "one".into(),
                issuer: "Apple Card".into(),
                amount: 100_000,
                date: "2026-10-29".into(),
                merchant: Some("Corner Market".into()),
                ..WalletRow::default()
            }],
            ..WalletData::default()
        }
    }

    #[test]
    fn a_wallet_change_replays_from_itself_and_commits_no_balance() {
        let before = state();
        let change = Change::Wallet(Some(Box::new(wallet())));
        let stored = Change::from_stored(change.kind(), change.body().unwrap()).unwrap();
        let next = apply(&before, &stored).unwrap();
        // The purchase is the user's; the balance Apple reports never reaches a total.
        assert_eq!(next.snapshot.transactions.len(), 1);
        assert_eq!(next.snapshot.net_worth, 0.0);
        assert!(!next.snapshot.net_worth_incomplete);
        assert_eq!(
            next.provider_data.sync_status[WALLET_PROVIDER]
                .updated_at
                .as_deref(),
            Some("2026-10-30T12:05:00Z")
        );
        // Replaying the same change gives the same state.
        assert_eq!(
            state_hash(&apply(&before, &stored).unwrap()).unwrap(),
            state_hash(&next).unwrap()
        );
    }

    /// What Wallet would change on a copy of the real database, in counts and totals only. It
    /// reads Wallet read-only and writes nothing: set `BRIEF_WALLET_REPORT_DB` to the copy.
    #[test]
    #[ignore]
    fn real_wallet_report() {
        use crate::{insights::calendar::LocalZone, providers::wallet};
        let path = std::env::var("BRIEF_WALLET_REPORT_DB").unwrap();
        let database =
            crate::database::Database::open_read_only(std::path::Path::new(&path)).unwrap();
        let (before, _) = authoritative(&database).unwrap();
        let now = Utc::now();
        let read = wallet::read(now).unwrap();
        println!("records read: {}", read.rows.len());
        let change = Change::Wallet(Some(Box::new(read)));
        let after = apply(&before, &change).unwrap();
        validate_snapshot(&after.snapshot).unwrap();
        let view = |state: &FinanceState| {
            crate::storage::derive_view(state, None, &BTreeMap::new(), now, &LocalZone).unwrap()
        };
        let ((old, _), (new, _)) = (view(&before), view(&after));
        println!(
            "net worth: {} -> {} (incomplete {} -> {})",
            old.snapshot.net_worth,
            new.snapshot.net_worth,
            old.snapshot.net_worth_incomplete,
            new.snapshot.net_worth_incomplete
        );
        println!(
            "transactions: {} -> {}",
            old.snapshot.transactions.len(),
            new.snapshot.transactions.len()
        );
        let summary = new.snapshot.wallet.as_ref().unwrap();
        println!("history from: {:?}", summary.history_from);
        for card in &summary.cards {
            println!(
                "{}: {} records, {} matched, {} waiting, {} review",
                card.issuer, card.records, card.matched, card.waiting, card.review
            );
        }
        println!("declined: {}", summary.declined.len());
        println!("daily cash: {:?}", summary.daily_cash);
        let apple = new
            .snapshot
            .transactions
            .iter()
            .filter(|transaction| transaction.paid_by_someone_else())
            .collect::<Vec<_>>();
        let spent = apple
            .iter()
            .filter(|t| t.classification.as_ref().is_some_and(|c| c.spending))
            .collect::<Vec<_>>();
        println!(
            "apple card: {} rows, {} spending, ${:.2}",
            apple.len(),
            spent.len(),
            spent.iter().map(|t| t.amount.abs()).sum::<f64>()
        );
        let count = |name: &str, test: &dyn Fn(&crate::finance_contract::Transaction) -> bool| {
            println!("{name}: {}", spent.iter().filter(|t| test(t)).count());
        };
        use crate::merchants::Presence;
        count("in person", &|t| t.presence == Some(Presence::InPerson));
        count("remote", &|t| t.presence == Some(Presence::Remote));
        count("no presence", &|t| t.presence.is_none());
        count("place", &|t| t.place.is_some());
        count("place exact", &|t| {
            t.place
                .as_ref()
                .is_some_and(|p| p.precision == crate::places::PlacePrecision::Exact)
        });
        count("time", &|t| t.occurred_at.is_some());
        let mut categories = BTreeMap::<&str, (usize, f64)>::new();
        for transaction in &spent {
            let entry = categories.entry(&transaction.category).or_default();
            entry.0 += 1;
            entry.1 += transaction.amount.abs();
        }
        println!("categories: {categories:?}");
        let refunds = apple
            .iter()
            .filter(|t| t.refund_of.is_some())
            .collect::<Vec<_>>();
        println!(
            "linked refunds: {}, ${:.2}",
            refunds.len(),
            refunds.iter().map(|t| t.amount).sum::<f64>()
        );
        let matched_time = new
            .snapshot
            .transactions
            .iter()
            .filter(|t| !t.from_wallet() && t.wallet.is_some())
            .count();
        println!("provider rows with Wallet's detail: {matched_time}");
        println!(
            "pending from wallet: {}",
            new.snapshot
                .transactions
                .iter()
                .filter(|t| t.from_wallet() && t.pending)
                .count()
        );
        if let (Some(old), Some(new)) = (&old.insights, &new.insights) {
            println!(
                "trips: {} -> {}; home: {:?}",
                old.trips.trips.len(),
                new.trips.trips.len(),
                new.trips.home
            );
            for trip in new.trips.trips.iter().take(12) {
                println!("  {} {} to {}", trip.destination, trip.start, trip.end);
            }
        }
    }

    /// Commits saved provider data as a refresh would, on a copy of the real database, and says
    /// why validation refuses it. Set `BRIEF_WALLET_REPORT_DB` to the copy; nothing is written.
    #[test]
    #[ignore]
    fn real_refresh_report() {
        let path = std::env::var("BRIEF_WALLET_REPORT_DB").unwrap();
        let database =
            crate::database::Database::open_read_only(std::path::Path::new(&path)).unwrap();
        let (state, _) = authoritative(&database).unwrap();
        let now = Utc::now();
        let refresh = Refresh {
            committed_at: now,
            calendar_date: now.date_naive(),
            balances_fresh: true,
            refreshed_account_ids: state
                .snapshot
                .accounts
                .iter()
                .filter(|account| account.id != "all")
                .map(|account| account.id.clone())
                .collect(),
            benchmark: serde_json::to_value(&state.snapshot.benchmark_history).unwrap(),
            warnings: Vec::new(),
            plaid_cache: state.plaid_cache.clone(),
            provider_data: state.provider_data.clone(),
        };
        let snapshot = project_refresh(
            &state.snapshot,
            &state.statements,
            &state.account_links,
            &refresh,
        );
        match snapshot {
            Err(error) => println!("projection refused: {error}"),
            Ok(snapshot) => match finish_refresh(&state, &refresh, snapshot) {
                Err(error) => println!("validation refused: {error}"),
                Ok(next) => println!(
                    "accepted: net worth {} -> {}",
                    state.snapshot.net_worth, next.snapshot.net_worth
                ),
            },
        }
    }

    #[test]
    fn turning_wallet_off_clears_its_records_and_status() {
        let on = apply(&state(), &Change::Wallet(Some(Box::new(wallet())))).unwrap();
        let change = Change::Wallet(None);
        let stored = Change::from_stored(change.kind(), change.body().unwrap()).unwrap();
        let off = apply(&on, &stored).unwrap();
        assert!(off.snapshot.transactions.is_empty());
        assert!(off.snapshot.wallet.is_none());
        assert!(off.provider_data.wallet.is_none());
        assert!(!off.provider_data.sync_status.contains_key(WALLET_PROVIDER));
    }
}
