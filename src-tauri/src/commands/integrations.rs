//! Provider credentials, connections, and the links that add them.
use tauri::State;

use super::require_sensitive_authorization;
use crate::{
    authentication,
    providers::{self, Integration, IntegrationStatus, LinkSession, LinkStatus},
    AppState,
};

/// Holds off refreshes while credentials change; a running refresh makes the change wait.
fn no_refresh_running<'a>(
    state: &'a State<'_, AppState>,
) -> Result<tokio::sync::MutexGuard<'a, ()>, String> {
    state
        .refresh
        .try_lock()
        .map_err(|_| "Wait for the current account refresh before changing credentials".into())
}

#[tauri::command]
pub(crate) fn get_integration_status(
    state: State<'_, AppState>,
) -> Result<IntegrationStatus, String> {
    state.providers.integration_status()
}

#[tauri::command]
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
