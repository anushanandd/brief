use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    database::SyncDiagnostics,
    financial_engine,
    providers::{self, PlaidData, PlaidRefreshProgress, ProviderSync},
    AppState,
};

const HISTORY_CACHE_HOURS: i64 = 12;

pub(super) fn history_cache_is_fresh(refreshed_at: Option<&str>, now: DateTime<Utc>) -> bool {
    refreshed_at
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| now.signed_duration_since(value.with_timezone(&Utc)))
        .is_some_and(|age| age >= Duration::zero() && age < Duration::hours(HISTORY_CACHE_HOURS))
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceRefreshTask {
    id: String,
    label: String,
    status: String,
    state: &'static str,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceRefreshProgress {
    title: &'static str,
    detail: &'static str,
    task: Option<FinanceRefreshTask>,
}

fn report_refresh(app: &AppHandle, title: &'static str, detail: &'static str) {
    let _ = app.emit(
        "finance-refresh-progress",
        FinanceRefreshProgress {
            title,
            detail,
            task: None,
        },
    );
}

fn report_refresh_task(
    app: &AppHandle,
    id: impl Into<String>,
    label: impl Into<String>,
    status: impl Into<String>,
    task_state: &'static str,
) {
    let _ = app.emit(
        "finance-refresh-progress",
        FinanceRefreshProgress {
            title: "Refreshing financial data…",
            detail: "Connected sources update independently, so completed checks appear as they finish.",
            task: Some(FinanceRefreshTask {
                id: id.into(),
                label: label.into(),
                status: status.into(),
                state: task_state,
            }),
        },
    );
}

/// Reports how a provider's refresh ended: with its notices, or `None` when it failed.
fn report_provider_outcome(
    app: &AppHandle,
    id: &str,
    label: &str,
    started: Instant,
    notices: Option<&[String]>,
) {
    let (status, task_state) = match notices {
        Some([]) => ("Complete", "complete"),
        Some(_) => ("Completed with notices", "warning"),
        None => ("Could not update", "warning"),
    };
    let status = format!("{status} · {:.1}s", started.elapsed().as_secs_f64());
    report_refresh_task(app, id, label, status, task_state);
}

pub(super) fn update_or_warn<T>(
    cached: &mut Option<T>,
    result: Result<T, String>,
    provider: &str,
    warnings: &mut Vec<String>,
) -> bool {
    match result {
        Ok(value) => {
            *cached = Some(value);
            true
        }
        Err(error) => {
            warnings.push(format!("{provider}: {error}"));
            false
        }
    }
}

const TIMED_OUT: &str = "Refresh timed out; saved data unchanged";

/// A refresh that failed before its caches were staged, with what to record about it.
struct PrepareFailure {
    message: String,
    diagnostics: SyncDiagnostics,
}

impl From<String> for PrepareFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            diagnostics: SyncDiagnostics::new("provider", Vec::new(), 0),
        }
    }
}

/// Records a staged refresh as failed and returns its error.
fn fail_staged(state: &AppState, sync_id: &str, code: &str, error: impl Into<String>) -> String {
    if let Ok(mut storage) = state.storage() {
        storage.fail_pending(sync_id, code);
    }
    error.into()
}

