//! One search over every object: words through an SQLite FTS5 index, and meaning through
//! on-device sentence embeddings when words find too little. The index is built in memory from
//! the object graph (and saved news stories), rebuilt whenever the graph changes; embeddings
//! are cached on disk by text so each is computed once. Results are typed objects, so the
//! palette makes one call.
use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

use crate::objects::{Kind, Object};

pub mod embeddings;
#[cfg(test)]
mod tests;

/// How a result matched.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SearchMatch")]
pub enum Match {
    /// Its words begin with the query's words.
    Words,
    /// It means something close to the query, though the words differ.
    Meaning,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SearchHit")]
pub struct Hit {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount: Option<f64>,
    #[serde(rename = "match")]
    pub matched: Match,
}

/// A query's text and the kinds it is limited to (none: every kind).
#[derive(Clone, Debug, Default, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SearchQuery")]
pub struct Query {
    pub text: String,
    #[serde(default)]
    pub kinds: Vec<Kind>,
    /// At most this many results; the command caps it.
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<usize>,
}

pub const DEFAULT_LIMIT: usize = 40;
pub const MAX_LIMIT: usize = 200;
/// The least cosine similarity a meaning match needs.
const MEANING_FLOOR: f32 = 0.55;

/// What one object is matched on.
fn searchable(object: &Object) -> (String, String, String) {
    // Dates and amounts as people type them: "march 2026", "42.10".
    let mut text = object.text.clone();
    if let Some(date) = object
        .date
        .as_deref()
        .and_then(|date| chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok())
    {
        text.push_str(&format!(" {}", date.format("%B %Y")));
    }
    if let Some(amount) = object.amount {
        text.push_str(&format!(" {:.2}", amount.abs()));
    }
    (
        object.title.clone(),
        object.subtitle.clone().unwrap_or_default(),
        text,
    )
}

/// The words of a query as an FTS5 expression: each word a quoted prefix, all required.
fn fts_query(text: &str) -> Option<String> {
    let words = text
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\"*"))
        .collect::<Vec<_>>();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Unit vectors for some objects, by position in the index.
struct Meanings {
    positions: Vec<usize>,
    vectors: Vec<Vec<f32>>,
}

pub struct Index {
    /// Says which objects the index holds; a different key means rebuild.
    pub key: String,
    objects: Vec<Object>,
    words: Connection,
    meanings: Option<Meanings>,
}

