use super::*;

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

fn timed_status(status: &str, started: Instant) -> String {
    format!("{status} · {:.1}s", started.elapsed().as_secs_f64())
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

#[tauri::command]
pub(crate) async fn refresh_finance_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "A refresh is already running")?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(120);
    report_refresh(
        &app,
        "Preparing refresh…",
        "Unlocking credentials and reading the last committed snapshot.",
    );
    let sync = match tokio::time::timeout_at(
        deadline,
        prepare_refresh(&app, &state, &run_id, &started_at),
    )
    .await
    {
        Ok(Ok(sync)) => sync,
        result => {
            let (error_code, phase) = if result.is_err() {
                ("refresh_timeout", "timeout")
            } else {
                ("provider_refresh_failed", "provider")
            };
            if let Ok(mut storage) = state.storage.lock() {
                storage.record_failed_sync(
                    &run_id,
                    &started_at,
                    error_code,
                    SyncDiagnostics::new(phase, Vec::new(), 0),
                );
            }
            return match result {
                Err(_) => Err("Account refresh timed out; your saved data is unchanged".into()),
                Ok(Err(error)) => Err(error),
                Ok(Ok(_)) => unreachable!(),
            };
        }
    };
    let account_links = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .data
        .account_links
        .clone();
    let sync_id = sync.sync_id.clone();
    let projection_now = Utc::now();
    let calendar_date = projection_now.with_timezone(&chrono::Local).date_naive();
    let projection = tauri::async_runtime::spawn_blocking(move || {
        financial_engine::project(&sync, &account_links, projection_now, calendar_date)
    });
    let snapshot = match tokio::time::timeout_at(deadline, projection).await {
        Ok(Ok(Ok(snapshot))) => snapshot,
        Ok(Ok(Err(error))) => {
            if let Ok(mut storage) = state.storage.lock() {
                storage.fail_pending(&sync_id, "projection_failed");
            }
            return Err(error);
        }
        Ok(Err(_)) => {
            if let Ok(mut storage) = state.storage.lock() {
                storage.fail_pending(&sync_id, "projection_failed");
            }
            return Err("Local refresh worker failed".into());
        }
        Err(_) => {
            if let Ok(mut storage) = state.storage.lock() {
                storage.fail_pending(&sync_id, "refresh_timeout");
            }
            return Err("Account refresh timed out; your saved data is unchanged".into());
        }
    };
    if tokio::time::Instant::now() >= deadline {
        if let Ok(mut storage) = state.storage.lock() {
            storage.fail_pending(&sync_id, "refresh_timeout");
        }
        return Err("Account refresh timed out; your saved data is unchanged".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable")?;
        if tokio::time::Instant::now() >= deadline {
            storage.fail_pending(&sync_id, "refresh_timeout");
            return Err("Account refresh timed out; your saved data is unchanged".into());
        }
        match storage.commit(&sync_id, snapshot) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                storage.fail_pending(&sync_id, error.code());
                Err(error.to_string())
            }
        }
    })
    .await
    .map_err(|_| "Local refresh worker failed".to_string())?
}

#[tauri::command]
pub(crate) fn save_account_link(
    plaid_account_id: String,
    snaptrade_account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .save_account_link(plaid_account_id, snaptrade_account_id)
}