#[tauri::command]
pub(crate) async fn refresh_finance_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    live_balances: Option<bool>,
) -> Result<Value, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "A refresh is already running")?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(120);
    report_refresh(&app, "Preparing refresh…", "Preparing refresh…");
    let fetching = Instant::now();
    let (prepared, usage) = providers::http::counted(tokio::time::timeout_at(
        deadline,
        prepare_refresh(
            &app,
            &state,
            &started_at,
            &run_id,
            live_balances.unwrap_or(false),
        ),
    ))
    .await;
    let fetch_ms = crate::database::millis(fetching.elapsed());
    let sync = match prepared {
        Ok(Ok(sync)) => sync,
        failed => {
            let (code, failure) = match failed {
                Ok(Err(failure)) => ("provider_refresh_failed", failure),
                _ => (
                    "refresh_timeout",
                    PrepareFailure {
                        message: TIMED_OUT.into(),
                        diagnostics: SyncDiagnostics::new("timeout", Vec::new(), 0),
                    },
                ),
            };
            if let Ok(mut storage) = state.storage() {
                let diagnostics = failure.diagnostics.with_usage(usage, fetch_ms);
                storage.record_failed_sync(&run_id, &started_at, code, diagnostics);
            }
            return Err(failure.message);
        }
    };
    state.storage()?.note_usage(&sync.sync_id, usage, fetch_ms);
    let account_links = state.storage()?.data.account_links.clone();
    let sync_id = sync.sync_id.clone();
    let projection_now = Utc::now();
    let calendar_date = projection_now.with_timezone(&chrono::Local).date_naive();
    let projecting = Instant::now();
    let projection = tauri::async_runtime::spawn_blocking(move || {
        financial_engine::project(&sync, &account_links, projection_now, calendar_date)
    });
    let projected = tokio::time::timeout_at(deadline, projection).await;
    state
        .storage()?
        .note_projection(&sync_id, projecting.elapsed());
    let snapshot = match projected {
        Ok(Ok(Ok(snapshot))) => snapshot,
        Ok(Ok(Err(error))) => {
            return Err(fail_staged(&state, &sync_id, "projection_failed", error))
        }
        Ok(Err(_)) => {
            return Err(fail_staged(
                &state,
                &sync_id,
                "projection_failed",
                "Local refresh worker failed",
            ))
        }
        Err(_) => return Err(fail_staged(&state, &sync_id, "refresh_timeout", TIMED_OUT)),
    };
    let committer = app.clone();
    let committed = tauri::async_runtime::spawn_blocking(move || {
        let state = committer.state::<AppState>();
        let mut storage = state.storage()?;
        if tokio::time::Instant::now() >= deadline {
            storage.fail_pending(&sync_id, "refresh_timeout");
            return Err(TIMED_OUT.into());
        }
        storage.commit(&sync_id, snapshot).map_err(|error| {
            storage.fail_pending(&sync_id, error.code());
            error.to_string()
        })
    })
    .await
    .map_err(|_| "Local refresh worker failed".to_string())?;
    if committed.is_ok() {
        crate::calendar::sync_in_background(app.clone());
        crate::bank_statements::sync_in_background(app);
    }
    committed
}

#[tauri::command(async)]
pub(crate) fn save_account_link(
    plaid_account_id: String,
    snaptrade_account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .storage()?
        .save_account_link(plaid_account_id, snaptrade_account_id)
}

