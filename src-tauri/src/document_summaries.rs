//! Summaries of the user's documents, written in the background by Apple Intelligence on this
//! Mac, one document at a time. A summary is kept only when every figure it states is printed
//! in the document and it names no account or identification number. A document whose summary
//! failed isn't tried again until Brief restarts.
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use tauri::{AppHandle, Emitter, Manager};

use crate::{
    documents::{crypto, read, Document},
    files, AppState,
};

/// The on-device model reads about 4,096 tokens at once, shared with its instructions.
const MAX_TEXT: usize = 6_000;
const MAX_SUMMARY: usize = 1_500;

static RUNNING: AtomicBool = AtomicBool::new(false);
static TRIED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Summarizes the documents that have none, unless a run is already doing so.
pub fn start(app: AppHandle) {
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        run(&app).await;
        RUNNING.store(false, Ordering::Release);
    });
}

async fn run(app: &AppHandle) {
    if crate::foundation_model::status().state != "available" {
        return;
    }
    let Ok(Ok(Some(key))) =
        tauri::async_runtime::spawn_blocking(|| crypto::keychain_key(false)).await
    else {
        return;
    };
    let state = app.state::<AppState>();
    loop {
        let next = {
            let Ok(storage) = state.storage() else {
                return;
            };
            let tried = TRIED.lock().map(|tried| tried.clone()).unwrap_or_default();
            storage
                .data
                .documents
                .iter()
                .find(|document| {
                    document.summary.is_none()
                        && document.unreadable.is_none()
                        && !tried.contains(&document.id)
                })
                .cloned()
        };
        let Some(document) = next else {
            return;
        };
        if let Ok(mut tried) = TRIED.lock() {
            tried.insert(document.id.clone());
        }
        let contents = match state
            .storage()
            .and_then(|storage| storage.document_contents(&document.id, &key))
        {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        let Ok(Ok(text)) =
            tauri::async_runtime::spawn_blocking(move || text_of(&document, &contents)).await
        else {
            continue;
        };
        let Some((id, text)) = text else {
            continue;
        };
        let Ok(summary) = crate::foundation_model::summarize(text.clone()).await else {
            continue;
        };
        if !acceptable(&summary, &text) {
            continue;
        }
        let saved = {
            let _refresh = state.refresh.lock().await;
            state
                .storage()
                .and_then(|mut storage| storage.set_document_summary(&id, summary))
        };
        if saved.is_ok() {
            let _ = app.emit("finance-snapshot-updated", ());
        }
    }
}

/// The document's text, at most `MAX_TEXT` characters: an export's own, or what its text layer
/// or Vision reads.
fn text_of(document: &Document, contents: &[u8]) -> Result<Option<(String, String)>, String> {
    let text = if document.media_type.starts_with("text/") {
        String::from_utf8_lossy(contents).into_owned()
    } else if document.media_type.contains("spreadsheet") {
        return Ok(None);
    } else {
        // Vision and PDFKit read from a path, so the original goes to a private scratch file
        // that is removed at once.
        let scratch = std::env::temp_dir().join(format!("brief-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&scratch).map_err(|_| "Could not prepare the document")?;
        let path = scratch.join(&document.file_name);
        let reading = files::write_private(&path, contents, "Could not prepare the document")
            .and_then(|()| {
                let quick = read::read_file(&path, false)?;
                if quick.is_empty() {
                    read::read_file(&path, true)
                } else {
                    Ok(quick)
                }
            });
        let _ = std::fs::remove_dir_all(&scratch);
        reading?.text()
    };
    let text = text.chars().take(MAX_TEXT).collect::<String>();
    Ok((text.trim().len() >= 40).then(|| (document.id.clone(), text)))
}

/// A summary that says something, states only figures the document prints, and carries no
/// run of six or more digits, which would be an account or identification number.
fn acceptable(summary: &str, text: &str) -> bool {
    let digits = summary
        .split(|character: char| !character.is_ascii_digit())
        .map(str::len)
        .max()
        .unwrap_or(0);
    !summary.is_empty()
        && summary.len() <= MAX_SUMMARY
        && digits < 6
        && crate::foundation_model::figures_not_in(summary, text).is_empty()
}

#[cfg(test)]
mod tests {
    use super::acceptable;

    #[test]
    fn a_summary_states_only_what_the_document_prints() {
        let text = "Example Bank statement, January 2026. Ending balance $2,382.47.";
        assert!(acceptable(
            "A January 2026 statement from Example Bank ending at $2,382.47.",
            text
        ));
        // A figure the document doesn't print, and an account number, are both refused.
        assert!(!acceptable("It ends at $2,400.00.", text));
        assert!(!acceptable("Account 000123459967 ends at $2,382.47.", text));
        assert!(!acceptable("", text));
    }
}
