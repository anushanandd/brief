use super::*;

#[test]
fn version_one_migration_preserves_data_and_the_unique_constraint() {
    let path =
        std::env::temp_dir().join(format!("brief-migration-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut db = Database::open(&path).unwrap();
    let mut state = crate::storage::empty_state().unwrap();
    state
        .account_links
        .insert("plaid:one".into(), "snaptrade:one".into());
    db.replace(&state).unwrap();
    db.connection.execute_batch("CREATE UNIQUE INDEX account_links_snaptrade_unique ON account_links(snaptrade_account_id); PRAGMA user_version = 1;").unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.load().unwrap().unwrap().account_links,
        state.account_links
    );
    assert!(db
        .connection
        .execute(
            "INSERT INTO account_links VALUES ('plaid:two', 'snaptrade:one')",
            []
        )
        .is_err());
    let redundant: u32 = db
        .connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'account_links_snaptrade_unique'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(redundant, 0);
    drop(db);
    fs::remove_file(path).unwrap();
}

#[test]
fn version_six_migration_adds_annotation_fields_and_keeps_existing_annotations() {
    let path =
        std::env::temp_dir().join(format!("brief-migration-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut db = Database::open(&path).unwrap();
    db.replace(&crate::storage::empty_state().unwrap()).unwrap();
    // Rebuild the annotations table as version 6 created it, holding one saved annotation.
    db.connection
        .execute_batch(
            "DROP TABLE annotations;
             CREATE TABLE annotations (
               transaction_id TEXT PRIMARY KEY, category TEXT,
               reviewed INTEGER CHECK (reviewed IN (0, 1)),
               benefit_confirmed INTEGER CHECK (benefit_confirmed IN (0, 1))
             ) STRICT;
             INSERT INTO annotations VALUES ('kept', 'Coffee', 1, NULL);
             PRAGMA user_version = 6;",
        )
        .unwrap();
    drop(db);

    let version = |db: &Database| {
        db.connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap()
    };
    let mut db = Database::open(&path).unwrap();
    assert_eq!(version(&db), DATABASE_SCHEMA_VERSION);
    // Later versions' tables exist too, empty.
    assert!(db.load().unwrap().unwrap().statements.is_empty());
    let mut annotations = db.load().unwrap().unwrap().annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations["kept"].category.as_deref(), Some("Coffee"));
    assert_eq!(annotations["kept"].reviewed, Some(true));
    assert!(annotations["kept"].tags.is_none() && annotations["kept"].splits.is_none());

    // The new fields round-trip beside the old ones, and reopening changes nothing.
    annotations.get_mut("kept").unwrap().tags = Some(vec!["trip".into()]);
    annotations.get_mut("kept").unwrap().people = Some(vec!["person:a".into()]);
    annotations.insert(
        "refund".into(),
        Annotation {
            // Nobody, chosen by hand, stays distinct from no choice.
            people: Some(Vec::new()),
            refund_of: Some("kept".into()),
            splits: Some(vec![
                crate::storage::Split {
                    category: "A".into(),
                    amount: 1.5,
                },
                crate::storage::Split {
                    category: "B".into(),
                    amount: 2.5,
                },
            ]),
            ..Default::default()
        },
    );
    db.replace_annotations(&annotations).unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(version(&db), DATABASE_SCHEMA_VERSION);
    assert!(db.load().unwrap().unwrap().annotations == annotations);
    Database::validate_backup(&path).unwrap();
    drop(db);
    fs::remove_file(path).unwrap();
}

#[test]
fn version_three_migration_adds_details_and_scrubs_legacy_provider_messages() {
    let path = std::env::temp_dir().join(format!("brief-v3-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut db = Database::open(&path).unwrap();
    let mut state = crate::storage::empty_state().unwrap();
    state.snapshot.sync_warnings = vec!["raw account message".into()];
    state.snapshot.provider_status = Some(BTreeMap::from([(
        "plaid".into(),
        crate::finance_contract::ProviderSyncStatus {
            updated_at: None,
            error: Some("raw account message".into()),
        },
    )]));
    state.plaid_cache = serde_json::from_value(serde_json::json!({
        "items": {"item": {
            "cursor": null, "transactions": [], "error": "raw account message",
            "data": {
                "accounts": [], "transactions": [], "investmentAccounts": [],
                "holdings": [], "securities": [], "warnings": ["raw account message"]
            }
        }}
    }))
    .unwrap();
    state.provider_data.plaid = Some(crate::providers::PlaidData {
        warnings: vec!["raw account message".into()],
        ..Default::default()
    });
    state.provider_data.sync_status.insert(
        "plaid".into(),
        crate::finance_contract::ProviderSyncStatus {
            updated_at: None,
            error: Some("raw account message".into()),
        },
    );
    db.replace(&state).unwrap();
    db.connection
        .execute_batch(
            "DROP INDEX sync_runs_finished_at;
             ALTER TABLE sync_runs RENAME TO sync_runs_v4;
             CREATE TABLE sync_runs (
               id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT NOT NULL,
               outcome TEXT NOT NULL CHECK (outcome IN ('committed', 'failed')),
               warnings_json TEXT NOT NULL CHECK (json_valid(warnings_json)), error_code TEXT
             ) STRICT;
             INSERT INTO sync_runs(id, started_at, finished_at, outcome, warnings_json, error_code)
             VALUES ('old', '2026-09-01T00:00:00Z', '2026-09-01T00:00:01Z', 'failed', '[\"raw account message\"]', 'refresh_timeout');
             DROP TABLE sync_runs_v4;
             CREATE INDEX sync_runs_finished_at ON sync_runs(finished_at DESC);
             PRAGMA user_version = 3;",
        )
        .unwrap();
    drop(db);

    let db = Database::open(&path).unwrap();
    let runs = db.sync_runs().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, "old");
    assert!(runs[0].warnings.is_empty());
    assert_eq!(runs[0].details.phase, "");
    let state = db.load().unwrap().unwrap();
    assert!(state.snapshot.sync_warnings.is_empty());
    assert_eq!(state.snapshot.provider_status.unwrap()["plaid"].error, None);
    assert!(state.provider_data.plaid.unwrap().warnings.is_empty());
    assert_eq!(state.provider_data.sync_status["plaid"].error, None);
    assert!(serde_json::to_value(state.plaid_cache).unwrap()["items"]["item"]["error"].is_null());
    assert_eq!(
        db.connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        DATABASE_SCHEMA_VERSION
    );
    db.connection
        .execute_batch(
            "UPDATE snapshot_scalar SET value_json = '[\"raw account message\"]' WHERE key = 'syncWarnings';
             PRAGMA user_version = 4;",
        )
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert!(db
        .load()
        .unwrap()
        .unwrap()
        .snapshot
        .sync_warnings
        .is_empty());
    drop(db);
    fs::remove_file(path).unwrap();
}

#[test]
fn normalized_store_round_trips_and_rejects_duplicate_entity_ids() {
    let mut database = Database::open(Path::new(":memory:")).unwrap();
    let schema_version = database
        .connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .unwrap();
    assert_eq!(schema_version, DATABASE_SCHEMA_VERSION);
    let mut state = crate::storage::empty_state().unwrap();
    let transaction = |id: &str| {
        serde_json::json!({
            "id": id, "merchant": "Shop", "category": "Other", "date": "2026-09-09",
            "amount": -1, "account": "Card", "pending": false
        })
    };
    crate::finance_contract::edit_json(&mut state.snapshot, |snapshot| {
        snapshot["transactions"] = serde_json::json!([transaction("one")]);
    });
    database.replace(&state).unwrap();
    let loaded = serde_json::to_value(database.load().unwrap().unwrap().snapshot).unwrap();
    let saved = serde_json::to_value(&state.snapshot).unwrap();
    assert_eq!(loaded["transactions"], saved["transactions"]);
    assert_eq!(loaded["netWorth"], saved["netWorth"]);
    assert_eq!(loaded["accountLinks"], serde_json::json!({}));

    crate::finance_contract::edit_json(&mut state.snapshot, |snapshot| {
        snapshot["transactions"] = serde_json::json!([transaction("same"), transaction("same")]);
    });
    assert!(database.replace(&state).is_err());
    assert_eq!(
        serde_json::to_value(database.load().unwrap().unwrap().snapshot).unwrap(),
        loaded
    );

    state.snapshot.transactions.clear();
    state.account_links = BTreeMap::from([
        ("plaid:one".into(), "snaptrade:same".into()),
        ("plaid:two".into(), "snaptrade:same".into()),
    ]);
    assert!(database.replace(&state).is_err());
}
