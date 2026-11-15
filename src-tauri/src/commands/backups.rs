//! Recovery, backups, and erasure.
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use super::require_sensitive_authorization;
use crate::{documents::crypto, storage, AppState};

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
    // The readable folder is a copy of this data, so it goes with it.
    let folder = state.storage()?.data.settings.data_folder.clone();
    if let Some(folder) = folder {
        crate::data_folder::remove(std::path::Path::new(&folder.path))?;
    }
    state.storage()?.erase()
}
