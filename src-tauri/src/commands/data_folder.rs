//! The readable folder: its status, a write on request, and a zip of it. The background job
//! in `lib.rs` keeps it current after changes.
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::{
    data_folder::{self, Status},
    AppState,
};

/// What `sync_data_folder` last wrote, so the background job writes only after a change.
#[derive(Default)]
pub(crate) struct Written {
    /// The folder chosen, whether masked, the revision, and the local day it was written for.
    key: Option<(String, bool, u64, chrono::NaiveDate)>,
    status: Status,
}

fn home(app: &AppHandle) -> PathBuf {
    app.path().home_dir().unwrap_or_default()
}

/// Writes the folder when the chosen folder, the data, or the day changed since the last
/// write, and removes the copy left in a folder no longer chosen. With `force`, writes anyway.
pub(crate) fn sync_data_folder(app: &AppHandle, force: bool) -> Result<Status, String> {
    let state = app.state::<AppState>();
    let today = chrono::Local::now().date_naive();
    let (choice, graph) = {
        let storage = state.storage()?;
        if storage.is_recovering() {
            return current_status(&state);
        }
        let choice = storage.data.settings.data_folder.clone();
        let key = choice.as_ref().map(|folder| {
            (
                folder.path.clone(),
                folder.masked,
                storage.data.revision,
                today,
            )
        });
        let mut written = state.written()?;
        if !force && written.key == key {
            return Ok(written.status.clone());
        }
        let previous = written.key.take();
        if let Some((path, ..)) =
            previous.filter(|(path, ..)| choice.as_ref().is_none_or(|folder| &folder.path != path))
        {
            // A copy Brief wrote where the user no longer wants it goes.
            let _ = data_folder::remove(Path::new(&path));
        }
        written.key = key;
        let graph = match &choice {
            Some(_) => Some(storage.graph()?),
            None => None,
        };
        (choice, graph)
    };
    let status = match (choice, graph) {
        (Some(folder), Some(graph)) => {
            let chosen = Path::new(&folder.path);
            let now = chrono::Utc::now().to_rfc3339();
            let result = data_folder::write(chosen, &graph, folder.masked, &now);
            Status {
                folder: Some(data_folder::target(chosen).display().to_string()),
                written_at: result.is_ok().then_some(now),
                error: result.err(),
                synced: data_folder::synced(chosen, &home(app)),
            }
        }
        _ => Status::default(),
    };
    let mut written = state.written()?;
    if status.error.is_some() {
        // A failed write is tried again on the next pass.
        written.key = None;
    }
    written.status = status.clone();
    Ok(status)
}

fn current_status(state: &State<'_, AppState>) -> Result<Status, String> {
    Ok(state.written()?.status.clone())
}

#[tauri::command(async)]
pub(crate) fn get_data_folder_status(app: AppHandle) -> Result<Status, String> {
    sync_data_folder(&app, false)
}

/// Writes the folder now, as after it is turned on or moved.
#[tauri::command(async)]
pub(crate) fn write_data_folder(app: AppHandle) -> Result<Status, String> {
    sync_data_folder(&app, true)
}

/// Zips the folder Brief wrote to `path`, a new file.
#[tauri::command(async)]
pub(crate) fn export_data_folder(app: AppHandle, path: String) -> Result<(), String> {
    let status = sync_data_folder(&app, false)?;
    let folder = status.folder.ok_or("Turn on the data folder first")?;
    let destination = PathBuf::from(&path);
    if path.len() > 1_024 || !destination.is_absolute() || destination.exists() {
        return Err("Choose a new file".into());
    }
    // `ditto` ships with macOS and keeps the folder's structure in a standard zip.
    let output = std::process::Command::new("/usr/bin/ditto")
        .args(["-c", "-k", "--keepParent"])
        .arg(&folder)
        .arg(&destination)
        .output()
        .map_err(|_| "Could not zip the data folder")?;
    if !output.status.success() {
        return Err("Could not zip the data folder".into());
    }
    crate::database::secure_file(&destination)
}
