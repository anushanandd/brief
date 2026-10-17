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
    db.checkpoint(&state, Reason::Start).unwrap();
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
    db.checkpoint(&crate::storage::empty_state().unwrap(), Reason::Start)
        .unwrap();
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
    let mut annotated = db.load().unwrap().unwrap();
    let change = Change::annotations(&annotated.annotations, &annotations);
    annotated.annotations = annotations.clone();
    db.commit(&annotated, &change).unwrap();
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
    db.checkpoint(&state, Reason::Start).unwrap();
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
    database.checkpoint(&state, Reason::Start).unwrap();
    let loaded = serde_json::to_value(database.load().unwrap().unwrap().snapshot).unwrap();
    let saved = serde_json::to_value(&state.snapshot).unwrap();
    assert_eq!(loaded["transactions"], saved["transactions"]);
    assert_eq!(loaded["netWorth"], saved["netWorth"]);
    assert_eq!(loaded["accountLinks"], serde_json::json!({}));

    crate::finance_contract::edit_json(&mut state.snapshot, |snapshot| {
        snapshot["transactions"] = serde_json::json!([transaction("same"), transaction("same")]);
    });
    assert!(database.checkpoint(&state, Reason::Start).is_err());
    assert_eq!(
        serde_json::to_value(database.load().unwrap().unwrap().snapshot).unwrap(),
        loaded
    );

    state.snapshot.transactions.clear();
    state.account_links = BTreeMap::from([
        ("plaid:one".into(), "snaptrade:same".into()),
        ("plaid:two".into(), "snaptrade:same".into()),
    ]);
    assert!(database.checkpoint(&state, Reason::Start).is_err());
}

#[test]
fn version_twenty_two_keeps_proofs_and_a_research_log() {
    let path =
        std::env::temp_dir().join(format!("brief-migration-{}.sqlite3", uuid::Uuid::new_v4()));
    let mut db = Database::open(&path).unwrap();
    db.checkpoint(&crate::storage::empty_state().unwrap(), Reason::Start)
        .unwrap();
    // Version 21's facts table, with no proofs and a saved graph hash.
    db.connection
        .execute_batch(
            "DROP TABLE graph_facts; DROP TABLE research_log; DROP TABLE research_sources;
             CREATE TABLE graph_facts (
               subject TEXT NOT NULL, predicate TEXT NOT NULL,
               value_json TEXT NOT NULL CHECK (json_valid(value_json)),
               valid_from TEXT, valid_to TEXT,
               sources_json TEXT NOT NULL CHECK (json_valid(sources_json)),
               grade TEXT, tier TEXT NOT NULL
             ) STRICT;
             INSERT INTO graph_meta(id, hash) VALUES (1, printf('%064d', 0));
             PRAGMA user_version = 21;",
        )
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    // The old graph's hash is forgotten, so the next read writes the graph with its proofs.
    assert_eq!(db.graph_hash().unwrap(), None);
    let proof = crate::evidence::Proof {
        program: "rule".into(),
        name: "Rule".into(),
        source: crate::evidence::programs::Source {
            url: "https://example.com/terms".into(),
            title: "Terms".into(),
        },
        reviewed: chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
        steps: vec![crate::evidence::Step {
            says: "A record".into(),
            evidence: vec!["transaction:t1".into()],
            detail: None,
            met: true,
        }],
        unchecked: Vec::new(),
        notes: Vec::new(),
    };
    let graph = crate::objects::Graph {
        objects: Vec::new(),
        links: Vec::new(),
        facts: vec![crate::objects::Fact {
            subject: "transaction:t1".into(),
            predicate: "offsets".into(),
            value: serde_json::json!("transaction:t2"),
            from: Some("2026-07-15".into()),
            to: None,
            sources: vec!["program:rule".into(), "transaction:t1".into()],
            grade: None,
            tier: crate::objects::Tier::Explained,
            proof: Some(proof),
        }],
    };
    db.write_graph(&graph, &graph.hash().unwrap()).unwrap();
    assert_eq!(db.graph_facts(None).unwrap(), graph.facts);

    // The log shows a query before it runs and what came back after, newest first.
    let now = chrono::Utc::now();
    let mut entry = crate::research::Entry::begin(
        crate::research::EntryKind::Search,
        "Example Bank checking fee waiver".into(),
        Some("account:example-bank".into()),
        now,
    );
    db.begin_research(&entry).unwrap();
    assert_eq!(
        db.research_log().unwrap()[0].status,
        crate::research::Status::Running
    );
    entry.finish(
        Ok(vec![crate::providers::exa::Page {
            url: "https://bank.example/fees".into(),
            title: "Fees".into(),
            text: "No monthly fee with $500 in deposits.".into(),
            published: None,
        }]),
        now,
    );
    db.finish_research(&entry).unwrap();
    assert_eq!(db.research_log().unwrap(), vec![entry]);
    let check = crate::research::checked(None, "https://bank.example/fees", "text", now);
    db.save_source_check(&check).unwrap();
    assert_eq!(
        db.source_check("https://bank.example/fees").unwrap(),
        Some(check)
    );
    // Erasure clears the log with everything else.
    let mut db = db;
    db.erase(&crate::storage::empty_state().unwrap()).unwrap();
    assert!(db.research_log().unwrap().is_empty() && db.source_checks().unwrap().is_empty());
    drop(db);
    fs::remove_file(path).unwrap();
}
