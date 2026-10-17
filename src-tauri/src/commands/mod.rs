//! Tauri commands, grouped by what they act on. A command checks its input and takes the locks
//! it needs; storage, providers, and insights do the work.
pub(crate) mod assistant;
pub(crate) mod backups;
pub(crate) mod data_folder;
pub(crate) mod documents;
pub(crate) mod finance;
pub(crate) mod integrations;
pub(crate) mod investments;
pub(crate) mod objects;
pub(crate) mod places;
pub(crate) mod research;

use tauri::State;

use crate::AppState;

/// Spends the single-use authorization a protected action requires.
fn require_sensitive_authorization(state: &State<'_, AppState>) -> Result<(), String> {
    if state.providers.take_credential_edit_authorization()? {
        Ok(())
    } else {
        Err("Authenticate before changing protected Brief data".into())
    }
}
