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
        unsafe extern "C" {
            fn brief_embed(texts: *const c_char) -> *mut c_char;
            fn brief_embed_free(value: *mut c_char);
        }
        let input = CString::new(serde_json::to_string(texts).ok()?).ok()?;
        let output = unsafe { brief_embed(input.as_ptr()) };
        if output.is_null() {
            return None;
        }
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
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
