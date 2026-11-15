//! Web lookups through IPC. Every command that sends anything checks that research is on and
//! logs the query before it runs. Pages that come back are shown to the user and never acted on.
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{research::Entry, AppState};

/// Whether lookups can run, and what was looked up.
#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchState")]
pub(crate) struct ResearchState {
    enabled: bool,
    connected: bool,
    log: Vec<Entry>,
}

pub(crate) fn enabled(state: &State<'_, AppState>) -> Result<(), String> {
    if state.storage()?.settings().research {
        Ok(())
    } else {
        Err("Turn on research in Settings first".into())
    }
}

/// Logs an entry before it runs and tells the renderer, so the query is visible first.
pub(crate) fn begin(
    app: &AppHandle,
    state: &State<'_, AppState>,
    entry: &Entry,
) -> Result<(), String> {
    state.storage()?.begin_research(entry)?;
    let _ = app.emit("research-updated", ());
    Ok(())
}

pub(crate) fn finish(
    app: &AppHandle,
    state: &State<'_, AppState>,
    entry: &Entry,
) -> Result<(), String> {
    state.storage()?.finish_research(entry)?;
    let _ = app.emit("research-updated", ());
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_research(state: State<'_, AppState>) -> Result<ResearchState, String> {
    let connected = state.providers.integration_status()?.exa;
    let storage = state.storage()?;
    let (log, _) = storage.research()?;
    Ok(ResearchState {
        enabled: storage.settings().research,
        connected,
        log,
    })
}