async fn prepare_refresh(
    app: &AppHandle,
    state: &AppState,
    started_at: &str,
    run_id: &str,
    live_balances: bool,
) -> Result<ProviderSync, PrepareFailure> {
    state.storage()?.ensure_writable()?;
    state.providers.unlock_credentials()?;
    let integrations = state.providers.integration_status()?;

    let (plaid_cache, mut provider_data, revision, previous, plaid_options) = {
        let storage = state.storage()?;
        let settings = &storage.data.settings;
        (
            storage.data.plaid_cache.clone(),
            storage.data.provider_data.clone(),
            storage.data.revision,
            storage.data.snapshot.clone(),
            providers::PlaidSyncOptions {
                live_balances,
                liabilities: settings.plaid_liabilities,
                investment_transactions: settings.plaid_investment_transactions,
                recurring: settings.plaid_recurring,
            },
        )
    };
    let had_accounts = |prefix: &str| {
        previous
            .accounts
            .iter()
            .any(|account| account.id.starts_with(prefix))
    };
    let (had_plaid, had_snaptrade) = (had_accounts("plaid:"), had_accounts("snaptrade:"));
    let benchmark_compatible = previous.provenance.as_ref().is_some_and(|provenance| {
        provenance.benchmark.provider == "Alpaca" && provenance.benchmark.adjustment == "all"
    });
    let json = |value: Result<Value, serde_json::Error>| value.map_err(|error| error.to_string());
    let previous_benchmark = if benchmark_compatible {
        json(serde_json::to_value(&previous.benchmark_history))?
    } else {
        Value::Array(Vec::new())
    };
    let market_cache_compatible = benchmark_compatible
        && provider_data.security_history_adjustment.as_deref() == Some("split");

    let now = Utc::now();
    let refresh_snaptrade_history =
        !history_cache_is_fresh(provider_data.snaptrade_history_refreshed_at.as_deref(), now);
    let backfill_snaptrade_activity = !history_cache_is_fresh(
        provider_data.snaptrade_activity_backfilled_at.as_deref(),
        now,
    );
    if provider_data.security_history_adjustment.as_deref() != Some("split") {
        provider_data.security_history.clear();
        provider_data.security_history_adjustment = None;
        provider_data.market_history_refreshed_at = None;
    }
    let refresh_market_history = !market_cache_compatible
        || !history_cache_is_fresh(provider_data.market_history_refreshed_at.as_deref(), now);
    let mut next_plaid_cache = plaid_cache.clone();
    if integrations.plaid {
        report_refresh_task(app, "plaid", "Plaid", "Checking", "active");
    }
    if integrations.snaptrade {
        report_refresh_task(app, "snaptrade", "SnapTrade", "Checking", "active");
    }
    let plaid_progress = |progress: PlaidRefreshProgress| {
        report_refresh_task(
            app,
            progress.id,
            progress.label,
            progress.status,
            progress.state,
        );
    };
    let plaid_sync = async {
        let started = Instant::now();
        let result = state
            .providers
            .sync_plaid(
                &mut next_plaid_cache,
                provider_data.plaid.as_ref(),
                plaid_options,
                &plaid_progress,
            )
            .await;
        if integrations.plaid {
            providers::http::count_duration("Plaid", started.elapsed());
            let notices = result.as_ref().ok().map(|data| data.warnings.as_slice());
            report_provider_outcome(app, "plaid", "Plaid", started, notices);
        }
        result
    };
    let snaptrade_sync = async {
        let started = Instant::now();
        let result = state
            .providers
            .sync_snaptrade(
                provider_data.snaptrade.as_ref(),
                refresh_snaptrade_history,
                backfill_snaptrade_activity,
            )
            .await;
        if integrations.snaptrade {
            providers::http::count_duration("SnapTrade", started.elapsed());
            let notices = result.as_ref().ok().map(|data| data.warnings.as_slice());
            report_provider_outcome(app, "snaptrade", "SnapTrade", started, notices);
        }
        result
    };
    let benchmark_sync = bounded_fetch(
        refresh_market_history,
        "Benchmark history",
        state.providers.sync_benchmark(),
    );
    let (plaid, snaptrade, benchmark) = tokio::join!(plaid_sync, snaptrade_sync, benchmark_sync);
    let mut provider_diagnostics = plaid
        .as_ref()
        .map(|data| data.diagnostics.clone())
        .unwrap_or_else(|error| vec![error.diagnostic()]);
    if snaptrade.is_err() {
        provider_diagnostics.push(providers::http::ProviderDiagnostic {
            provider: "SnapTrade".into(),
            endpoint: "/refresh".into(),
            kind: "provider".into(),
            http_status: None,
            error_type: None,
            error_code: None,
            request_ref: None,
            attempts: 1,
            retryable: false,
            retry_at: None,
        });
    }
    let mut warnings = Vec::new();
    let ProviderResults {
        plaid_updated,
        plaid_complete,
        snaptrade_updated,
        refreshed_account_ids,
    } = record_provider_results(
        &mut provider_data,
        plaid.map_err(|error| error.to_string()),
        snaptrade,
        HistoryDue {
            snaptrade_history: refresh_snaptrade_history,
            snaptrade_activity: backfill_snaptrade_activity,
        },
        now,
        &mut warnings,
    );
    if !plaid_updated {
        next_plaid_cache = plaid_cache;
    }
    if (!plaid_updated && provider_data.plaid.is_none() && had_plaid)
        || (!snaptrade_updated && provider_data.snaptrade.is_none() && had_snaptrade)
    {
        return Err(PrepareFailure {
            message: "Could not refresh safely without a provider cache; saved data is unchanged"
                .into(),
            diagnostics: SyncDiagnostics::new("provider", provider_diagnostics, warnings.len()),
        });
    }
    let fresh_benchmark = fetched(benchmark, &mut warnings)
        .filter(|history| history.as_array().is_some_and(|points| !points.is_empty()));
    let benchmark_refreshed = fresh_benchmark.is_some();
    let benchmark = fresh_benchmark.unwrap_or(previous_benchmark);

    // Security history and corporate actions both follow the holdings just fetched, and
    // neither waits on the other.
    let empty_plaid = PlaidData::default();
    let security_sync = bounded_fetch(
        refresh_market_history,
        "Market history",
        state.providers.sync_security_history(
            provider_data.plaid.as_ref().unwrap_or(&empty_plaid),
            provider_data.snaptrade.as_ref(),
        ),
    );
    // Splits correct saved trades and dividends price upcoming income. A failure keeps the
    // saved records, which stay correct for everything before them.
    let corporate_actions_due = integrations.alpaca
        && !history_cache_is_fresh(provider_data.corporate_actions_refreshed_at.as_deref(), now);
    let symbols = corporate_action_symbols(&provider_data);
    let since = (now - chrono::Duration::days(CORPORATE_ACTION_YEARS * 365)).date_naive();
    let corporate_sync = bounded_fetch(
        corporate_actions_due,
        "Splits and dividends",
        state.providers.sync_corporate_actions(&symbols, since),
    );
    let (security_history, corporate_actions) = tokio::join!(security_sync, corporate_sync);
    let security_history = fetched(security_history, &mut warnings);
    let security_history_refreshed = security_history.is_some();
    if let Some(history) = security_history {
        provider_data.security_history = history;
        provider_data.security_history_adjustment = Some("split".into());
    }
    if benchmark_refreshed && security_history_refreshed {
        provider_data.market_history_refreshed_at = Some(now.to_rfc3339());
    }
    if let Some(actions) = fetched(corporate_actions, &mut warnings) {
        provider_data.corporate_actions = actions;
        provider_data.corporate_actions_refreshed_at = Some(now.to_rfc3339());
    }

    let mut storage = state.storage()?;
    let sync_id = storage.stage_refresh(
        run_id,
        started_at,
        revision,
        crate::storage::Staged {
            plaid_cache: next_plaid_cache,
            provider_data: provider_data.clone(),
            warnings: warnings.clone(),
            diagnostics: SyncDiagnostics::new("provider", provider_diagnostics, warnings.len()),
        },
    )?;

    // Statement imports wait for the refresh lock, so these can't change before the commit.
    let statements = storage.data.statements.clone();
    drop(storage);
    Ok(ProviderSync {
        sync_id,
        balances_fresh: (!integrations.plaid || plaid_complete)
            && (!integrations.snaptrade || snaptrade_updated),
        refreshed_account_ids,
        previous_snapshot: previous,
        plaid: provider_data.plaid.unwrap_or_default(),
        snaptrade: provider_data.snaptrade.unwrap_or_default(),
        benchmark_history: benchmark,
        security_history: provider_data.security_history,
        statements,
        corporate_actions: provider_data.corporate_actions,
    })
}

