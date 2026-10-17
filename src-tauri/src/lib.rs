mod authentication;
mod background;
mod bank_statements;
mod calendar;
mod close;
mod commands;
mod contacts;
mod data_folder;
mod database;
mod document_summaries;
mod documents;
mod files;
mod finance_contract;
mod financial_engine;
mod foundation_model;
mod health;
mod insights;
mod inventory;
mod market;
mod merchants;
mod native_menu;
mod notify;
mod numeric;
mod objects;
mod places;
mod providers;
mod quality;
mod refresh;
mod search;
mod session;
mod settings;
mod startup_market;
mod statements;
mod storage;
mod summary;
#[cfg(test)]
mod synthetic;
mod transaction_policy;
mod window;

use std::{sync::Mutex, time::Duration as StdDuration};

use commands::{
    assistant, backups, documents as document_commands, finance, integrations, investments,
    objects as object_commands, places as place_commands,
};
use providers::Providers;
use storage::Storage;
use tauri::{Emitter, Manager};

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
    /// Held while Brief finds places, so only one search runs.
    places: tokio::sync::Mutex<()>,
    /// The search index over the current graph, rebuilt when the graph changes.
    search: Mutex<Option<search::Index>>,
    /// What the readable folder last wrote.
    written: Mutex<commands::data_folder::Written>,
}

impl AppState {
    /// The finance store, or an error once a panic has poisoned its lock.
    pub(crate) fn storage(&self) -> Result<std::sync::MutexGuard<'_, Storage>, String> {
        self.storage
            .lock()
            .map_err(|_| "The local data store is unavailable".to_string())
    }

    fn written(&self) -> Result<std::sync::MutexGuard<'_, commands::data_folder::Written>, String> {
        self.written
            .lock()
            .map_err(|_| "The data folder is unavailable".to_string())
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

/// How long after launch the change log is replayed, so launch's own reads go first.
const LOG_CHECK_DELAY: StdDuration = StdDuration::from_secs(60);

/// Replays the change log once per launch, through its own connection, and keeps what it
/// found for Diagnostics and Data health.
fn check_log_in_background(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(LOG_CHECK_DELAY);
        let state = app.state::<AppState>();
        let Some(path) = state
            .storage()
            .ok()
            .filter(|storage| !storage.is_recovering())
            .map(|storage| storage.database_path().to_owned())
        else {
            return;
        };
        let check = match database::Database::open_read_only(&path) {
            Ok(database) => storage::change::check(&database, chrono::Utc::now()),
            Err(error) => storage::change::LogCheck {
                checked_at: chrono::Utc::now().to_rfc3339(),
                outcome: storage::change::Outcome::Failed,
                replayed: 0,
                detail: Some(error),
            },
        };
        if let Ok(mut storage) = state.storage() {
            storage.log_check = Some(check);
        };
    });
}

/// How often the readable folder checks for changes to write.
const DATA_FOLDER_INTERVAL: StdDuration = StdDuration::from_secs(30);

/// Keeps the readable folder current: after a change, or a new day, it is written again in
/// the background, never in a commit's way. Off until the user chooses a folder.
fn write_data_folder_in_background(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(DATA_FOLDER_INTERVAL);
        let _ = commands::data_folder::sync_data_folder(&app, false);
    });
}

/// The system-wide key that summons the command palette: ⌘⇧B.
fn palette_shortcut() -> tauri_plugin_global_shortcut::Shortcut {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyB)
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    // ⌘⇧B from any app brings Brief forward with the command palette open.
                    if event.state() == ShortcutState::Pressed && *shortcut == palette_shortcut() {
                        window::bring_forward(app);
                        let _ = app.emit("open-palette", ());
                    }
                })
                .build(),
        );

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
                places: tokio::sync::Mutex::new(()),
                search: Mutex::new(None),
                written: Mutex::default(),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(StdDuration::from_secs(3)).await;
                let _ = window::reveal_main_window(handle);
            });
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                // Another app may hold the key; Brief still runs without it.
                let _ = app.global_shortcut().register(palette_shortcut());
            }
            background::start(app.handle().clone());
            document_summaries::start(app.handle().clone());
            calendar::start(app.handle().clone());
            close::start(app.handle().clone());
            check_log_in_background(app.handle().clone());
            write_data_folder_in_background(app.handle().clone());
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
            finance::get_spending_period,
            finance::get_reports,
            finance::get_sync_runs,
            finance::get_change_log,
            object_commands::search_objects,
            object_commands::get_object,
            object_commands::get_stored_files,
            object_commands::get_stored_rows,
            commands::data_folder::get_data_folder_status,
            commands::data_folder::write_data_folder,
            commands::data_folder::export_data_folder,
            finance::get_data_health,
            finance::transaction_annotations,
            finance::restore_transaction_annotations,
            finance::save_finance_settings,
            finance::preview_statement_import,
            finance::import_statement,
            finance::get_statement_imports,
            finance::remove_statement_import,
            finance::confirm_statement_row,
            document_commands::import_documents,
            document_commands::update_document,
            document_commands::delete_documents,
            document_commands::open_document,
            document_commands::reread_document,
            document_commands::document_file,
            document_commands::add_document_history,
            finance::export_finance_csv,
            place_commands::find_places,
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
            integrations::get_bank_statements,
            integrations::sync_bank_statements,
            integrations::open_bank_statement,
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
            contacts::contacts_access,
            contacts::request_contacts_access,
            contacts::search_contacts,
            contacts::contact_photos,
            contacts::match_contacts,
            calendar::calendar_status,
            calendar::request_calendar_access,
            calendar::calendar_sources,
            calendar::create_brief_calendar,
            calendar::remove_brief_events,
            calendar::calendar_events,
            calendar::calendar_list,
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
