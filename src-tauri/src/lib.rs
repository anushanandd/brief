mod authentication;
mod background;
mod close;
mod commands;
mod database;
mod files;
mod finance_contract;
mod financial_engine;
mod foundation_model;
mod health;
mod insights;
mod market;
mod native_menu;
mod notify;
mod numeric;
mod providers;
mod refresh;
mod rules;
mod session;
mod settings;
mod startup_market;
mod statements;
mod storage;
mod summary;
mod transaction_policy;
mod window;

use std::{sync::Mutex, time::Duration as StdDuration};

use commands::{assistant, backups, finance, integrations, investments};
use providers::Providers;
use storage::Storage;
use tauri::Manager;

/// A market socket's owner: the request that started it, and how to cancel it.
type StreamOwner = Mutex<Option<(String, tokio::sync::watch::Sender<bool>)>>;

struct AppState {
    startup_market: startup_market::StartupMarket,
    storage: Mutex<Storage>,
    providers: Providers,
    refresh: tokio::sync::Mutex<()>,
    /// Needs already notified in this run, so each interrupts once.
    notified: Mutex<std::collections::HashSet<String>>,
    market_owner: StreamOwner,
    holding_owner: StreamOwner,
    holding_selection:
        Mutex<Option<tokio::sync::watch::Sender<providers::holding_market::ChartSelection>>>,
    /// The statement file chosen for import, awaiting its account.
    statement_draft: Mutex<Option<storage::StatementDraft>>,
}

impl AppState {
    /// The finance store, or an error once a panic has poisoned its lock.
    pub(crate) fn storage(&self) -> Result<std::sync::MutexGuard<'_, Storage>, String> {
        self.storage
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())
    }

    /// The statement awaiting import, or an error once a panic has poisoned its lock.
    fn statement_draft(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<storage::StatementDraft>>, String> {
        self.statement_draft
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())
    }

    /// Saves the closed sessions a summary computed and their events. A failed save only means
    /// they are computed again next time, so it never fails the summary.
    fn save_market_sessions(&self, closed: &summary::Closed) {
        if let Ok(mut storage) = self.storage() {
            let _ = storage.save_market_sessions(closed);
        }
    }

    /// Stops the portfolio stream and the Holdings chart, as erasing data or removing the
    /// Alpaca keys requires.
    fn stop_market_streams(&self) -> Result<(), String> {
        for owner in [&self.market_owner, &self.holding_owner] {
            market::cancel_stream(
                &mut *owner.lock().map_err(|_| "Market stream unavailable")?,
                None,
            );
        }
        *self
            .holding_selection
            .lock()
            .map_err(|_| "Chart selection unavailable")? = None;
        Ok(())
    }
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(native_menu::shortcut_menu)
        .on_menu_event(native_menu::handle_shortcut);

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            window::hide_window_buttons(
                &app.get_webview_window("main")
                    .ok_or("Main window unavailable")?,
            )?;
            let directory = app.path().app_data_dir()?;
            app.manage(AppState {
                startup_market: startup_market::StartupMarket::new(
                    directory.join("startup-market.sqlite3"),
                ),
                storage: Mutex::new(Storage::new(directory.clone())?),
                providers: Providers::new()
                    .map_err(|error| format!("failed to initialize providers: {error}"))?
                    .with_news_cache(&directory.join("news.sqlite3")),
                refresh: tokio::sync::Mutex::new(()),
                notified: Mutex::new(Default::default()),
                market_owner: Mutex::new(None),
                holding_owner: Mutex::new(None),
                holding_selection: Mutex::new(None),
                statement_draft: Mutex::new(None),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(StdDuration::from_secs(3)).await;
                let _ = window::reveal_main_window(handle);
            });
            background::start(app.handle().clone());
            close::start(app.handle().clone());
            notify::sync_tray(app.handle(), background::enabled(app.handle()));
            Ok(())
        })
        // With background refresh on, closing the window hides it so the daily refresh can
        // run; Quit still exits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && background::enabled(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            window::reveal_main_window,
            window::take_opened_view,
            market::get_saved_market,
            market::start_market_stream,
            market::stop_market_stream,
            market::start_holding_chart,
            market::stop_holding_chart,
            finance::get_finance_snapshot,
            finance::get_sync_runs,
            finance::get_data_health,
            finance::transaction_annotations,
            finance::save_finance_settings,
            finance::save_category_rules,
            finance::preview_statement_import,
            finance::import_statement,
            finance::get_statement_imports,
            finance::remove_statement_import,
            finance::export_finance_csv,
            refresh::refresh_finance_snapshot,
            refresh::save_account_link,
            backups::recover_finance_state,
            backups::export_finance_backup,
            backups::inspect_finance_backup,
            backups::restore_finance_backup,
            backups::erase_finance_state,
            backups::export_settings_file,
            backups::read_settings_file,
            integrations::get_integration_status,
            integrations::get_provider_connections,
            integrations::get_snaptrade_connections,
            integrations::forget_provider_connection,
            integrations::authenticate_sensitive_action,
            integrations::save_integration_credentials,
            integrations::remove_integration_credentials,
            integrations::begin_provider_link,
            integrations::poll_provider_link,
            integrations::cancel_provider_link,
            assistant::get_foundation_model_status,
            assistant::generate_foundation_explanation,
            assistant::cancel_foundation_explanation,
            assistant::ledger_query,
            investments::get_market_news,
            investments::get_portfolio_today,
            investments::get_portfolio_sectors,
            investments::get_investment_summary,
            investments::refresh_portfolio_news,
            investments::get_earnings_calendar,
            investments::get_calibration,
            investments::get_summary_runs,
            background::get_login_item,
            background::set_login_item,
            native_menu::set_command_menu,
            native_menu::copy_text,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Brief")
        .run(|app, event| {
            // Clicking the Dock icon brings back a window hidden by background refresh.
            #[cfg(target_os = "macos")]
            match &event {
                tauri::RunEvent::Reopen { .. } => window::bring_forward(app),
                tauri::RunEvent::Opened { urls } => window::open_links(app, urls),
                _ => {}
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
