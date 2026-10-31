//! Provider credentials, connections, and the links that add them.
use tauri::State;

use super::require_sensitive_authorization;
use crate::{
    authentication,
    providers::{self, wallet, Integration, IntegrationStatus, LinkSession, LinkStatus},
    AppState,
};

/// Reads Apple Wallet and saves what's new, or clears its records once it is turned off. With
/// `only_if_changed`, as when polled, nothing happens unless Wallet has written since the last
/// read, or while a refresh runs, since the refresh reads Wallet itself. Returns the view when
/// anything was saved.
#[tauri::command]
pub(crate) async fn sync_wallet(
    only_if_changed: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    let polled = only_if_changed.unwrap_or(false);
    let _refresh = if polled {
        match state.refresh.try_lock() {
            Ok(guard) => guard,
            Err(_) => return Ok(None),
        }
    } else {
        state.refresh.lock().await
    };
    let (enabled, saved) = {
        let storage = state.storage()?;
        (
            storage.data.settings.apple_wallet,
            storage.data.provider_data.wallet.clone(),
        )
    };
    if !enabled {
        return if saved.is_some() {
            state.storage()?.save_wallet(None).map(Some)
        } else {
            Ok(None)
        };
    }
    let read_at = saved.as_ref().map(|wallet| wallet.read_at.clone());
    let changed = tauri::async_runtime::spawn_blocking(move || {
        if polled && !wallet::changed_since(read_at.as_deref()) {
            return Ok(None);
        }
        wallet::read(chrono::Utc::now()).map(Some)
    })
    .await
    .map_err(|_| "Could not read Wallet".to_string())??;
    let Some(read) = changed else {
        return Ok(None);
    };
    let merged = wallet::merged(saved.as_ref(), read);
    // Wallet rewrites its file for many reasons; a poll saves only new records.
    if polled && saved.is_some_and(|saved| saved.same_records(&merged)) {
        return Ok(None);
    }
    state.storage()?.save_wallet(Some(merged)).map(Some)
}

/// Takes a Wallet charge the provider never reported into history, or back out of it.
#[tauri::command]
pub(crate) async fn confirm_wallet_row(
    id: String,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let _refresh = state.refresh.lock().await;
    state.storage()?.confirm_wallet_row(&id, confirmed)
}

/// Holds off refreshes while credentials change; a running refresh makes the change wait.
fn no_refresh_running<'a>(
    state: &'a State<'_, AppState>,
) -> Result<tokio::sync::MutexGuard<'a, ()>, String> {
    state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh before changing credentials".into())
}

#[tauri::command(async)]
pub(crate) fn get_integration_status(
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    state.providers.integration_status()
}

#[tauri::command(async)]
pub(crate) fn get_provider_connections(
    state: State<'_, AppState>,
) -> Result<Vec<providers::ProviderConnection>, String> {
    let (cache, snaptrade) = {
        let storage = state.storage()?;
        (
            storage.data.plaid_cache.clone(),
            storage
                .data
                .provider_data
                .snaptrade
                .as_ref()
                .map(|data| data.connections.clone())
                .unwrap_or_default(),
        )
    };
    state.providers.connections(&cache, &snaptrade)
}

#[tauri::command]
pub(crate) async fn get_snaptrade_connections(
    state: State<'_, AppState>,
) -> Result<Vec<providers::ProviderConnection>, String> {
    state.providers.snaptrade_connections_live().await
}

#[tauri::command]
pub(crate) async fn forget_provider_connection(
    item_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _refresh = state.refresh.lock().await;
    require_sensitive_authorization(&state)?;
    state.providers.forget_connection(&item_id)?;
    state.storage()?.cancel_pending_refresh();
    Ok(())
}

/// Tests and saves credentials. Replacing saved ones needs authentication, which a failed
/// test gives back so the user can correct a typo without authenticating again.
#[tauri::command]
pub(crate) async fn save_integration_credentials(
    provider: Integration,
    client_id: String,
    secret: Option<String>,
    consumer_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let _refresh = no_refresh_running(&state)?;
    let replacing = state.providers.integration_status()?.connected(provider);
    if replacing && !state.providers.take_credential_edit_authorization()? {
        return Err("Authenticate before replacing saved credentials".into());
    }
    if let Err(error) = state
        .providers
        .save_credentials(provider, client_id, secret, consumer_key)
        .await
    {
        if replacing {
            state.providers.authorize_credential_edit()?;
        }
        return Err(error);
    }
    state.storage()?.cancel_pending_refresh();
    state.providers.integration_status()
}

#[tauri::command]
pub(crate) async fn remove_integration_credentials(
    provider: Integration,
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    let _refresh = no_refresh_running(&state)?;
    require_sensitive_authorization(&state)?;
    if provider == Integration::Alpaca {
        state.stop_market_streams()?;
    }
    state.providers.remove_credentials(provider)?;
    state.storage()?.cancel_pending_refresh();
    state.providers.integration_status()
}

#[tauri::command]
pub(crate) async fn authenticate_sensitive_action(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.providers.take_credential_edit_authorization()?;
    authentication::authenticate_sensitive_action().await?;
    state.providers.authorize_credential_edit()
}

#[tauri::command]
pub(crate) async fn begin_provider_link(
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
pub(crate) async fn poll_provider_link(
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
pub(crate) fn cancel_provider_link(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.providers.finish_link(&session_id)
}

/// Saved bank statements, newest first, without their contents.
#[tauri::command(async)]
pub(crate) fn get_bank_statements(
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::BankStatement>, String> {
    state.storage()?.bank_statements()
}

/// Checks every connection with statements now and saves new ones.
#[tauri::command]
pub(crate) async fn sync_bank_statements(
    state: State<'_, AppState>,
) -> Result<crate::bank_statements::StatementSync, String> {
    crate::bank_statements::sync(&state, chrono::Utc::now(), true).await
}

/// Opens one saved statement in the Mac's PDF viewer, from a private copy.
#[tauri::command(async)]
pub(crate) fn open_bank_statement(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = state.storage()?.open_copy(&id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|_| "Could not open the statement".to_string())
}
