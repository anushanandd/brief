//! Recovery, backups, erasure, and the display-preferences file.
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use super::require_sensitive_authorization;
use crate::{documents::crypto, files, storage, AppState};

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

/// A passphrase lets a backup's documents open on another Mac; a short one would be guessed.
fn document_passphrase(passphrase: Option<String>) -> Result<Option<String>, String> {
    match passphrase.filter(|passphrase| !passphrase.is_empty()) {
        Some(passphrase) if passphrase.chars().count() < 8 => {
            Err("Use a passphrase of at least 8 characters".into())
        }
        passphrase => Ok(passphrase),
    }
}

#[tauri::command]
pub(crate) async fn export_finance_backup(
    path: String,
    passphrase: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let passphrase = document_passphrase(passphrase)?;
    require_sensitive_authorization(&state)?;
    let key = tauri::async_runtime::spawn_blocking(|| crypto::keychain_key(false))
        .await
        .map_err(|_| "Could not read the document key")??;
    state
        .storage()?
        .export_backup(&PathBuf::from(path), key.as_ref(), passphrase.as_deref())
}

#[tauri::command]
pub(crate) async fn inspect_finance_backup(
    path: String,
    state: State<'_, AppState>,
) -> Result<storage::BackupInfo, String> {
    let key = tauri::async_runtime::spawn_blocking(|| crypto::keychain_key(false))
        .await
        .map_err(|_| "Could not read the document key")??;
    state
        .storage()?
        .inspect_backup(&PathBuf::from(path), key.as_ref())
}

#[tauri::command]
pub(crate) async fn restore_finance_backup(
    path: String,
    passphrase: Option<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    require_sensitive_authorization(&state)?;
    // A Mac without a document key gets one, so documents from the backup can be sealed again.
    let key = tauri::async_runtime::spawn_blocking(|| crypto::keychain_key(true))
        .await
        .map_err(|_| "Could not read the document key")??;
    let mut storage = state.storage()?;
    storage.import_backup(
        &PathBuf::from(path),
        key.as_ref(),
        passphrase
            .as_deref()
            .filter(|passphrase| !passphrase.is_empty()),
    )?;
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

#[tauri::command(async)]
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

#[tauri::command(async)]
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