/// How long one market source may hold a refresh.
const MARKET_FETCH_TIMEOUT: StdDuration = StdDuration::from_secs(12);

/// Market data fetched only when `due`, and bounded so a slow source can't hold the refresh. A
/// failure or timeout becomes a warning naming `label`.
async fn bounded_fetch<T>(
    due: bool,
    label: &str,
    fetch: impl std::future::Future<Output = Result<T, String>>,
) -> Option<Result<T, String>> {
    if !due {
        return None;
    }
    Some(
        match tokio::time::timeout(MARKET_FETCH_TIMEOUT, fetch).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(format!("{label}: {error}")),
            Err(_) => Err(format!("{label}: request timed out")),
        },
    )
}

/// What a bounded fetch returned, recording its warning when it failed.
fn fetched<T>(result: Option<Result<T, String>>, warnings: &mut Vec<String>) -> Option<T> {
    result?.map_err(|warning| warnings.push(warning)).ok()
}

/// Which SnapTrade histories this refresh asked for again.
struct HistoryDue {
    snaptrade_history: bool,
    snaptrade_activity: bool,
}

/// What the provider syncs updated, and the accounts whose balances they refreshed.
struct ProviderResults {
    plaid_updated: bool,
    /// Plaid updated without notices.
    plaid_complete: bool,
    snaptrade_updated: bool,
    refreshed_account_ids: Vec<String>,
}

