//! Keeps saved bank statements current for the Plaid connections the user added statements to.
//! A check lists each connection's statements and downloads the new ones; once a month it also
//! asks Plaid to fetch statements the bank has posted since. Plaid reports that fetch finishing
//! only by webhook, which a desktop app can't receive, so a later check lists what it found.
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    database::{BankStatement, StatementCheck},
    AppState,
};

/// A connection's statements are listed at most this often, unless the user asks.
const LIST_EVERY: Duration = Duration::hours(24);
/// Plaid is asked to fetch new statements from the bank at most this often.
const REQUEST_EVERY: Duration = Duration::days(30);
/// How far back each monthly request reaches, so a statement posted late is still found.
const REQUEST_DAYS: i64 = 62;
/// Statements downloaded in one check; the rest wait for the next.
const MAX_DOWNLOADS: usize = 40;
const MAX_FAILURES: usize = 5;

/// What one check found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatementSync {
    /// Statements Plaid listed across the checked connections.
    pub available: usize,
    /// Statements saved by this check.
    pub saved: usize,
    pub failures: Vec<String>,
}

static RUNNING: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

/// Checks each connection with statements: all of them when `force`, otherwise those not listed
/// within a day. Network calls hold no lock on saved data.
pub async fn sync(
    state: &AppState,
    now: DateTime<Utc>,
    force: bool,
) -> Result<StatementSync, String> {
    let _running = RUNNING
        .try_lock()
        .map_err(|_| "Statements are already being checked")?;
    let mut outcome = StatementSync::default();
    let mut downloads_left = MAX_DOWNLOADS;
    for item_id in state.providers.statement_items()? {
        let check = state.storage()?.statement_check(&item_id)?;
        let due = |at: &Option<String>, every: Duration| {
            at.as_deref()
                .and_then(parsed)
                .is_none_or(|at| now - at >= every)
        };
        if !force && !due(&check.listed_at, LIST_EVERY) {
            continue;
        }
        // Adding statements fetches two years of them, so the first check only starts the clock.
        let mut requested_at = None;
        if check.requested_at.is_none() {
            requested_at = Some(now.to_rfc3339());
        } else if due(&check.requested_at, REQUEST_EVERY) {
            let today = now.date_naive();
            match state
                .providers
                .request_statements(&item_id, today - Duration::days(REQUEST_DAYS), today)
                .await
            {
                Ok(()) => requested_at = Some(now.to_rfc3339()),
                Err(error) => note(&mut outcome, error),
            }
        }
        let listed = match state.providers.list_statements(&item_id).await {
            Ok(listed) => listed,
            Err(error) => {
                note(&mut outcome, error);
                record(state, &item_id, None, requested_at)?;
                continue;
            }
        };
        outcome.available += listed.len();
        let saved = state.storage()?.bank_statement_ids()?;
        let mut complete = true;
        for statement in listed
            .iter()
            .filter(|statement| !saved.contains(&statement.statement_id))
        {
            if downloads_left == 0 {
                complete = false;
                break;
            }
            downloads_left -= 1;
            match state
                .providers
                .download_statement(&item_id, &statement.statement_id)
                .await
            {
                Ok(contents) => {
                    let digest = Sha256::digest(&contents)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    state.storage()?.save_bank_statement(
                        &BankStatement {
                            id: statement.statement_id.clone(),
                            item_id: statement.item_id.clone(),
                            account_id: statement.account_id.clone(),
                            account_name: statement.account_name.clone(),
                            year: statement.year,
                            month: statement.month,
                            date_posted: statement.date_posted.clone(),
                            fetched_at: now.to_rfc3339(),
                            bytes: contents.len() as u64,
                        },
                        &digest,
                        &contents,
                    )?;
                    outcome.saved += 1;
                }
                Err(error) => {
                    complete = false;
                    note(&mut outcome, error);
                }
            }
        }
        // An unfinished check stays due, so the next one picks up where this stopped.
        record(
            state,
            &item_id,
            complete.then(|| now.to_rfc3339()),
            requested_at,
        )?;
    }
    Ok(outcome)
}

/// Checks statements in the background after a refresh, when any connection is due.
pub fn sync_in_background(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        let state = app.state::<AppState>();
        if state
            .providers
            .statement_items()
            .is_ok_and(|items| !items.is_empty())
        {
            let _ = sync(&state, Utc::now(), false).await;
        }
    });
}

fn record(
    state: &AppState,
    item_id: &str,
    listed_at: Option<String>,
    requested_at: Option<String>,
) -> Result<(), String> {
    state.storage()?.record_statement_check(
        item_id,
        &StatementCheck {
            listed_at,
            requested_at,
        },
    )
}

fn note(outcome: &mut StatementSync, failure: String) {
    if outcome.failures.len() < MAX_FAILURES && !outcome.failures.contains(&failure) {
        outcome.failures.push(failure);
    }
}

fn parsed(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}
