//! Keeps saved data current while Brief's window is closed: a once-a-day refresh with
//! Plaid's cached balances, run by this process on this Mac. There is no server.
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// How often the hidden app checks whether a refresh is due.
const CHECK_INTERVAL: StdDuration = StdDuration::from_secs(15 * 60);
/// A closed window refreshes at most this often; an open window refreshes hourly itself.
const REFRESH_AGE: i64 = 20;

/// Whether a snapshot saved at `updated_at` is old enough for a background refresh.
pub fn refresh_due(updated_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(updated_at)
        .ok()
        .is_none_or(|saved| now - saved.with_timezone(&Utc) >= Duration::hours(REFRESH_AGE))
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

pub fn enabled(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .storage()
        .is_ok_and(|storage| storage.data.settings.background_refresh)
}

/// Starts the scheduler. It does nothing unless the user turned on background refresh.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(CHECK_INTERVAL).await;
            if window_visible(&app) {
                continue;
            }
            let state = app.state::<AppState>();
            let (enabled, updated_at, recovering) = match state.storage() {
                Ok(storage) => (
                    storage.data.settings.background_refresh,
                    storage.data.snapshot.updated_at.clone(),
                    storage.is_recovering(),
                ),
                Err(_) => continue,
            };
            let configured = state
                .providers
                .integration_status()
                .map(|status| status.plaid || status.snaptrade)
                .unwrap_or(false);
            if !enabled || recovering || !configured || !refresh_due(&updated_at, Utc::now()) {
                continue;
            }
            if crate::refresh::refresh_finance_snapshot(app.clone(), state, Some(false))
                .await
                .is_ok()
            {
                // A hidden window rereads the saved snapshot so it is current when shown.
                let _ = app.emit("finance-snapshot-updated", ());
                crate::notify::deliver(&app);
            }
        }
    });
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LoginItem {
    Enabled,
    Disabled,
    RequiresApproval,
    Unavailable,
}

#[cfg(target_os = "macos")]
mod login {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    use super::LoginItem;

    pub fn status() -> LoginItem {
        let status = unsafe { SMAppService::mainAppService().status() };
        match status {
            SMAppServiceStatus::Enabled => LoginItem::Enabled,
            SMAppServiceStatus::RequiresApproval => LoginItem::RequiresApproval,
            SMAppServiceStatus::NotRegistered => LoginItem::Disabled,
            _ => LoginItem::Unavailable,
        }
    }

    pub fn set(enabled: bool) -> Result<LoginItem, String> {
        let service = unsafe { SMAppService::mainAppService() };
        let result = if enabled {
            unsafe { service.registerAndReturnError() }
        } else {
            unsafe { service.unregisterAndReturnError() }
        };
        result.map_err(|_| {
            if enabled {
                "macOS did not allow Brief to open at login. Check Login Items in System Settings."
            } else {
                "macOS did not remove Brief from Login Items."
            }
            .to_string()
        })?;
        Ok(status())
    }
}

#[tauri::command]
pub fn get_login_item() -> LoginItem {
    #[cfg(target_os = "macos")]
    return login::status();
    #[cfg(not(target_os = "macos"))]
    LoginItem::Unavailable
}

#[tauri::command]
pub fn set_login_item(enabled: bool) -> Result<LoginItem, String> {
    #[cfg(target_os = "macos")]
    return login::set(enabled);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enabled;
        Err("Opening at login is available only on macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_only_snapshots_older_than_twenty_hours() {
        let now = DateTime::parse_from_rfc3339("2026-09-16T12:00:00Z")
            .unwrap()
            .to_utc();
        assert!(!refresh_due("2026-09-16T00:00:00Z", now));
        assert!(refresh_due("2026-09-15T16:00:00Z", now));
        assert!(refresh_due("not a date", now));
    }
}