/// Saves each provider's result in its cache, or keeps the last good cache with a warning, and
/// records each provider's status and when its histories were last complete.
fn record_provider_results(
    provider_data: &mut providers::ProviderDataCache,
    plaid: Result<PlaidData, String>,
    snaptrade: Result<providers::SnapTradeData, String>,
    due: HistoryDue,
    now: DateTime<Utc>,
    warnings: &mut Vec<String>,
) -> ProviderResults {
    for (name, error) in [
        ("plaid", plaid.as_ref().err()),
        ("snaptrade", snaptrade.as_ref().err()),
    ] {
        let status = provider_data.sync_status.entry(name.into()).or_default();
        status.error = error.cloned();
        if error.is_none() {
            status.updated_at = Some(now.to_rfc3339());
        }
    }
    let plaid_updated = update_or_warn(&mut provider_data.plaid, plaid, "Plaid", warnings);
    let mut refreshed_account_ids = if plaid_updated {
        provider_data
            .plaid
            .as_ref()
            .map(|data| data.refreshed_account_ids.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let plaid_complete = plaid_updated
        && provider_data
            .plaid
            .as_ref()
            .is_none_or(|data| data.warnings.is_empty());
    if let Some(data) = &provider_data.plaid {
        warnings.extend(data.warnings.clone());
        if !data.warnings.is_empty() {
            provider_data
                .sync_status
                .entry("plaid".into())
                .or_default()
                .error = Some(data.warnings.join(" · "));
        }
    }
    let snaptrade_updated = update_or_warn(
        &mut provider_data.snaptrade,
        snaptrade,
        "SnapTrade",
        warnings,
    );
    if let Some(data) = &provider_data.snaptrade {
        warnings.extend(data.warnings.clone());
        if snaptrade_updated {
            refreshed_account_ids.extend(
                data.accounts
                    .iter()
                    .filter_map(|account| account.get("id").and_then(Value::as_str))
                    .map(|id| format!("snaptrade:{id}")),
            );
        }
    }
    if due.snaptrade_history
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.history_complete)
    {
        provider_data.snaptrade_history_refreshed_at = Some(now.to_rfc3339());
    }
    if due.snaptrade_activity
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.activity_complete)
    {
        provider_data.snaptrade_activity_backfilled_at = Some(now.to_rfc3339());
    }
    ProviderResults {
        plaid_updated,
        plaid_complete,
        snaptrade_updated,
        refreshed_account_ids,
    }
}

/// How far back splits are fetched: far enough for any saved trade history.
const CORPORATE_ACTION_YEARS: i64 = 10;

