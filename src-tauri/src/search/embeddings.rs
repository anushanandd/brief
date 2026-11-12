//! Sentence embeddings from Apple's Natural Language framework, computed on this Mac, and their
//! cache in `embeddings.sqlite3`, a disposable file keyed by each text's SHA-256 so a text is
//! embedded once. Nothing leaves the Mac. Where the framework has no model (or off macOS),
//! search keeps to words.
use std::{collections::BTreeMap, path::Path};

use rusqlite::{params, Connection};

const CACHE_ERROR: &str = "Search cache unavailable";
/// Texts sent to the framework per call.
const BATCH: usize = 64;

fn connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|_| CACHE_ERROR)?;
    crate::database::secure_file(path)?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS vectors (
               text_hash TEXT PRIMARY KEY CHECK (length(text_hash) = 64),
               vector BLOB NOT NULL
             ) STRICT;",
        )
        .map_err(|_| CACHE_ERROR)?;
    Ok(connection)
}

fn bytes(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Computes embeddings for texts; `None` when the framework can't.
pub trait Embedder {
    fn embed(&self, texts: &[String]) -> Option<Vec<Vec<f32>>>;
}

/// Each text's embedding, from the cache or else from `embedder`, which then fills the cache.
/// A text the embedder can't embed is left out.
pub fn embed_cached(
    path: &Path,
    embedder: &dyn Embedder,
    texts: &[(usize, String)],
) -> Result<Vec<(usize, Vec<f32>)>, String> {
    let connection = connection(path)?;
    let hashes = texts
        .iter()
        .map(|(_, text)| crate::database::sha256(text.as_bytes()))
        .collect::<Vec<_>>();
    let mut cached = BTreeMap::new();
    {
        let mut select = connection
            .prepare_cached("SELECT vector FROM vectors WHERE text_hash = ?1")
            .map_err(|_| CACHE_ERROR)?;
        for hash in &hashes {
            if let Ok(found) = select.query_row([hash], |row| row.get::<_, Vec<u8>>(0)) {
                cached.insert(hash.clone(), vector(&found));
            }
        }
    }
    let missing = texts
        .iter()
        .zip(&hashes)
        .filter(|(_, hash)| !cached.contains_key(*hash))
        .map(|((_, text), hash)| (text.clone(), hash.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect::<Vec<_>>();
    for batch in missing.chunks(BATCH) {
        let batch_texts = batch
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>();
        let Some(vectors) = embedder.embed(&batch_texts) else {
            break;
        };
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| CACHE_ERROR)?;
        for ((_, hash), embedded) in batch.iter().zip(vectors) {
            if embedded.is_empty() {
                continue;
            }
            transaction
                .execute(
                    "INSERT OR REPLACE INTO vectors(text_hash, vector) VALUES (?1, ?2)",
                    params![hash, bytes(&embedded)],
                )
                .map_err(|_| CACHE_ERROR)?;
            cached.insert(hash.clone(), embedded);
        }
        transaction.commit().map_err(|_| CACHE_ERROR)?;
    }
    Ok(texts
        .iter()
        .zip(&hashes)
        .filter_map(|((position, _), hash)| Some((*position, cached.get(hash)?.clone())))
        .collect())
}

/// The cache at `path` alone: it never calls the framework, so a view can read it under the
/// storage lock. A text not cached yet comes back empty, which callers read as unknown, and is
/// noted in `missing` for a `Warmer` to embed later.
pub struct CachedOnly {
    pub path: std::path::PathBuf,
    pub missing: std::cell::RefCell<std::collections::BTreeSet<String>>,
}

impl CachedOnly {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            missing: Default::default(),
        }
    }
}

