//! Apple Intelligence answers and the read-only ledger queries they cite.
use serde_json::Value;
use tauri::State;

use crate::{foundation_model, AppState};

#[tauri::command]
pub(crate) fn get_foundation_model_status() -> foundation_model::FoundationModelStatus {
    foundation_model::status()
}

#[tauri::command]
pub(crate) async fn generate_foundation_explanation(
    question: String,
    request_id: String,
    started: tauri::ipc::Channel<()>,
    state: State<'_, AppState>,
) -> Result<foundation_model::Answer, String> {
    // The model sees only what read-only ledger tools return for the saved snapshot.
    let view = state.storage()?.read_view()?;
    let today = chrono::Local::now().date_naive();
    let context = format!(
        "Today is {today}. Saved financial data was last refreshed at {}. Amounts are USD.",
        view.snapshot.updated_at
    );
    let tools = foundation_model::ToolHost::new(view, today);
    foundation_model::generate(question, context, request_id, started, tools).await
}

/// Runs one read-only ledger query over the saved snapshot: the same figures an answer cited,
/// recomputed for today, so a pinned query stays current without the model.
#[tauri::command]
pub(crate) fn ledger_query(
    name: String,
    arguments: Value,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    if name.len() > 64 || arguments.to_string().len() > 2_000 || !arguments.is_object() {
        return Err("Invalid ledger query".into());
    }
    let view = state.storage()?.read_view()?;
    foundation_model::ToolHost::new(view, chrono::Local::now().date_naive())
        .query(&name, &arguments)
}

#[tauri::command]
pub(crate) fn cancel_foundation_explanation(request_id: String) {
    foundation_model::cancel(request_id);
}
