use std::path::Path;

use serde_json::json;

use super::*;
use crate::{database::Database, statements::StatementFile};

/// A Plaid cache holding `transactions` and one card account with `balance`.
fn cache(transactions: Value, balance: f64) -> PlaidCache {
    serde_json::from_value(json!({ "items": { "item": {
        "cursor": "cursor",
        "transactions": transactions,
        "data": {
            "accounts": [{ "account_id": "card", "balances": { "current": balance } }],
            "transactions": [], "investmentAccounts": [], "holdings": [], "securities": []
        }
    }}}))
    .unwrap()
}

fn state(revision: u64, plaid_cache: PlaidCache) -> FinanceState {
    let mut state = crate::storage::empty_state().unwrap();
    state.revision = revision;
    state.plaid_cache = plaid_cache;
    state
}

/// The log as (source, kind, id, amount or balance), oldest first.
fn events(database: &Database) -> Vec<(String, String, String, Option<f64>)> {
    let mut query = database
        .connection
        .prepare("SELECT source, kind, entity_id, payload_json FROM ledger_events ORDER BY seq")
        .unwrap();
    query
        .query_map([], |row| {
            let payload = row
                .get::<_, Option<String>>(3)?
                .map(|json| serde_json::from_str::<Value>(&json).unwrap());
            let figure = payload.as_ref().and_then(|value| {
                value
                    .get("amount")
                    .or_else(|| value.get("current"))
                    .and_then(Value::as_f64)
            });
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, figure))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn event(
    source: &str,
    kind: &str,
    id: &str,
    figure: Option<f64>,
) -> (String, String, String, Option<f64>) {
    (source.into(), kind.into(), id.into(), figure)
}

#[test]
fn a_pending_charge_that_posts_with_a_tip_keeps_both_versions() {
    let mut database = Database::open(Path::new(":memory:")).unwrap();
    database.replace(&state(0, PlaidCache::default())).unwrap();
    assert!(events(&database).is_empty());

    let pending = json!({ "transaction_id": "pending", "amount": 40.0, "pending": true });
    database
        .replace(&state(1, cache(json!([pending]), 40.0)))
        .unwrap();
    let posted = json!({
        "transaction_id": "posted", "amount": 48.0, "pending": false,
        "pending_transaction_id": "pending"
    });
    let settled = state(2, cache(json!([posted]), 48.0));
    database.replace(&settled).unwrap();
    // Writing the same evidence again appends nothing.
    database.replace(&settled).unwrap();

    assert_eq!(
        events(&database),
        [
            event("plaid_balance", "added", "card", Some(40.0)),
            event("plaid_transaction", "added", "pending", Some(40.0)),
            event("plaid_balance", "modified", "card", Some(48.0)),
            event("plaid_transaction", "added", "posted", Some(48.0)),
            event("plaid_transaction", "removed", "pending", None),
        ]
    );
}

#[test]
fn annotations_and_statement_files_are_logged() {
    let mut database = Database::open(Path::new(":memory:")).unwrap();
    database.replace(&state(0, PlaidCache::default())).unwrap();
    let note = Annotation {
        note: Some("shared dinner".into()),
        ..Default::default()
    };
    database
        .replace_annotations(&BTreeMap::from([("posted".into(), note)]))
        .unwrap();
    database.replace_annotations(&BTreeMap::new()).unwrap();

    let mut imported = state(1, PlaidCache::default());
    imported.statements.push(StatementFile {
        id: "a".repeat(64),
        account_id: "plaid:card".into(),
        file_name: "statement.csv".into(),
        imported_at: "2026-10-01T00:00:00Z".into(),
        contents: b"Date,Amount".to_vec(),
        confirmed_rows: Default::default(),
    });
    database.replace(&imported).unwrap();

    assert_eq!(
        events(&database),
        [
            event("annotation", "added", "posted", None),
            event("annotation", "removed", "posted", None),
            event("statement", "added", &"a".repeat(64), None),
        ]
    );
}

#[test]
fn a_failed_write_appends_nothing() {
    let mut database = Database::open(Path::new(":memory:")).unwrap();
    database.replace(&state(0, PlaidCache::default())).unwrap();
    let mut invalid = state(
        1,
        cache(json!([{ "transaction_id": "kept", "amount": 5.0 }]), 5.0),
    );
    invalid.statements.push(StatementFile {
        id: "not a digest".into(),
        account_id: "plaid:card".into(),
        file_name: "statement.csv".into(),
        imported_at: "2026-10-01T00:00:00Z".into(),
        contents: b"Date,Amount".to_vec(),
        confirmed_rows: Default::default(),
    });
    assert!(database.replace(&invalid).is_err());
    assert!(events(&database).is_empty());
}

#[test]
fn the_log_refuses_edits_and_erasure_starts_it_over() {
    let mut database = Database::open(Path::new(":memory:")).unwrap();
    database.replace(&state(0, PlaidCache::default())).unwrap();
    database
        .replace(&state(
            1,
            cache(json!([{ "transaction_id": "one", "amount": 5.0 }]), 5.0),
        ))
        .unwrap();
    for edit in [
        "UPDATE ledger_events SET kind = 'removed'",
        "DELETE FROM ledger_events",
    ] {
        assert!(database.connection.execute(edit, []).is_err());
    }
    assert_eq!(events(&database).len(), 2);

    database.erase(&state(2, PlaidCache::default())).unwrap();
    assert!(events(&database).is_empty());
    database.replace(&state(3, cache(json!([]), 7.0))).unwrap();
    assert_eq!(
        events(&database),
        [event("plaid_balance", "added", "card", Some(7.0))]
    );
}

#[test]
fn upgrading_records_what_the_database_holds_once() {
    let path = std::env::temp_dir().join(format!("brief-journal-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut database = Database::open(&path).unwrap();
    database
        .replace(&state(
            4,
            cache(json!([{ "transaction_id": "old", "amount": 9.0 }]), 9.0),
        ))
        .unwrap();
    // A database's first state predates its log, so it is a baseline too.
    let baseline = [
        event("plaid_balance", "baseline", "card", Some(9.0)),
        event("plaid_transaction", "baseline", "old", Some(9.0)),
    ];
    assert_eq!(events(&database), baseline);
    database
        .connection
        .execute_batch("DROP TABLE ledger_events; PRAGMA user_version = 16;")
        .unwrap();
    drop(database);

    for _ in 0..2 {
        let database = Database::open(&path).unwrap();
        assert_eq!(events(&database), baseline);
    }
    std::fs::remove_file(path).unwrap();
}
