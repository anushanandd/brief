//! Updates from GitHub Releases. The updater reads `latest.json` from the newest release,
//! checks the update's signature against the public key in `tauri.conf.json`, replaces the app,
//! and relaunches it. Only the request for that file leaves the Mac; it carries no data.
//! Debug builds never update: they don't run from an app bundle.

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

#[tauri::command]
pub(crate) async fn check_app_update(app: AppHandle) -> Result<Option<AppUpdate>, String> {
    Ok(newer(&app).await?.map(|update| AppUpdate {
        version: update.version,
        notes: update.body,
    }))
}

/// Installs the newest release, then relaunches the way Quit exits.
#[tauri::command]
pub(crate) async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let update = newer(&app).await?.ok_or("Brief is already up to date.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    app.restart()
}
