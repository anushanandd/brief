mod authentication;
mod database;
mod finance_contract;
mod financial_engine;
mod foundation_model;
mod health;
mod market;
mod providers;
mod refresh;
mod startup_market;
mod storage;
mod transaction_policy;
mod workspace;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Duration, Utc};
use database::SyncDiagnostics;
#[cfg(test)]
use market::{cancel_stream, market_update_interval, reuse_holding_chart};
use market::{
    get_saved_market, start_holding_chart, start_market_stream, stop_holding_chart,
    stop_market_stream,
};
use providers::{
    IntegrationStatus, LinkSession, LinkStatus, PlaidData, PlaidRefreshProgress, ProviderSync,
    Providers,
};
#[cfg(test)]
use refresh::{history_cache_is_fresh, update_or_warn};
use refresh::{refresh_finance_snapshot, save_account_link};
use serde_json::Value;
use storage::{Annotation, Storage};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(target_os = "macos")]
use objc2_app_kit::{NSWindow, NSWindowButton};
#[cfg(target_os = "macos")]
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    Runtime,
};

#[cfg(target_os = "macos")]
fn hide_window_buttons(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let native = window.ns_window()? as usize;
    window.run_on_main_thread(move || {
        // SAFETY: Tauri owns this NSWindow for the life of the main app window.
        let native = unsafe { &*(native as *const NSWindow) };
        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = native.standardWindowButton(kind) {
                button.setHidden(true);
            }
        }
    })
}

struct AppState {
    startup_market: startup_market::StartupMarket,
    storage: Mutex<Storage>,
    providers: Providers,
    refresh: tokio::sync::Mutex<()>,
    market_owner: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>,
    holding_owner: Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>,
    holding_selection:
        Mutex<Option<tokio::sync::watch::Sender<providers::holding_market::ChartSelection>>>,
}

#[tauri::command]
fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?;
    if storage.upgrade_projection().is_err() {
        let mut snapshot = storage.snapshot()?;
        let warnings = snapshot["syncWarnings"].as_array_mut();
        if let Some(warnings) = warnings {
            warnings.push("Cached calculations need a refresh to update.".into());
        } else {
            snapshot["syncWarnings"] =
                serde_json::json!(["Cached calculations need a refresh to update."]);
        }
        return Ok(snapshot);
    }
    storage.snapshot()
}

#[tauri::command]
fn get_sync_runs(state: State<'_, AppState>) -> Result<Vec<database::SyncRun>, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())?
        .sync_runs()
}

#[tauri::command]
async fn recover_finance_state(restore: bool, state: State<'_, AppState>) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .recover(restore)
}

#[tauri::command]
fn export_finance_backup(path: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .export_backup(&PathBuf::from(path))
}

#[tauri::command]
fn inspect_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<storage::BackupInfo, String> {
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .inspect_backup(&PathBuf::from(path))
}

#[tauri::command]
async fn restore_finance_backup(path: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let workspace = storage.import_backup(&PathBuf::from(path))?;
    Ok(serde_json::json!({
        "snapshot": storage.snapshot()?,
        "workspace": workspace,
    }))
}

#[tauri::command]
fn get_data_health(state: State<'_, AppState>) -> Result<health::HealthReport, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let snapshot = serde_json::from_value(storage.snapshot()?)
        .map_err(|error| format!("Invalid committed snapshot: {error}"))?;
    Ok(health::report(&snapshot, &storage.sync_runs()?, Utc::now()))
}

#[tauri::command]
fn transaction_annotations(
    changes: BTreeMap<String, Annotation>,
    importing: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?;
    let annotations = storage.save_annotations(changes, importing)?;
    Ok(serde_json::json!({ "annotations": annotations, "snapshot": storage.snapshot()? }))
}

#[tauri::command]
async fn export_finance_csv(
    app: AppHandle,
    contents: String,
    filename: String,
) -> Result<bool, String> {
    if contents.len() > 50 * 1024 * 1024 {
        return Err("Export exceeds 50 MB".into());
    }
    workspace::validate_csv_filename(&filename)?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .set_title("Export account activity CSV")
            .set_file_name(filename)
            .add_filter("CSV", &["csv"])
            .blocking_save_file()
    })
    .await
    .map_err(|_| "File picker interrupted")?;
    let Some(path) = path else { return Ok(false) };
    let path = path
        .into_path()
        .map_err(|_| "The selected destination is not a local file")?;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&path)
        .map_err(|_| "Choose a new filename; existing files are never overwritten")?;
    if file
        .write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = std::fs::remove_file(path);
        return Err("Export could not be saved".into());
    }
    Ok(true)
}

