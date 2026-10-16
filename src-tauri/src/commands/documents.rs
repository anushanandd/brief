//! The user's documents: importing files and folders, changing what Brief read, opening an
//! original, and adding a statement's history to an account.
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::{
    documents::{self, crypto, AccountMatch, Document, MAX_DOCUMENT_BYTES},
    files,
    storage::documents::DocumentChange,
    AppState,
};

/// A folder import reads at most this many files, so a wrong pick can't run away.
const MAX_FILES: usize = 2_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportSummary {
    /// Ids of the documents added, in the order read.
    added: Vec<String>,
    /// Files already in Brief, by name.
    already: Vec<String>,
    /// Files Brief couldn't keep, with why.
    failed: Vec<Failure>,
    snapshot: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Failure {
    name: String,
    reason: String,
}

/// Every file under `paths`, skipping hidden ones and anything that isn't a document.
fn collect(paths: &[String]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        let hidden = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if hidden || found.len() >= MAX_FILES {
            continue;
        }
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut entries = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>();
                entries.sort();
                pending.extend(entries.into_iter().rev());
            }
        } else if path.is_file() {
            found.push(path);
        }
    }
    found
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

/// Reads files and folders into documents. Reading runs off the storage lock and reports
/// progress; the sealed originals and what was read are saved together at the end.
#[tauri::command]
pub(crate) async fn import_documents(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ImportSummary, String> {
    let files = collect(&paths);
    let (known, statement_ids, accounts) = {
        let storage = state.storage()?;
        storage.ensure_writable()?;
        let accounts = storage
            .data
            .snapshot
            .accounts
            .iter()
            .map(|account| (account.id.clone(), account.mask.clone()))
            .collect::<Vec<_>>();
        (
            storage
                .data
                .documents
                .iter()
                .map(|document| document.id.clone())
                .collect::<std::collections::BTreeSet<_>>(),
            storage
                .data
                .statements
                .iter()
                .map(|file| file.id.clone())
                .collect::<std::collections::BTreeSet<_>>(),
            accounts,
        )
    };
    let progress = app.clone();
    let (read, already, failed, key) = tauri::async_runtime::spawn_blocking(move || {
        let key = crypto::keychain_key(true)?.ok_or("Could not create the document key")?;
        let matches = accounts
            .iter()
            .map(|(id, mask)| AccountMatch {
                id,
                mask: mask.as_deref(),
            })
            .collect::<Vec<_>>();
        let now = chrono::Utc::now().to_rfc3339();
        let (mut read, mut already, mut failed) = (Vec::new(), Vec::new(), Vec::new());
        let total = files.len();
        for (index, path) in files.iter().enumerate() {
            let _ = progress.emit(
                "documents-progress",
                json!({ "done": index, "total": total }),
            );
            let name = file_name(path);
            let bytes = match files::read_bounded(path, MAX_DOCUMENT_BYTES) {
                Ok(bytes) => bytes,
                Err(_) => {
                    failed.push(Failure {
                        name,
                        reason: "Empty, unreadable, or larger than 40 MB".into(),
                    });
                    continue;
                }
            };
            let id = crate::statements::content_id(&bytes);
            if known.contains(&id) || statement_ids.contains(&id) {
                already.push(name);
                continue;
            }
            match documents::ingest(path, &name, &bytes, &matches, &now)
                .and_then(|document| Ok((key.seal(&document.id, &bytes)?, document)))
            {
                Ok((sealed, document)) => read.push((document, sealed)),
                Err(reason) => failed.push(Failure { name, reason }),
            }
        }
        let _ = progress.emit(
            "documents-progress",
            json!({ "done": total, "total": total }),
        );
        Ok::<_, String>((read, already, failed, key))
    })
    .await
    .map_err(|_| "Reading the documents stopped unexpectedly")??;
    let added = read
        .iter()
        .map(|(document, _)| document.id.clone())
        .collect();
    let snapshot = {
        let _refresh = state.refresh.lock().await;
        state.storage()?.add_documents(read, &key)?
    };
    crate::document_summaries::start(app);
    Ok(ImportSummary {
        added,
        already,
        failed,
        snapshot,
    })
}

#[tauri::command]
pub(crate) async fn update_document(
    id: String,
    change: DocumentChange,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state.storage()?.update_document(&id, change)
}

#[tauri::command]
pub(crate) async fn delete_documents(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _refresh = state.refresh.lock().await;
    state.storage()?.delete_documents(&ids)
}

fn this_macs_key() -> Result<crypto::DocumentKey, String> {
    crypto::keychain_key(false)?.ok_or_else(|| "This Mac has no document key yet".into())
}

/// Opens a document's original in the Mac's own viewer, from a private copy.
#[tauri::command]
pub(crate) async fn open_document(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let key = tauri::async_runtime::spawn_blocking(this_macs_key)
        .await
        .map_err(|_| "Could not read the document key")??;
    let path = state.storage()?.open_document_copy(&id, &key)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|_| "Could not open the document".to_string())
}

/// A document's original, decrypted in memory for Brief's own viewer. Nothing is written to
/// disk.
#[tauri::command]
pub(crate) async fn document_file(
    id: String,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let key = tauri::async_runtime::spawn_blocking(this_macs_key)
        .await
        .map_err(|_| "Could not read the document key")??;
    let contents = state.storage()?.document_contents(&id, &key)?;
    Ok(tauri::ipc::Response::new(contents))
}

/// Reads a document again, as a newer Brief reads it, keeping every value the user typed.
#[tauri::command]
pub(crate) async fn reread_document(
    id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let key = tauri::async_runtime::spawn_blocking(this_macs_key)
        .await
        .map_err(|_| "Could not read the document key")??;
    let (document, contents, accounts) = {
        let storage = state.storage()?;
        let document = storage
            .data
            .documents
            .iter()
            .find(|document| document.id == id)
            .cloned()
            .ok_or("That document was removed")?;
        let contents = storage.document_contents(&id, &key)?;
        let accounts = storage
            .data
            .snapshot
            .accounts
            .iter()
            .map(|account| (account.id.clone(), account.mask.clone()))
            .collect::<Vec<_>>();
        (document, contents, accounts)
    };
    let read: Document = tauri::async_runtime::spawn_blocking(move || {
        // Vision and PDFKit read from a path, so the original is written to a private scratch
        // file that is removed at once.
        let scratch = std::env::temp_dir().join(format!("brief-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&scratch).map_err(|_| "Could not prepare the document")?;
        let path = scratch.join(&document.file_name);
        files::write_private(&path, &contents, "Could not prepare the document")?;
        let matches = accounts
            .iter()
            .map(|(id, mask)| AccountMatch {
                id,
                mask: mask.as_deref(),
            })
            .collect::<Vec<_>>();
        let read = documents::ingest(
            &path,
            &document.file_name,
            &contents,
            &matches,
            &document.imported_at,
        );
        let _ = std::fs::remove_dir_all(&scratch);
        read
    })
    .await
    .map_err(|_| "Reading the document stopped unexpectedly")??;
    let _refresh = state.refresh.lock().await;
    state.storage()?.replace_reading(read)
}

/// Adds the history in statement documents to one account.
#[tauri::command]
pub(crate) async fn add_document_history(
    ids: Vec<String>,
    account_id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let key = tauri::async_runtime::spawn_blocking(this_macs_key)
        .await
        .map_err(|_| "Could not read the document key")??;
    let _refresh = state.refresh.lock().await;
    state
        .storage()?
        .add_document_history(&ids, &account_id, &key)
}