impl Index {
    pub fn build(key: String, objects: Vec<Object>) -> Result<Self, String> {
        let words = Connection::open_in_memory().map_err(|error| error.to_string())?;
        words
            .execute_batch(
                "CREATE VIRTUAL TABLE objects USING fts5(
                   position UNINDEXED, kind UNINDEXED, title, subtitle, text,
                   tokenize = 'unicode61 remove_diacritics 2'
                 );",
            )
            .map_err(|error| error.to_string())?;
        {
            let transaction = words
                .unchecked_transaction()
                .map_err(|error| error.to_string())?;
            let mut insert = transaction
                .prepare("INSERT INTO objects VALUES (?1, ?2, ?3, ?4, ?5)")
                .map_err(|error| error.to_string())?;
            for (position, object) in objects.iter().enumerate() {
                let (title, subtitle, text) = searchable(object);
                insert
                    .execute(rusqlite::params![
                        position as i64,
                        object.kind.name(),
                        title,
                        subtitle,
                        text
                    ])
                    .map_err(|error| error.to_string())?;
            }
            drop(insert);
            transaction.commit().map_err(|error| error.to_string())?;
        }
        Ok(Self {
            key,
            objects,
            words,
            meanings: None,
        })
    }

    /// An object the index holds, such as a story the graph doesn't.
    pub fn object(&self, id: &str) -> Option<&Object> {
        self.objects.iter().find(|object| object.id == id)
    }

    /// The texts embeddings are computed from, by position, for objects not yet embedded.
    pub fn meaning_texts(&self) -> Option<Vec<(usize, String)>> {
        self.meanings.is_none().then(|| {
            self.objects
                .iter()
                .enumerate()
                .map(|(position, object)| {
                    let (title, subtitle, _) = searchable(object);
                    (position, format!("{title} {subtitle}").trim().to_owned())
                })
                .collect()
        })
    }

    /// Takes embeddings for objects by position; each vector is scaled to unit length.
    pub fn set_meanings(&mut self, embedded: Vec<(usize, Vec<f32>)>) {
        let (positions, vectors) = embedded
            .into_iter()
            .filter_map(|(position, vector)| Some((position, unit(vector)?)))
            .unzip();
        self.meanings = Some(Meanings { positions, vectors });
    }

    /// Objects whose words match, best first; then, when `meaning` (the query's embedding)
    /// is given and words found fewer than `limit`, objects close in meaning.
    pub fn search(
        &self,
        query: &Query,
        limit: usize,
        meaning: Option<Vec<f32>>,
    ) -> Result<Vec<Hit>, String> {
        let allowed = |kind: Kind| query.kinds.is_empty() || query.kinds.contains(&kind);
        let mut hits = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        // No words but a kind, as when the palette asks which object a verb acts on: every
        // object of that kind, the most recent first.
        if fts_query(&query.text).is_none() && !query.kinds.is_empty() {
            let mut listed = self
                .objects
                .iter()
                .filter(|object| allowed(object.kind))
                .collect::<Vec<_>>();
            listed.sort_by(|left, right| {
                right
                    .date
                    .cmp(&left.date)
                    .then(left.title.cmp(&right.title))
            });
            return Ok(listed
                .into_iter()
                .take(limit)
                .map(|object| hit(object, Match::Words))
                .collect());
        }
        if let Some(expression) = fts_query(&query.text) {
            let mut statement = self
                .words
                .prepare(
                    "SELECT position FROM objects WHERE objects MATCH ?1
                     ORDER BY bm25(objects, 0.0, 0.0, 10.0, 3.0, 1.0)",
                )
                .map_err(|error| error.to_string())?;
            let positions = statement
                .query_map([expression], |row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            for position in positions {
                let position = usize::try_from(position.map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
                let object = &self.objects[position];
                if hits.len() < limit && allowed(object.kind) && seen.insert(position) {
                    hits.push(hit(object, Match::Words));
                }
            }
        }
        if let (Some(meanings), Some(query_vector)) = (&self.meanings, meaning.and_then(unit)) {
            let mut close = meanings
                .positions
                .iter()
                .zip(&meanings.vectors)
                .filter(|(position, _)| {
                    !seen.contains(*position) && allowed(self.objects[**position].kind)
                })
                .map(|(position, vector)| (*position, dot(vector, &query_vector)))
                .filter(|(_, similarity)| *similarity >= MEANING_FLOOR)
                .collect::<Vec<_>>();
            close.sort_by(|left, right| right.1.total_cmp(&left.1));
            for (position, _) in close.into_iter().take(limit.saturating_sub(hits.len())) {
                hits.push(hit(&self.objects[position], Match::Meaning));
            }
        }
        Ok(hits)
    }
}

fn hit(object: &Object, matched: Match) -> Hit {
    Hit {
        id: object.id.clone(),
        kind: object.kind,
        title: object.title.clone(),
        subtitle: object.subtitle.clone(),
        date: object.date.clone(),
        amount: object.amount,
        matched,
    }
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

/// The vector scaled to unit length; none for an empty or zero vector.
fn unit(vector: Vec<f32>) -> Option<Vec<f32>> {
    let length = dot(&vector, &vector).sqrt();
    (length > 0.0 && length.is_finite())
        .then(|| vector.into_iter().map(|value| value / length).collect())
}

/// A saved news story as a searchable object.
pub fn story(article: &crate::providers::MarketNewsArticle) -> Object {
    Object {
        id: Kind::Story.id(&crate::database::sha256(article.url.as_bytes())[..16]),
        kind: Kind::Story,
        title: article.headline.clone(),
        subtitle: Some(format!(
            "{} · {}",
            article.source,
            article.symbols.join(", ")
        )),
        date: article.created_at.get(..10).map(str::to_owned),
        amount: None,
        text: article.summary.clone(),
        record: serde_json::to_value(article).unwrap_or_default(),
    }
}