async fn prepare_refresh(
    app: &AppHandle,
    state: &AppState,
    run_id: &str,
    started_at: &str,
) -> Result<ProviderSync, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .ensure_writable()?;
    state.providers.unlock_credentials()?;
    let integrations = state.providers.integration_status()?;

    let (
        plaid_cache,
        mut provider_data,
        previous_benchmark,
        had_plaid,
        had_snaptrade,
        revision,
        previous_snapshot,
        market_cache_compatible,
    ) = {
        let storage = state
            .storage
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())?;
        let snapshot = &storage.data.snapshot;
        let benchmark_compatible = snapshot
            .pointer("/provenance/benchmark/provider")
            .and_then(Value::as_str)
            == Some("Alpaca")
            && snapshot
                .pointer("/provenance/benchmark/adjustment")
                .and_then(Value::as_str)
                == Some("all");
        let security_history_compatible = storage
            .data
            .provider_data
            .security_history_adjustment
            .as_deref()
            == Some("split");
        (
            storage.data.plaid_cache.clone(),
            storage.data.provider_data.clone(),
            if benchmark_compatible {
                snapshot
                    .get("benchmarkHistory")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new()))
            } else {
                Value::Array(Vec::new())
            },
            snapshot
                .get("accounts")
                .and_then(Value::as_array)
                .is_some_and(|accounts| {
                    accounts.iter().any(|account| {
                        account
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| id.starts_with("plaid:"))
                    })
                }),
            snapshot
                .get("accounts")
                .and_then(Value::as_array)
                .is_some_and(|accounts| {
                    accounts.iter().any(|account| {
                        account
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| id.starts_with("snaptrade:"))
                    })
                }),
            storage.data.revision,
            snapshot.clone(),
            benchmark_compatible && security_history_compatible,
        )
    };

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
                &plaid_progress,
            )
            .await;
        if integrations.plaid {
            match &result {
                Ok(data) if data.warnings.is_empty() => {
                    report_refresh_task(
                        app,
                        "plaid",
                        "Plaid",
                        timed_status("Complete", started),
                        "complete",
                    );
                }
                Ok(_) => report_refresh_task(
                    app,
                    "plaid",
                    "Plaid",
                    timed_status("Completed with notices", started),
                    "warning",
                ),
                Err(_) => {
                    report_refresh_task(
                        app,
                        "plaid",
                        "Plaid",
                        timed_status("Could not update", started),
                        "warning",
                    );
                }
            }
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
            match &result {
                Ok(data) if data.warnings.is_empty() => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Complete", started),
                    "complete",
                ),
                Ok(_) => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Completed with notices", started),
                    "warning",
                ),
                Err(_) => report_refresh_task(
                    app,
                    "snaptrade",
                    "SnapTrade",
                    timed_status("Could not update", started),
                    "warning",
                ),
            }
        }
        result
    };
    let benchmark_sync = async {
        if refresh_market_history {
            Some(
                tokio::time::timeout(
                    std::time::Duration::from_secs(12),
                    state.providers.sync_benchmark(),
                )
                .await,
            )
        } else {
            None
        }
    };
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
    let plaid = plaid.map_err(|error| error.to_string());
    let mut warnings = Vec::new();
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
    let plaid_updated = update_or_warn(&mut provider_data.plaid, plaid, "Plaid", &mut warnings);
    if !plaid_updated {
        next_plaid_cache = plaid_cache;
    }
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
        &mut warnings,
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
    if refresh_snaptrade_history
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.history_complete)
    {
        provider_data.snaptrade_history_refreshed_at = Some(now.to_rfc3339());
    }
    if backfill_snaptrade_activity
        && snaptrade_updated
        && provider_data
            .snaptrade
            .as_ref()
            .is_some_and(|data| data.activity_complete)
    {
        provider_data.snaptrade_activity_backfilled_at = Some(now.to_rfc3339());
    }
    if (!plaid_updated && provider_data.plaid.is_none() && had_plaid)
        || (!snaptrade_updated && provider_data.snaptrade.is_none() && had_snaptrade)
    {
        if let Ok(mut storage) = state.storage.lock() {
            storage.record_failed_sync(
                run_id,
                started_at,
                "provider_refresh_failed",
                SyncDiagnostics::new("provider", provider_diagnostics.clone(), warnings.len()),
            );
        }
        return Err(
            "Could not refresh safely without a provider cache; saved data is unchanged".into(),
        );
    }
    let mut benchmark_refreshed = false;
    if let Some(result) = &benchmark {
        match result {
            Ok(Err(error)) => warnings.push(format!("Benchmark history: {error}")),
            Err(_) => warnings.push("Benchmark history: request timed out".into()),
            _ => {}
        }
    }
    let benchmark = benchmark
        .and_then(Result::ok)
        .and_then(Result::ok)
        .filter(|history| history.as_array().is_some_and(|points| !points.is_empty()))
        .inspect(|_| benchmark_refreshed = true)
        .unwrap_or(previous_benchmark);
    let mut security_history_refreshed = false;
    if refresh_market_history {
        let empty_plaid = PlaidData::default();
        match tokio::time::timeout(
            std::time::Duration::from_secs(12),
            state
                .providers
                .sync_security_history(provider_data.plaid.as_ref().unwrap_or(&empty_plaid)),
        )
        .await
        {
            Ok(Ok(history)) => {
                provider_data.security_history = history;
                provider_data.security_history_adjustment = Some("split".into());
                security_history_refreshed = true;
            }
            Ok(Err(error)) => warnings.push(format!("Market history: {error}")),
            Err(_) => warnings.push("Market history: request timed out".into()),
        }
    }
    if benchmark_refreshed && security_history_refreshed {
        provider_data.market_history_refreshed_at = Some(now.to_rfc3339());
    }

    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    let sync_id = storage.stage_with_diagnostics(
        run_id,
        started_at,
        revision,
        next_plaid_cache,
        provider_data.clone(),
        warnings.clone(),
        SyncDiagnostics::new("provider", provider_diagnostics, warnings.len()),
    )?;

    Ok(ProviderSync {
        sync_id,
        balances_fresh: (!integrations.plaid || plaid_complete)
            && (!integrations.snaptrade || snaptrade_updated),
        refreshed_account_ids,
        previous_snapshot,
        plaid: provider_data.plaid.unwrap_or_default(),
        snaptrade: provider_data.snaptrade.unwrap_or_default(),
        benchmark_history: benchmark,
        security_history: provider_data.security_history,
    })
}
