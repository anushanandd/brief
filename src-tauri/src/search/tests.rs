use std::cell::Cell;

use serde_json::json;

use super::{embeddings::Embedder, *};

fn object(kind: Kind, key: &str, title: &str, subtitle: &str, date: &str, amount: f64) -> Object {
    Object {
        id: kind.id(key),
        kind,
        title: title.into(),
        subtitle: Some(subtitle.into()),
        date: Some(date.into()),
        amount: Some(amount),
        text: String::new(),
        record: json!({}),
    }
}

fn index() -> Index {
    Index::build(
        "key".into(),
        vec![
            object(
                Kind::Transaction,
                "t1",
                "Amazon",
                "Shopping",
                "2026-03-04",
                -42.1,
            ),
            object(
                Kind::Transaction,
                "t2",
                "Amazon",
                "Shopping",
                "2026-05-09",
                -18.0,
            ),
            object(Kind::Organization, "amazon", "Amazon", "merchant", "", 0.0),
            object(
                Kind::Document,
                "w2",
                "2025 W-2.pdf",
                "W-2",
                "2026-01-31",
                0.0,
            ),
            object(Kind::Place, "p1", "Ramen Shop", "Tokyo", "", 0.0),
        ],
    )
    .unwrap()
}

fn query(text: &str, kinds: &[Kind]) -> Query {
    Query {
        text: text.into(),
        kinds: kinds.to_vec(),
        limit: None,
    }
}

fn ids(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

#[test]
fn words_match_by_prefix_and_a_month_or_amount_narrows_them() {
    let index = index();
    let found = index.search(&query("amaz", &[]), 10, None).unwrap();
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|hit| hit.matched == Match::Words));
    assert_eq!(
        ids(&index.search(&query("amazon march", &[]), 10, None).unwrap()),
        ["transaction:t1"]
    );
    assert_eq!(
        ids(&index.search(&query("42.10", &[]), 10, None).unwrap()),
        ["transaction:t1"]
    );
    // Punctuation and quotes are word breaks, never FTS syntax.
    assert_eq!(
        ids(&index.search(&query("w-2 \"", &[]), 10, None).unwrap()),
        ["document:w2"]
    );
    assert!(index
        .search(&query("  ", &[]), 10, None)
        .unwrap()
        .is_empty());
}

#[test]
fn a_kind_without_words_lists_its_objects_newest_first() {
    let index = index();
    assert_eq!(
        ids(&index
            .search(&query("", &[Kind::Transaction]), 10, None)
            .unwrap()),
        ["transaction:t2", "transaction:t1"]
    );
    assert!(index.search(&query("", &[]), 10, None).unwrap().is_empty());
}

#[test]
fn kinds_and_limits_narrow_results() {
    let index = index();
    assert_eq!(
        ids(&index
            .search(&query("amazon", &[Kind::Organization]), 10, None)
            .unwrap()),
        ["organization:amazon"]
    );
    assert_eq!(
        index.search(&query("amazon", &[]), 2, None).unwrap().len(),
        2
    );
}

#[test]
fn meaning_fills_in_after_words_and_never_repeats_them() {
    let mut index = index();
    // Unit axes stand in for embeddings: the ramen shop points where the query does.
    index.set_meanings(
        (0..5)
            .map(|position| {
                let mut vector = vec![0.0; 5];
                vector[position] = 1.0;
                (position, vector)
            })
            .collect(),
    );
    let found = index
        .search(
            &query("noodles", &[]),
            10,
            Some(vec![0.1, 0.0, 0.0, 0.0, 1.0]),
        )
        .unwrap();
    assert_eq!(ids(&found), ["place:p1"]);
    assert_eq!(found[0].matched, Match::Meaning);
    // A word match is listed once, as a word match.
    let found = index
        .search(
            &query("ramen", &[]),
            10,
            Some(vec![0.0, 0.0, 0.0, 0.0, 1.0]),
        )
        .unwrap();
    assert_eq!(ids(&found), ["place:p1"]);
    assert_eq!(found[0].matched, Match::Words);
}

struct Counting(Cell<usize>);

impl Embedder for Counting {
    fn embed(&self, texts: &[String]) -> Option<Vec<Vec<f32>>> {
        self.0.set(self.0.get() + texts.len());
        Some(
            texts
                .iter()
                .map(|text| vec![text.len() as f32, 1.0])
                .collect(),
        )
    }
}

#[test]
fn each_text_is_embedded_once_and_the_cache_clears() {
    let path =
        std::env::temp_dir().join(format!("brief-embeddings-{}.sqlite3", uuid::Uuid::new_v4()));
    let embedder = Counting(Cell::new(0));
    let texts = vec![
        (0, "Amazon".to_owned()),
        (1, "Ramen".to_owned()),
        (2, "Amazon".to_owned()),
    ];
    let first = embeddings::embed_cached(&path, &embedder, &texts).unwrap();
    assert_eq!(first.len(), 3);
    assert_eq!(embedder.0.get(), 2);
    let again = embeddings::embed_cached(&path, &embedder, &texts).unwrap();
    assert_eq!(again, first);
    assert_eq!(embedder.0.get(), 2);
    embeddings::clear(&path).unwrap();
    assert!(!path.exists());
    embeddings::clear(&path).unwrap();
}
