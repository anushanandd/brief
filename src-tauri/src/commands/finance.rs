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

#[tauri::command(async)]
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

/// One Spending period in full: the snapshot carries only each scope's current periods.
#[tauri::command(async)]
pub(crate) fn get_spending_period(
    request: crate::insights::spending::PeriodRequest,
    state: State<'_, AppState>,
) -> Result<Option<crate::insights::spending::PeriodDetail>, String> {
    if request.scope.len() > 128 || request.offset > 10_000 {
        return Err("Invalid spending period".into());
    }
    let view = state.storage()?.read_view()?;
    Ok(crate::insights::spending_period(
        &view,
        &request,
        &crate::insights::calendar::LocalZone,
    ))
}

/// Money's and Investments' reports over a window and accounts; see `insights::reports`.
#[tauri::command(async)]
pub(crate) fn get_reports(
    request: crate::insights::reports::ReportRequest,
    state: State<'_, AppState>,
) -> Result<Option<crate::insights::reports::Reports>, String> {
    if request.accounts.as_ref().is_some_and(|accounts| {
        accounts.len() > 256 || accounts.iter().any(|account| account.len() > 128)
    }) {
        return Err("Invalid report request".into());
    }
    let view = state.storage()?.read_view()?;
    let zone = crate::insights::calendar::LocalZone;
    // Reports run through today, which may be later than the last refresh.
    let today = view.insights.as_ref().map_or_else(
        || crate::insights::calendar::Zone::date(&zone, Utc::now()),
        |insights| insights.today,
    );
    Ok(crate::insights::reports::reports(
        &view, &request, today, &zone,
    ))
}

#[tauri::command(async)]
pub(crate) fn get_sync_runs(state: State<'_, AppState>) -> Result<Vec<database::SyncRun>, String> {
    state.storage()?.sync_runs()
}

#[tauri::command]
pub(crate) async fn get_data_health(
    state: State<'_, AppState>,
) -> Result<health::HealthReport, String> {
    let (view, sync_runs, calibration, log) = {
        let storage = state.storage()?;
        // Rates are a check of their own; their absence never fails the report.
        (
            storage.read_view()?,
            storage.sync_runs()?,
            storage.calibration().ok(),
            storage.log_check.clone(),
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
        log.as_ref(),
        Utc::now(),
    ))
}

/// What the change log holds and what the latest replay of it found.
#[derive(serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct ChangeLog {
    summary: database::LogSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    check: Option<crate::storage::change::LogCheck>,
}

#[tauri::command(async)]
pub(crate) fn get_change_log(state: State<'_, AppState>) -> Result<ChangeLog, String> {
    let storage = state.storage()?;
    Ok(ChangeLog {
        summary: storage.log_summary()?,
        check: storage.log_check.clone(),
    })
}

/// What an annotation edit returns: every annotation, those it changed as they were (so it can
/// be undone; none after an undo), and the view.
#[derive(serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct AnnotationsSaved {
    annotations: BTreeMap<String, Annotation>,
    previous: BTreeMap<String, Annotation>,
    #[ts(type = "import('./FinanceSnapshot').FinanceSnapshot")]
    snapshot: Value,
}

// Edits rebuild the whole view; as async commands they run off the main thread, so the
// window keeps painting meanwhile.
#[tauri::command]
pub(crate) async fn transaction_annotations(
    changes: BTreeMap<String, Annotation>,
    importing: bool,
    state: State<'_, AppState>,
) -> Result<AnnotationsSaved, String> {
    let mut storage = state.storage()?;
    let saved = storage.save_annotations(changes, importing)?;
    Ok(AnnotationsSaved {
        annotations: saved.annotations,
        previous: saved.previous,
        snapshot: storage.snapshot()?,
    })
}

/// Undoes an annotation save by putting back the annotations it returned as `previous`.
#[tauri::command]
pub(crate) async fn restore_transaction_annotations(
    previous: BTreeMap<String, Annotation>,
    state: State<'_, AppState>,
) -> Result<AnnotationsSaved, String> {
    let mut storage = state.storage()?;
    let annotations = storage.restore_annotations(previous)?;
    Ok(AnnotationsSaved {
        annotations,
        previous: BTreeMap::new(),
        snapshot: storage.snapshot()?,
    })
}

#[tauri::command(async)]
pub(crate) fn save_finance_settings(
    settings: settings::FinanceSettings,
    importing: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let saved = state.storage()?.save_settings(settings, importing)?;
    // The menu bar item follows background refresh, which this save may have changed. The save
    // runs off the main thread; the tray is built back on it.
    let tray = app.clone();
    let _ = app.run_on_main_thread(move || notify::sync_tray(&tray, background::enabled(&tray)));
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

#[tauri::command(async)]
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