#[tauri::command]
fn get_integration_status(state: State<'_, AppState>) -> Result<IntegrationStatus, String> {
    state.providers.integration_status()
}

#[tauri::command]
fn get_provider_connections(
    state: State<'_, AppState>,
) -> Result<Vec<providers::ProviderConnection>, String> {
    let cache = state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .data
        .plaid_cache
        .clone();
    state.providers.connections(&cache)
}

#[tauri::command]
async fn get_plaid_recurring_report(
    state: State<'_, AppState>,
) -> Result<providers::PlaidRecurringReport, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh to finish")?;
    state.providers.plaid_recurring_report().await
}

#[tauri::command]
async fn forget_provider_connection(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _refresh = state.refresh.lock().await;
    state.providers.forget_connection(&item_id)?;
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .cancel_pending_refresh();
    Ok(())
}

#[tauri::command]
async fn save_integration_credentials(
    provider: String,
    client_id: String,
    secret: Option<String>,
    consumer_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let _refresh = state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh before changing credentials")?;
    let status = state.providers.integration_status()?;
    let requires_authentication = match provider.as_str() {
        "plaid" => status.plaid,
        "snaptrade" => status.snaptrade,
        "alpaca" => status.alpaca,
        "alphavantage" => status.alpha_vantage,
        _ => return Err("Unknown provider".into()),
    };
    if requires_authentication && !state.providers.take_credential_edit_authorization()? {
        return Err("Authenticate before replacing saved credentials".into());
    }

    if let Err(error) = state
        .providers
        .save_credentials(&provider, client_id, secret, consumer_key)
        .await
    {
        if requires_authentication {
            state.providers.authorize_credential_edit()?;
        }
        return Err(error);
    }
    state
        .storage
        .lock()
        .map_err(|_| "The local data store is unavailable")?
        .cancel_pending_refresh();
    state.providers.integration_status()
}

#[tauri::command]
async fn authenticate_sensitive_action(state: State<'_, AppState>) -> Result<(), String> {
    state.providers.lock_credentials()?;
    authentication::authenticate_sensitive_action().await?;
    state.providers.unlock_credentials()?;
    state.providers.authorize_credential_edit()
}

#[tauri::command]
fn get_foundation_model_status() -> foundation_model::FoundationModelStatus {
    foundation_model::status()
}

#[tauri::command]
async fn generate_foundation_explanation(
    question: String,
    evidence: String,
    request_id: String,
    started: tauri::ipc::Channel<()>,
) -> Result<String, String> {
    foundation_model::generate(question, evidence, request_id, started).await
}

#[tauri::command]
fn cancel_foundation_explanation(request_id: String) {
    foundation_model::cancel(request_id);
}

