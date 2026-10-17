//! Content-addressed JSON: the change log's storage for what each change carried.
//!
//! A value is stored as a tree of nodes, each saved once under the SHA-256 of its bytes, so a
//! refresh that changes ten of a thousand transactions stores those ten and a few small nodes
//! that point at the rest, as Git stores a commit's tree. A child small enough to inline sits
//! in its parent; a larger one is its own node. A long array is cut into chunks at elements
//! whose hash ends a chunk, so a transaction added at the top changes one chunk, not every
//! position after it. An array of byte values (a statement file's contents) is stored as one
//! base64 string. Nodes are compressed, checked against their hash when read, and never
//! changed or deleted except by erasure.
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

use super::{db_error, json_error};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS blobs (
       hash TEXT PRIMARY KEY CHECK (length(hash) = 64),
       data BLOB NOT NULL
     ) STRICT;
     CREATE TRIGGER IF NOT EXISTS blobs_no_update BEFORE UPDATE ON blobs
       BEGIN SELECT RAISE(ABORT, 'Stored blobs never change'); END;
     CREATE TRIGGER IF NOT EXISTS blobs_no_delete BEFORE DELETE ON blobs
       BEGIN SELECT RAISE(ABORT, 'Stored blobs never change'); END;";

/// A child whose JSON is at most this many bytes sits inside its parent.
const INLINE_BYTES: usize = 256;
/// Arrays longer than this are chunked.
const CHUNK_MIN: usize = 64;
/// A chunk ends after an element whose hash has its low four bits clear (16 on average), or
/// at this many elements.
const CHUNK_MAX: usize = 64;

/// The lowercase hex SHA-256 of `bytes`.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Stores `value` and returns its hash. Storing the same value again adds nothing.
pub(super) fn put(connection: &Connection, value: &Value) -> Result<String, String> {
    let node = node(connection, value)?;
    save(connection, &node)
}

/// The value stored under `hash`.
pub(super) fn get(connection: &Connection, hash: &str) -> Result<Value, String> {
    let node = load(connection, hash)?;
    decode(connection, node)
}

fn node(connection: &Connection, value: &Value) -> Result<Value, String> {
    Ok(match value {
        Value::Object(map) => {
            let pairs = map
                .iter()
                .map(|(key, value)| Ok(json!([key, child(connection, value)?])))
                .collect::<Result<Vec<_>, String>>()?;
            json!({ "o": pairs })
        }
        Value::Array(items) if items.len() > INLINE_BYTES && bytes(items).is_some() => {
            let bytes = bytes(items).unwrap_or_default();
            json!({ "b": STANDARD.encode(bytes) })
        }
        Value::Array(items) if items.len() > CHUNK_MIN => {
            let mut chunks = Vec::new();
            let mut chunk = Vec::new();
            for item in items {
                let child = child(connection, item)?;
                let ends = sha256(&serde_json::to_vec(&child).map_err(json_error)?).ends_with('0')
                    || chunk.len() + 1 == CHUNK_MAX;
                chunk.push(child);
                if ends {
                    chunks.push(save(
                        connection,
                        &json!({ "a": std::mem::take(&mut chunk) }),
                    )?);
                }
            }
            if !chunk.is_empty() {
                chunks.push(save(connection, &json!({ "a": chunk }))?);
            }
            json!({ "c": chunks })
        }
        Value::Array(items) => {
            let children = items
                .iter()
                .map(|item| child(connection, item))
                .collect::<Result<Vec<_>, String>>()?;
            json!({ "a": children })
        }
        leaf => json!({ "l": leaf }),
    })
}

/// A value inside its parent, or a reference to its own node.
fn child(connection: &Connection, value: &Value) -> Result<Value, String> {
    let small = match value {
        Value::Object(_) | Value::Array(_) => {
            serde_json::to_vec(value).map_err(json_error)?.len() <= INLINE_BYTES
        }
        Value::String(text) => text.len() <= INLINE_BYTES,
        _ => true,
    };
    if small {
        Ok(json!({ "v": value }))
    } else {
        Ok(json!({ "h": put(connection, value)? }))
    }
}

/// The bytes an array holds, when every element is an integer from 0 to 255.
fn bytes(items: &[Value]) -> Option<Vec<u8>> {
    items
        .iter()
        .map(|item| item.as_u64().and_then(|byte| u8::try_from(byte).ok()))
        .collect()
}

fn save(connection: &Connection, node: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(node).map_err(json_error)?;
    let hash = sha256(&bytes);
    let known = connection
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM blobs WHERE hash = ?1)")
        .and_then(|mut query| query.query_row([&hash], |row| row.get::<_, bool>(0)))
        .map_err(db_error)?;
    if !known {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder
            .write_all(&bytes)
            .and_then(|()| encoder.finish())
            .map_err(|error| format!("Could not compress the change log: {error}"))
            .and_then(|data| {
                connection
                    .prepare_cached("INSERT INTO blobs(hash, data) VALUES (?1, ?2)")
                    .and_then(|mut insert| insert.execute(params![hash, data]))
                    .map_err(db_error)
            })?;
    }
    Ok(hash)
}

