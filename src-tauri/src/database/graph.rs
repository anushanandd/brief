//! The object graph's typed tables: objects, their links, and facts, each a row with real
//! columns and indexes, so search, the raw view, and the readable folder read them without
//! parsing the snapshot. They are derived data, like market sessions: rewritten whole when the
//! derived view gives a different graph, never part of the change log, and cleared by erasure.
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;

use super::{db_error, json_error, Database};
use crate::objects::{Fact, Graph, Kind, Link, Object};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS graph_objects (
       id TEXT PRIMARY KEY,
       kind TEXT NOT NULL,
       title TEXT NOT NULL,
       subtitle TEXT,
       date TEXT,
       amount REAL,
       text TEXT NOT NULL,
       record_json TEXT NOT NULL CHECK (json_valid(record_json))
     ) STRICT;
     CREATE INDEX IF NOT EXISTS graph_objects_kind ON graph_objects(kind, date);
     CREATE TABLE IF NOT EXISTS graph_links (
       source TEXT NOT NULL,
       relation TEXT NOT NULL,
       target TEXT NOT NULL,
       PRIMARY KEY (source, relation, target)
     ) STRICT, WITHOUT ROWID;
     CREATE INDEX IF NOT EXISTS graph_links_target ON graph_links(target);
     CREATE TABLE IF NOT EXISTS graph_facts (
       subject TEXT NOT NULL,
       predicate TEXT NOT NULL,
       value_json TEXT NOT NULL CHECK (json_valid(value_json)),
       valid_from TEXT,
       valid_to TEXT,
       sources_json TEXT NOT NULL CHECK (json_valid(sources_json)),
       grade TEXT,
       tier TEXT NOT NULL
     ) STRICT;
     CREATE INDEX IF NOT EXISTS graph_facts_subject ON graph_facts(subject, predicate);
     CREATE TABLE IF NOT EXISTS graph_meta (
       id INTEGER PRIMARY KEY CHECK (id = 1),
       hash TEXT NOT NULL CHECK (length(hash) = 64)
     ) STRICT;";

/// A JSON string column of a unit enum, such as `"account"`, as the enum.
fn name<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).map_err(json_error)
}

fn text_of<T: serde::Serialize>(value: &T) -> Result<String, String> {
    match serde_json::to_value(value).map_err(json_error)? {
        serde_json::Value::String(text) => Ok(text),
        other => Ok(other.to_string()),
    }
}

const OBJECT_COLUMNS: &str = "id, kind, title, subtitle, date, amount, text, record_json";

type ObjectRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<f64>,
    String,
    String,
);

fn object_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn object(connection: &Connection, id: &str) -> Result<Option<Object>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {OBJECT_COLUMNS} FROM graph_objects WHERE id = ?1"
        ))
        .map_err(db_error)?;
    let rows = statement.query_map([id], object_row).map_err(db_error)?;
    let objects = rows
        .map(|row| {
            let (id, kind, title, subtitle, date, amount, text, record) = row.map_err(db_error)?;
            Ok(Object {
                id,
                kind: Kind::from_name(&kind).ok_or("Unknown object kind")?,
                title,
                subtitle,
                date,
                amount,
                text,
                record: serde_json::from_str(&record).map_err(json_error)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(objects.into_iter().next())
}

impl Database {
    /// The hash of the graph the tables hold, if any.
    pub fn graph_hash(&self) -> Result<Option<String>, String> {
        self.connection
            .query_row("SELECT hash FROM graph_meta WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(db_error)
    }

    /// Replaces the graph the tables hold with `graph`, whose hash is `hash`.
    pub fn write_graph(&self, graph: &Graph, hash: &str) -> Result<(), String> {
        let transaction = self.connection.unchecked_transaction().map_err(db_error)?;
        transaction
            .execute_batch(
                "DELETE FROM graph_objects; DELETE FROM graph_links; DELETE FROM graph_facts;",
            )
            .map_err(db_error)?;
        {
            let mut insert = transaction
                .prepare(&format!(
                    "INSERT INTO graph_objects({OBJECT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
                ))
                .map_err(db_error)?;
            for object in &graph.objects {
                insert
                    .execute(params![
                        object.id,
                        object.kind.name(),
                        object.title,
                        object.subtitle,
                        object.date,
                        object.amount,
                        object.text,
                        serde_json::to_string(&object.record).map_err(json_error)?,
                    ])
                    .map_err(db_error)?;
            }
            let mut insert = transaction
                .prepare("INSERT OR IGNORE INTO graph_links(source, relation, target) VALUES (?1, ?2, ?3)")
                .map_err(db_error)?;
            for link in &graph.links {
                insert
                    .execute(params![link.from, text_of(&link.relation)?, link.to])
                    .map_err(db_error)?;
            }
            let mut insert = transaction
                .prepare(
                    "INSERT INTO graph_facts(subject, predicate, value_json, valid_from, valid_to,
                       sources_json, grade, tier) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(db_error)?;
            for fact in &graph.facts {
                insert
                    .execute(params![
                        fact.subject,
                        fact.predicate,
                        serde_json::to_string(&fact.value).map_err(json_error)?,
                        fact.from,
                        fact.to,
                        serde_json::to_string(&fact.sources).map_err(json_error)?,
                        fact.grade.as_ref().map(text_of).transpose()?,
                        text_of(&fact.tier)?,
                    ])
                    .map_err(db_error)?;
            }
        }
        transaction
            .execute(
                "INSERT INTO graph_meta(id, hash) VALUES (1, ?1)
                 ON CONFLICT(id) DO UPDATE SET hash = excluded.hash",
                [hash],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    pub fn graph_object(&self, id: &str) -> Result<Option<Object>, String> {
        object(&self.connection, id)
    }

    /// Links from or to an object.
    pub fn graph_links(&self, id: &str) -> Result<Vec<Link>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT source, relation, target FROM graph_links WHERE source = ?1
                 UNION SELECT source, relation, target FROM graph_links WHERE target = ?1
                 ORDER BY 1, 2, 3",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(db_error)?;
        rows.map(|row| {
            let (from, relation, to) = row.map_err(db_error)?;
            Ok(Link {
                from,
                relation: name(&relation)?,
                to,
            })
        })
        .collect()
    }

    /// Facts about an object, or every fact when `subject` is `None`.
    pub fn graph_facts(&self, subject: Option<&str>) -> Result<Vec<Fact>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT subject, predicate, value_json, valid_from, valid_to, sources_json, grade,
                   tier FROM graph_facts WHERE ?1 IS NULL OR subject = ?1
                 ORDER BY subject, predicate, rowid",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([subject], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(db_error)?;
        rows.map(|row| {
            let (subject, predicate, value, from, to, sources, grade, tier) =
                row.map_err(db_error)?;
            Ok(Fact {
                subject,
                predicate,
                value: serde_json::from_str(&value).map_err(json_error)?,
                from,
                to,
                sources: serde_json::from_str(&sources).map_err(json_error)?,
                grade: grade.as_deref().map(name).transpose()?,
                tier: name(&tier)?,
            })
        })
        .collect()
    }

    /// Clears the graph, as erasure does.
    pub(super) fn clear_graph(transaction: &rusqlite::Transaction<'_>) -> Result<(), String> {
        transaction
            .execute_batch(
                "DELETE FROM graph_objects; DELETE FROM graph_links; DELETE FROM graph_facts;
                 DELETE FROM graph_meta;",
            )
            .map_err(db_error)
    }
}