#[tauri::command]
async fn begin_provider_link(
    provider: String,
    item_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<LinkSession, String> {
    state
        .providers
        .begin_link(&provider, item_id.as_deref())
        .await
}

#[tauri::command]
async fn poll_provider_link(
    provider: String,
    session_id: String,
    browser_completed: Option<bool>,
    state: State<'_, AppState>,
) -> Result<LinkStatus, String> {
    state
        .providers
        .poll_link(&provider, &session_id, browser_completed.unwrap_or(false))
        .await
}

#[tauri::command]
fn cancel_provider_link(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.providers.finish_link(&session_id)
}

#[tauri::command]
fn reveal_main_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Window unavailable")?;
    window.show().map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_market_news(
    symbols: Vec<String>,
    refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<providers::news_cache::NewsResult, String> {
    state
        .providers
        .market_news(symbols, refresh.unwrap_or(false))
        .await
}

#[tauri::command]
async fn get_earnings_calendar(
    symbols: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<providers::EarningsEvent>, String> {
    state.providers.earnings_calendar(symbols).await
}

#[cfg(target_os = "macos")]
fn shortcut_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    let settings = MenuItem::with_id(app, "open-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    if let Some(app_menu) = menu.items()?.first().and_then(|item| item.as_submenu()) {
        app_menu.insert(&settings, 1)?;
        for child in app_menu.items()? {
            if let Some(predefined) = child.as_predefined_menuitem() {
                let label = predefined.text()?.replace('&', "");
                if label.starts_with("Hide ") && label != "Hide Others" {
                    app_menu.remove(&child)?;
                }
            }
        }
        let hide_values = MenuItem::with_id(
            app,
            "toggle-sensitive-values",
            "Hide or Show Values",
            true,
            Some("CmdOrCtrl+H"),
        )?;
        app_menu.insert(&hide_values, 2)?;
    }

    for item in menu.items()? {
        let Some(submenu) = item.as_submenu() else {
            continue;
        };
        let title = submenu.text()?;

        if title == "File" {
            menu.remove(&item)?;
            continue;
        }

        if title == "Edit" || title == "Window" {
            for child in submenu.items()? {
                let Some(predefined) = child.as_predefined_menuitem() else {
                    continue;
                };
                let label = predefined.text()?.replace('&', "");
                if (title == "Edit" && label == "Select All")
                    || (title == "Window" && label != "Zoom")
                {
                    submenu.remove(&child)?;
                }
            }
        }
    }

    let previous = MenuItem::with_id(app, "graph-previous", "Previous Graph", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "graph-next", "Next Graph", true, None::<&str>)?;
    let previous_window = MenuItem::with_id(
        app,
        "graph-window-previous",
        "Previous Date View",
        true,
        Some("Cmd+Ctrl+ArrowLeft"),
    )?;
    let next_window = MenuItem::with_id(
        app,
        "graph-window-next",
        "Next Date View",
        true,
        Some("Cmd+Ctrl+ArrowRight"),
    )?;
    let week = MenuItem::with_id(app, "graph-week", "1 Week", true, None::<&str>)?;
    let month = MenuItem::with_id(app, "graph-month", "1 Month", true, None::<&str>)?;
    let quarter = MenuItem::with_id(app, "graph-quarter", "3 Months", true, None::<&str>)?;
    let year = MenuItem::with_id(app, "graph-year", "1 Year", true, None::<&str>)?;
    let all = MenuItem::with_id(app, "graph-all", "All Time", true, None::<&str>)?;
    let graph = Submenu::with_items(
        app,
        "Graph",
        true,
        &[
            &previous,
            &next,
            &previous_window,
            &next_window,
            &week,
            &month,
            &quarter,
            &year,
            &all,
        ],
    )?;
    menu.append(&graph)?;

    Ok(menu)
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(target_os = "macos")]
    let builder = builder.menu(shortcut_menu).on_menu_event(|app, event| {
        let shortcut = event.id().as_ref();
        if shortcut == "open-settings" {
            let _ = app.emit("open-settings", ());
        } else if shortcut == "toggle-sensitive-values" {
            let _ = app.emit("toggle-sensitive-values", ());
        } else if shortcut.starts_with("graph-") {
            let _ = app.emit("graph-shortcut", shortcut.to_string());
        }
    });

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            hide_window_buttons(
                &app.get_webview_window("main")
                    .ok_or("Main window unavailable")?,
            )?;
            app.manage(AppState {
                startup_market: startup_market::StartupMarket::new(
                    app.path().app_data_dir()?.join("startup-market.sqlite3"),
                ),
                storage: Mutex::new(Storage::new(app.path().app_data_dir()?)?),
                providers: Providers::new()
                    .and_then(|providers| {
                        providers.with_news_cache(
                            &app.path()
                                .app_data_dir()
                                .map_err(|_| "App data directory unavailable")?
                                .join("news.sqlite3"),
                        )
                    })
                    .map_err(|error| format!("failed to initialize providers: {error}"))?,
                refresh: tokio::sync::Mutex::new(()),
                market_owner: Mutex::new(None),
                holding_owner: Mutex::new(None),
                holding_selection: Mutex::new(None),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(StdDuration::from_secs(3)).await;
                let _ = reveal_main_window(handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            reveal_main_window,
            get_saved_market,
            get_finance_snapshot,
            get_sync_runs,
            recover_finance_state,
            export_finance_backup,
            inspect_finance_backup,
            restore_finance_backup,
            get_data_health,
            transaction_annotations,
            export_finance_csv,
            get_integration_status,
            get_provider_connections,
            get_plaid_recurring_report,
            forget_provider_connection,
            get_foundation_model_status,
            generate_foundation_explanation,
            cancel_foundation_explanation,
            authenticate_sensitive_action,
            save_integration_credentials,
            begin_provider_link,
            poll_provider_link,
            cancel_provider_link,
            start_market_stream,
            stop_market_stream,
            start_holding_chart,
            stop_holding_chart,
            get_market_news,
            get_earnings_calendar,
            refresh_finance_snapshot,
            save_account_link,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brief");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_cleanup_cannot_cancel_a_new_market_request() {
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let mut stream = Some(("new".into(), sender));
        cancel_stream(&mut stream, Some("old"));
        assert!(!*receiver.borrow());
        assert!(stream.is_some());
        cancel_stream(&mut stream, Some("new"));
        assert!(*receiver.borrow());
        assert!(stream.is_none());
    }

    #[test]
    fn changing_chart_selection_retains_socket_owner_and_old_cleanup_cannot_stop_it() {
        use providers::holding_market::ChartSelection;
        let (cancel, cancelled) = tokio::sync::watch::channel(false);
        let mut owner = Some(("first".into(), cancel));
        let (chart, selection) = tokio::sync::watch::channel(ChartSelection {
            symbol: "TEST".into(),
            range: 86400,
            request_id: "first".into(),
            warm_symbols: Vec::new(),
        });
        let next = ChartSelection {
            symbol: "OTHER".into(),
            range: 604800,
            request_id: "second".into(),
            warm_symbols: Vec::new(),
        };
        assert!(reuse_holding_chart(&mut owner, Some(&chart), &next));
        assert_eq!(selection.borrow().symbol, "OTHER");
        assert_eq!(selection.borrow().range, 604800);
        cancel_stream(&mut owner, Some("first"));
        assert!(!*cancelled.borrow());
        assert_eq!(owner.as_ref().unwrap().0, "second");
        cancel_stream(&mut owner, Some("second"));
        assert!(*cancelled.borrow());
        assert!(!reuse_holding_chart(&mut owner, Some(&chart), &next));

        let (cancel, _) = tokio::sync::watch::channel(false);
        owner = Some(("portfolio".into(), cancel));
        assert!(!reuse_holding_chart(&mut owner, Some(&chart), &next));
        assert_eq!(owner.unwrap().0, "portfolio");
    }

    #[tokio::test]
    async fn socket_handoff_waits_for_release_and_cancelled_waiters_never_acquire() {
        let socket = tokio::sync::Mutex::new(());
        let active = socket.lock().await;
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let next = providers::market_stream::socket_lease(&socket, receiver.clone());
        tokio::pin!(next);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(next.as_mut(), &mut context).is_pending());
        cancel.send(true).unwrap();
        assert!(next.await.is_none());
        assert!(providers::market_stream::socket_lease(&socket, receiver)
            .await
            .is_none());
        drop(active);
        let (_cancel, receiver) = tokio::sync::watch::channel(false);
        assert!(providers::market_stream::socket_lease(&socket, receiver)
            .await
            .is_some());
    }

    #[test]
    fn market_update_interval_defaults_and_stays_bounded() {
        assert_eq!(market_update_interval(None), StdDuration::from_secs(10));
        assert_eq!(market_update_interval(Some(500)), StdDuration::from_secs(1));
        assert_eq!(
            market_update_interval(Some(30_000)),
            StdDuration::from_secs(30)
        );
        assert_eq!(
            market_update_interval(Some(90_000)),
            StdDuration::from_secs(60)
        );
    }

    #[test]
    fn credential_edit_authorization_is_single_use() {
        let providers = Providers::new().expect("provider state");
        assert!(!providers.take_credential_edit_authorization().unwrap());
        providers.authorize_credential_edit().unwrap();
        assert!(providers.take_credential_edit_authorization().unwrap());
        assert!(!providers.take_credential_edit_authorization().unwrap());
    }

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
    fn historical_data_expires_after_twelve_hours() {
        let now = "2026-09-04T16:00:00Z".parse::<DateTime<Utc>>().unwrap();
        assert!(history_cache_is_fresh(Some("2026-09-04T05:00:01Z"), now));
        assert!(!history_cache_is_fresh(Some("2026-09-04T04:00:00Z"), now));
        assert!(!history_cache_is_fresh(Some("invalid"), now));
    }
}