impl Embedder for CachedOnly {
    fn embed(&self, texts: &[String]) -> Option<Vec<Vec<f32>>> {
        let found = if self.path.exists() {
            let connection = connection(&self.path).ok()?;
            let mut select = connection
                .prepare_cached("SELECT vector FROM vectors WHERE text_hash = ?1")
                .ok()?;
            texts
                .iter()
                .map(|text| {
                    let hash = crate::database::sha256(text.as_bytes());
                    select
                        .query_row([hash], |row| row.get::<_, Vec<u8>>(0))
                        .map(|found| vector(&found))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        } else {
            vec![Vec::new(); texts.len()]
        };
        let mut missing = self.missing.borrow_mut();
        for (text, embedded) in texts.iter().zip(&found) {
            if embedded.is_empty() {
                missing.insert(text.clone());
            }
        }
        Some(found)
    }
}

/// Embeds texts a view found missing (`CachedOnly`), off the storage lock and the view path:
/// one background job at a time, each text tried once per launch. `generation` rises when a
/// job finishes, so the next read derives its view again with the new vectors.
#[derive(Clone)]
pub struct Warmer {
    path: std::path::PathBuf,
    embedder: std::sync::Arc<dyn Embedder + Send + Sync>,
    state: std::sync::Arc<std::sync::Mutex<WarmState>>,
}

#[derive(Default)]
struct WarmState {
    running: bool,
    tried: std::collections::BTreeSet<String>,
    queued: std::collections::BTreeSet<String>,
    generation: u64,
}

impl Warmer {
    pub fn new(
        path: std::path::PathBuf,
        embedder: std::sync::Arc<dyn Embedder + Send + Sync>,
    ) -> Self {
        Self {
            path,
            embedder,
            state: Default::default(),
        }
    }

    /// Changes each time a job has added to the cache.
    pub fn generation(&self) -> u64 {
        self.state.lock().map_or(0, |state| state.generation)
    }

    /// Queues the texts not tried yet. Outside tests a background thread embeds them; tests
    /// call `warm` themselves, so results never depend on a thread's timing.
    pub fn request(&self, texts: std::collections::BTreeSet<String>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let fresh = texts
            .into_iter()
            .filter(|text| !state.tried.contains(text))
            .collect::<Vec<_>>();
        state.queued.extend(fresh);
        if cfg!(test) || state.running || state.queued.is_empty() {
            return;
        }
        state.running = true;
        drop(state);
        let warmer = self.clone();
        std::thread::spawn(move || warmer.drain());
    }

    /// Warms until nothing is queued, so texts queued during a run aren't left waiting for the
    /// next request; a run that panics ends its batch, not the warmer.
    pub(crate) fn drain(&self) {
        loop {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.warm()));
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if state.queued.is_empty() {
                state.running = false;
                return;
            }
        }
    }

    /// Embeds what is queued into the cache, then raises `generation`.
    pub fn warm(&self) {
        let queued = match self.state.lock() {
            Ok(mut state) => {
                let queued = std::mem::take(&mut state.queued);
                state.tried.extend(queued.iter().cloned());
                queued
            }
            Err(_) => return,
        };
        if queued.is_empty() {
            return;
        }
        let texts = queued.into_iter().enumerate().collect::<Vec<_>>();
        let added = embed_cached(&self.path, self.embedder.as_ref(), &texts)
            .is_ok_and(|found| !found.is_empty());
        if let (true, Ok(mut state)) = (added, self.state.lock()) {
            state.generation += 1;
        }
    }
}

/// Removes the cache, as erasure and clearing caches do.
pub fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Could not remove the search cache".into()),
    }
}

/// The Natural Language framework's English sentence embedding.
pub struct Apple;

#[cfg(target_os = "macos")]
impl Embedder for Apple {
    fn embed(&self, texts: &[String]) -> Option<Vec<Vec<f32>>> {
        use std::ffi::{c_char, CStr, CString};
        // The Swift bridge (native/EmbeddingBridge.swift): the result is null or a
        // NUL-terminated `strdup` string, owned by the caller until `brief_embed_free`.
        unsafe extern "C" {
            fn brief_embed(texts: *const c_char) -> *mut c_char;
            fn brief_embed_free(value: *mut c_char);
        }
        let input = CString::new(serde_json::to_string(texts).ok()?).ok()?;
        // SAFETY: the bridge only reads the NUL-terminated `input` during the call; it
        // outlives it.
        let output = unsafe { brief_embed(input.as_ptr()) };
        if output.is_null() {
            return None;
        }
        // SAFETY: a non-null result is a NUL-terminated `strdup` string, read before it is
        // freed.
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: freed once, by the bridge's own `free`, and never read again.
        unsafe { brief_embed_free(output) };
        serde_json::from_str(&text).ok()
    }
}

#[cfg(not(target_os = "macos"))]
impl Embedder for Apple {
    fn embed(&self, _texts: &[String]) -> Option<Vec<Vec<f32>>> {
        None
    }
}
