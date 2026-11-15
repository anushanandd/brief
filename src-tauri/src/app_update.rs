//! Updates from GitHub Releases. The updater reads `latest.json` from the newest release,
//! checks the update's signature against the public key in `tauri.conf.json`, replaces the app,
//! and relaunches it. Only the request for that file leaves the Mac; it carries no data.
//! Debug builds never update: they don't run from an app bundle.

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use ts_rs::TS;

/// A newer Brief than this one, as its release describes it.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppUpdate {
    pub version: String,
    pub notes: Option<String>,
}

async fn newer(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let updater = app.updater().map_err(|error| error.to_string())?;
    updater.check().await.map_err(|error| error.to_string())
}

/// The update the last check offered, so installing it installs what was shown.
static OFFERED: Mutex<Option<tauri_plugin_updater::Update>> = Mutex::new(None);

fn offered() -> std::sync::MutexGuard<'static, Option<tauri_plugin_updater::Update>> {
    OFFERED.lock().unwrap_or_else(PoisonError::into_inner)
}

#[tauri::command]
pub(crate) async fn check_app_update(app: AppHandle) -> Result<Option<AppUpdate>, String> {
    let update = newer(&app).await?;
    let shown = update.as_ref().map(|update| AppUpdate {
        version: update.version.clone(),
        notes: update.body.clone(),
    });
    *offered() = update;
    Ok(shown)
}

/// Installs the update the last check offered, or the newest release when none was, then
/// relaunches the way Quit exits.
#[tauri::command]
pub(crate) async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let held = offered().clone();
    let update = match held {
        Some(update) => update,
        None => newer(&app).await?.ok_or("Brief is already up to date.")?,
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    app.restart()
}
