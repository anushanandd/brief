//! The committed finance view and the edits made to it: annotations, settings, imported
//! statements, and the activity export.
use std::{collections::BTreeMap, path::PathBuf};

use chrono::Utc;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::{
    background, database, files, health, notify, settings, statements, storage,
    storage::Annotation, AppState,
};

#[tauri::command]
pub(crate) fn get_finance_snapshot(state: State<'_, AppState>) -> Result<Value, String> {
    let mut storage = state.storage()?;
    let upgraded = storage.upgrade_projection().is_ok();
    let mut snapshot = storage.snapshot()?;
    if !upgraded {
        if let Some(warnings) = snapshot["syncWarnings"].as_array_mut() {
            warnings.push("Refresh to update calculations".into());
        }
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_sync_runs(state: State<'_, AppState>) -> Result<Vec<database::SyncRun>, String> {
    state.storage()?.sync_runs()
}

#[tauri::command]
pub(crate) async fn get_data_health(
    state: State<'_, AppState>,
) -> Result<health::HealthReport, String> {
    let (view, sync_runs, calibration) = {
        let storage = state.storage()?;
        // Rates are a check of their own; their absence never fails the report.
        (
            storage.read_view()?,
            storage.sync_runs()?,
            storage.calibration().ok(),
        )
    };
    let summary = state
        .providers
        .summary_health(&view.snapshot, calibration)
        .await;
    Ok(health::report_with(
        &view.snapshot,
        &sync_runs,
        view.insights.as_ref(),
        &summary,
        Utc::now(),
    ))
}

// Edits rebuild the whole view; as async commands they run off the main thread, so the
// window keeps painting meanwhile.
#[tauri::command]
pub(crate) async fn transaction_annotations(
    changes: BTreeMap<String, Annotation>,
    importing: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let mut storage = state.storage()?;
    let saved = storage.save_annotations(changes, importing)?;
    Ok(serde_json::json!({
        "annotations": saved.annotations,
        "previous": saved.previous,
        "snapshot": storage.snapshot()?,
    }))
}

/// Undoes an annotation save by putting back the annotations it returned as `previous`.
#[tauri::command]
pub(crate) async fn restore_transaction_annotations(
    previous: BTreeMap<String, Annotation>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let mut storage = state.storage()?;
    let annotations = storage.restore_annotations(previous)?;
    Ok(serde_json::json!({ "annotations": annotations, "snapshot": storage.snapshot()? }))
}

#[tauri::command]
pub(crate) fn save_finance_settings(
    settings: settings::FinanceSettings,
    importing: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let saved = state.storage()?.save_settings(settings, importing)?;
    // The menu bar item follows background refresh, which this save may have changed.
    notify::sync_tray(&app, background::enabled(&app));
    // Sync, account names, and benefit visibility all change the Brief calendar.
    crate::calendar::sync_in_background(app);
    Ok(saved)
}

/// Async so reading and parsing the file never runs on the main thread.
#[tauri::command]
pub(crate) async fn preview_statement_import(
    path: String,
    state: State<'_, AppState>,
) -> Result<storage::StatementPreview, String> {
    let path = PathBuf::from(path);
    let contents =
        files::read_bounded(&path, statements::MAX_FILE_BYTES).map_err(
            |failure| match failure {
                files::ReadFailure::OutOfBounds => {
                    "Statement files must be between 1 byte and 5 MB"
                }
                files::ReadFailure::Unreadable => "This statement file can't be read",
            },
        )?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.chars().take(255).collect::<String>())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Statement".into());
    let (preview, draft) = state.storage()?.preview_statement(file_name, contents)?;
    *state.statement_draft()? = Some(draft);
    Ok(preview)
}

#[tauri::command]
pub(crate) async fn import_statement(
    draft_id: String,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    // Waits for a running refresh, whose commit would otherwise replace this import.
    let _refresh = state.refresh.lock().await;
    let draft = state
        .statement_draft()?
        .clone()
        .filter(|draft| draft.id == draft_id)
        .ok_or("Choose the statement again")?;
    let snapshot = state.storage()?.import_statement(draft, &account_id)?;
    *state.statement_draft()? = None;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_statement_imports(
    state: State<'_, AppState>,
) -> Result<Vec<storage::StatementSummary>, String> {
    Ok(state.storage()?.statement_summaries())
}

#[tauri::command]
pub(crate) async fn remove_statement_import(
    id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state.storage()?.remove_statement(&id)
}

#[tauri::command]
pub(crate) async fn confirm_statement_row(
    file_id: String,
    row_id: String,
    confirmed: bool,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state
        .storage()?
        .confirm_statement_row(&file_id, &row_id, confirmed)
}

/// Asks where to save an export the renderer wrote (activity or lots as CSV, upcoming dates
/// as a calendar), then writes it as a new private file. `false` when the picker was cancelled.
#[tauri::command]
pub(crate) async fn export_finance_csv(
    app: AppHandle,
    contents: String,
    filename: String,
) -> Result<bool, String> {
    if contents.len() > 50 * 1024 * 1024 {
        return Err("Export exceeds 50 MB".into());
    }
    let extension = validate_export_filename(&filename)?;
    let path = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let (title, label) = match extension {
            "ics" => ("Export calendar", "Calendar"),
            _ => ("Export CSV", "CSV"),
        };
        app.dialog()
            .file()
            .set_title(title)
            .set_file_name(filename)
            .add_filter(label, &[extension])
            .blocking_save_file()
    })
    .await
    .map_err(|_| "File picker interrupted")?;
    let Some(path) = path else { return Ok(false) };
    let path = path
        .into_path()
        .map_err(|_| "The selected destination is not a local file")?;
    files::write_private(&path, contents.as_bytes(), "Export could not be saved")?;
    Ok(true)
}

/// The extension of a safe export name: a plain `.csv` or `.ics` file name with no path.
fn validate_export_filename(filename: &str) -> Result<&'static str, String> {
    let extension = ["csv", "ics"]
        .into_iter()
        .find(|extension| filename.ends_with(&format!(".{extension}")));
    match extension {
        Some(extension)
            if filename.len() <= 240
                && !filename.starts_with('.')
                && !filename
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':')) =>
        {
            Ok(extension)
        }
        _ => Err("Invalid export filename".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_export_rejects_invalid_names_before_opening() {
        for name in [
            "../other.csv",
            "folder/file.csv",
            "bad\nname.csv",
            "not-a-csv.txt",
        ] {
            assert!(validate_export_filename(name).is_err());
        }
        assert_eq!(validate_export_filename("activity.csv"), Ok("csv"));
        assert_eq!(validate_export_filename("upcoming.ics"), Ok("ics"));
    }
}
