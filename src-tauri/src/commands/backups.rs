//! Recovery, backups, erasure, and the display-preferences file.
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use super::require_sensitive_authorization;
use crate::{files, storage, AppState};

/// Display preferences are small; a larger file is not one Brief wrote.
const PREFERENCES_BYTES: usize = 256 * 1024;

#[tauri::command]
pub(crate) async fn recover_finance_state(
    restore: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state.storage()?.recover(restore)
}

#[tauri::command]
pub(crate) fn export_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    require_sensitive_authorization(&state)?;
    state.storage()?.export_backup(&PathBuf::from(path))
}

#[tauri::command]
pub(crate) fn inspect_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<storage::BackupInfo, String> {
    state.storage()?.inspect_backup(&PathBuf::from(path))
}

#[tauri::command]
pub(crate) async fn restore_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    require_sensitive_authorization(&state)?;
    let mut storage = state.storage()?;
    storage.import_backup(&PathBuf::from(path))?;
    storage.snapshot()
}

/// Clears the finance database, its retained copies, and every cache; Keychain credentials
/// stay until removed on their own.
#[tauri::command]
pub(crate) async fn erase_finance_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    require_sensitive_authorization(&state)?;
    state.stop_market_streams()?;
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "App data directory unavailable")?;
    state.startup_market.clear()?;
    state.providers.clear_disposable_data(&directory).await?;
    state.storage()?.erase()
}

#[tauri::command]
pub(crate) fn export_settings_file(path: String, contents: String) -> Result<(), String> {
    if contents.len() > PREFERENCES_BYTES {
        return Err("Preferences file exceeds 256 KB".into());
    }
    files::write_private(
        &PathBuf::from(path),
        contents.as_bytes(),
        "Preferences could not be saved",
    )
}

#[tauri::command]
pub(crate) fn read_settings_file(path: String) -> Result<String, String> {
    let contents =
        files::read_bounded(&PathBuf::from(path), PREFERENCES_BYTES).map_err(|failure| {
            match failure {
                files::ReadFailure::OutOfBounds => {
                    "Preferences file must be between 1 byte and 256 KB"
                }
                files::ReadFailure::Unreadable => "Preferences file cannot be read",
            }
        })?;
    String::from_utf8(contents).map_err(|_| "Preferences file cannot be read".into())
}