/// Every symbol held or traded, for splits and dividends.
fn corporate_action_symbols(data: &providers::ProviderDataCache) -> Vec<String> {
    let mut symbols = std::collections::BTreeSet::new();
    let mut add = |value: Option<&Value>| {
        if let Some(symbol) = value
            .and_then(Value::as_str)
            .map(|symbol| symbol.trim().to_ascii_uppercase())
            .filter(|symbol| {
                !symbol.is_empty()
                    && symbol.len() <= 24
                    && symbol.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                    })
            })
        {
            symbols.insert(symbol);
        }
    };
    if let Some(plaid) = &data.plaid {
        for security in &plaid.securities {
            add(security.get("ticker_symbol"));
        }
    }
    if let Some(snaptrade) = &data.snaptrade {
        for position in snaptrade.positions.values().flatten() {
            add(position.pointer("/instrument/symbol"));
            add(position.pointer("/instrument/raw_symbol"));
        }
        for activity in snaptrade.activities.values().flatten() {
            add(activity.pointer("/symbol/symbol"));
            add(activity.pointer("/symbol/raw_symbol"));
        }
    }
    symbols.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_failures_keep_last_good_data_and_add_a_warning() {
        let mut cached = Some(vec!["saved"]);
        let mut warnings = Vec::new();

        assert!(!update_or_warn(
            &mut cached,
            Err("offline".into()),
            "Plaid",
            &mut warnings,
        ));
        assert_eq!(cached, Some(vec!["saved"]));
        assert_eq!(warnings, vec!["Plaid: offline"]);

        assert!(update_or_warn(
            &mut cached,
            Ok(vec!["fresh"]),
            "Plaid",
            &mut warnings,
        ));
        assert_eq!(cached, Some(vec!["fresh"]));
    }

    #[test]
    fn a_failed_provider_keeps_its_cache_and_a_good_one_refreshes_its_accounts() {
        let now = "2026-09-04T16:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let mut data = providers::ProviderDataCache {
            plaid: Some(PlaidData {
                refreshed_account_ids: vec!["plaid:saved".into()],
                ..PlaidData::default()
            }),
            ..Default::default()
        };
        let mut warnings = Vec::new();
        let results = record_provider_results(
            &mut data,
            Err("offline".into()),
            Ok(providers::SnapTradeData {
                accounts: vec![serde_json::json!({ "id": "brokerage" })],
                history_complete: true,
                ..Default::default()
            }),
            HistoryDue {
                snaptrade_history: true,
                snaptrade_activity: true,
            },
            now,
            &mut warnings,
        );
        assert!(!results.plaid_updated && !results.plaid_complete);
        assert!(results.snaptrade_updated);
        assert_eq!(results.refreshed_account_ids, ["snaptrade:brokerage"]);
        assert_eq!(warnings, ["Plaid: offline"]);
        assert_eq!(
            data.plaid.unwrap().refreshed_account_ids,
            ["plaid:saved"],
            "the saved cache stays"
        );
        assert_eq!(data.sync_status["plaid"].error.as_deref(), Some("offline"));
        assert!(data.sync_status["snaptrade"].updated_at.is_some());
        // Only a complete history moves its timestamp.
        assert!(data.snaptrade_history_refreshed_at.is_some());
        assert!(data.snaptrade_activity_backfilled_at.is_none());
    }

    #[test]
    fn historical_data_expires_after_twelve_hours() {
        let now = "2026-09-04T16:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(history_cache_is_fresh(Some("2026-09-04T05:00:01Z"), now));
        assert!(!history_cache_is_fresh(Some("2026-09-04T04:00:00Z"), now));
        assert!(!history_cache_is_fresh(Some("invalid"), now));
    }

    #[tokio::test]
    async fn market_fetches_run_only_when_due_and_label_their_failures() {
        let mut warnings = Vec::new();
        let skipped = bounded_fetch(false, "Benchmark history", async { Ok::<_, String>(1) });
        assert_eq!(fetched(skipped.await, &mut warnings), None);
        let found = bounded_fetch(true, "Benchmark history", async { Ok::<_, String>(1) });
        assert_eq!(fetched(found.await, &mut warnings), Some(1));
        assert!(warnings.is_empty());
        let failed = bounded_fetch(true, "Market history", async {
            Err::<u8, _>("HTTP 503".to_string())
        });
        assert_eq!(fetched(failed.await, &mut warnings), None);
        assert_eq!(warnings, ["Market history: HTTP 503"]);
    }
}