fn load(connection: &Connection, hash: &str) -> Result<Value, String> {
    let data = connection
        .prepare_cached("SELECT data FROM blobs WHERE hash = ?1")
        .and_then(|mut query| {
            query
                .query_row([hash], |row| row.get::<_, Vec<u8>>(0))
                .optional()
        })
        .map_err(db_error)?
        .ok_or("The change log is missing a stored value")?;
    let mut bytes = Vec::new();
    ZlibDecoder::new(data.as_slice())
        .read_to_end(&mut bytes)
        .map_err(|_| "The change log holds an unreadable value")?;
    if sha256(&bytes) != hash {
        return Err("The change log holds a damaged value".into());
    }
    serde_json::from_slice(&bytes).map_err(json_error)
}

fn decode(connection: &Connection, node: Value) -> Result<Value, String> {
    let Value::Object(mut node) = node else {
        return Err("The change log holds an unknown value".into());
    };
    let resolve = |child: Value| -> Result<Value, String> {
        match child {
            Value::Object(mut child) => match (child.remove("v"), child.remove("h")) {
                (Some(value), None) => Ok(value),
                (None, Some(Value::String(hash))) => get(connection, &hash),
                _ => Err("The change log holds an unknown value".into()),
            },
            _ => Err("The change log holds an unknown value".into()),
        }
    };
    let list = |value: Option<Value>| match value {
        Some(Value::Array(items)) => Ok(items),
        _ => Err("The change log holds an unknown value".to_string()),
    };
    if let Some(pairs) = node.remove("o") {
        let mut map = Map::new();
        for pair in list(Some(pairs))? {
            let Value::Array(pair) = pair else {
                return Err("The change log holds an unknown value".into());
            };
            let [Value::String(key), child] = <[Value; 2]>::try_from(pair)
                .map_err(|_| "The change log holds an unknown value")?
            else {
                return Err("The change log holds an unknown value".into());
            };
            map.insert(key, resolve(child)?);
        }
        Ok(Value::Object(map))
    } else if let Some(children) = node.remove("a") {
        list(Some(children))?
            .into_iter()
            .map(resolve)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    } else if let Some(chunks) = node.remove("c") {
        let mut items = Vec::new();
        for chunk in list(Some(chunks))? {
            let Value::String(hash) = chunk else {
                return Err("The change log holds an unknown value".into());
            };
            items.extend(list(Some(get(connection, &hash)?))?);
        }
        Ok(Value::Array(items))
    } else if let Some(Value::String(encoded)) = node.remove("b") {
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "The change log holds an unreadable value")?;
        Ok(Value::Array(bytes.into_iter().map(Value::from).collect()))
    } else if let Some(leaf) = node.remove("l") {
        Ok(leaf)
    } else {
        Err("The change log holds an unknown value".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection
    }

    fn count(connection: &Connection) -> i64 {
        connection
            .query_row("SELECT count(*) FROM blobs", [], |row| row.get(0))
            .unwrap()
    }

    /// Synthetic transactions large enough to be their own nodes.
    fn transactions(ids: impl Iterator<Item = usize>) -> Value {
        ids.map(|id| {
            json!({
                "transaction_id": format!("synthetic-{id}"),
                "amount": id as f64 * 1.25,
                "name": format!("Example merchant {id} with a description long enough to stand alone as its own stored node"),
                "date": "2026-09-01",
            })
        })
        .collect()
    }

    #[test]
    fn values_come_back_exactly_as_stored() {
        let connection = connection();
        for value in [
            json!(null),
            json!("a string longer than the inline limit ".repeat(20)),
            json!({ "nested": { "list": [1, 2.5, "three", null, true], "empty": {} } }),
            json!({ "bytes": (0..2000).map(|byte| byte % 256).collect::<Vec<_>>() }),
            json!({ "notBytes": (0..2000).map(|value| value % 300).collect::<Vec<_>>() }),
            json!({ "transactions": transactions(0..500) }),
        ] {
            let hash = put(&connection, &value).unwrap();
            assert_eq!(get(&connection, &hash).unwrap(), value);
            assert_eq!(put(&connection, &value).unwrap(), hash);
        }
    }

    #[test]
    fn a_new_transaction_at_the_top_stores_little_more() {
        let connection = connection();
        put(
            &connection,
            &json!({ "transactions": transactions(1..600) }),
        )
        .unwrap();
        let before = count(&connection);
        let next = json!({ "transactions": transactions(0..600) });
        let hash = put(&connection, &next).unwrap();
        // The new transaction, the chunk it joined, the chunk list, and the root.
        assert!(
            count(&connection) - before <= 5,
            "{}",
            count(&connection) - before
        );
        assert_eq!(get(&connection, &hash).unwrap(), next);
    }

    #[test]
    fn a_damaged_value_is_refused_rather_than_read() {
        let connection = connection();
        let hash = put(&connection, &json!({ "transactions": transactions(0..3) })).unwrap();
        connection
            .execute_batch("DROP TRIGGER blobs_no_update")
            .unwrap();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(br#"{"l":"tampered"}"#).unwrap();
        connection
            .execute(
                "UPDATE blobs SET data = ?1 WHERE hash = ?2",
                params![encoder.finish().unwrap(), hash],
            )
            .unwrap();
        assert_eq!(
            get(&connection, &hash).unwrap_err(),
            "The change log holds a damaged value"
        );
    }

    #[test]
    fn stored_values_never_change() {
        let connection = connection();
        let hash = put(&connection, &json!("x".repeat(400))).unwrap();
        assert!(connection
            .execute("DELETE FROM blobs WHERE hash = ?1", [&hash])
            .is_err());
        assert!(connection
            .execute("UPDATE blobs SET data = x'00' WHERE hash = ?1", [&hash])
            .is_err());
    }
}
