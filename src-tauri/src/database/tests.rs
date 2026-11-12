use super::*;
use std::path::PathBuf;

/// Writes, at `path`, a database as schema `version` left it, holding `state`: today's schema
/// with what each later version added taken away, and the version stamped. Before version 20
/// there is no change log, so the tables alone hold the state. Covers versions 4 and later.
pub(crate) fn database_at(path: &Path, version: u32, state: &FinanceState) {
    let mut database = Database::open(path).unwrap();
    database.checkpoint(state, Reason::Start).unwrap();
    drop(database);
    let connection = Connection::open(path).unwrap();
    let run = |sql: &str| connection.execute_batch(sql).unwrap();
    if version < 24 {
        run("DROP TABLE investment_summaries;");
    }
    if version < 23 {
        run("DROP TABLE theses;");
    }
    if version < 22 {
        run("DROP TABLE research_log; DROP TABLE research_sources;");
        run("ALTER TABLE graph_facts DROP COLUMN proof_json;");
    }
    if version < 21 {
        run(
            "DROP TABLE graph_objects; DROP TABLE graph_links; DROP TABLE graph_facts;
             DROP TABLE graph_meta;",
        );
    }
    if version < 20 {
        run("DROP TABLE changesets; DROP TABLE log_device; DROP TABLE blobs;");
    }
    if version < 19 {
        run("DROP TABLE documents;");
    }
    if version < 17 {
        run("DROP TABLE ledger_events;
             ALTER TABLE statement_files DROP COLUMN confirmed_json;");
    }
    if version < 14 {
        run("DROP TABLE bank_statements; DROP TABLE bank_statement_checks;");
    }
    // Annotation columns, by the version that added each.
    for (added, column) in [
        (18, "people_json"),
        (16, "note"),
        (15, "benefit_id"),
        (13, "other"),
        (7, "refund_of"),
        (7, "splits_json"),
        (7, "tags_json"),
    ] {
        if version < added {
            run(&format!("ALTER TABLE annotations DROP COLUMN {column};"));
        }
    }
    run(&format!("PRAGMA user_version = {version};"));
}

/// The database's schema version, read as SQLite stores it.
pub(crate) fn user_version(path: &Path) -> u32 {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn intermediate_versions_migrate_to_the_current_schema_and_a_second_open_writes_nothing() {
    for version in [4, 10, 15, 20, 21, 23] {
        let path =
            std::env::temp_dir().join(format!("brief-v{version}-{}.sqlite3", uuid::Uuid::new_v4()));
        let mut state = crate::storage::empty_state().unwrap();
        state.revision = 3;
        state.annotations.insert(
            "kept".into(),
            Annotation {
                category: Some("Coffee".into()),
                reviewed: Some(true),
                ..Default::default()
            },
        );
        database_at(&path, version, &state);
        assert_eq!(user_version(&path), version);

        let database = Database::open(&path).unwrap();
        assert_eq!(
            user_version(&path),
            DATABASE_SCHEMA_VERSION,
            "from {version}"
        );
        let loaded = database.load().unwrap().unwrap();
        assert_eq!(loaded.revision, 3);
        assert!(loaded.annotations == state.annotations, "from {version}");
        // Every later version's tables are there, empty.
        let theses: i64 = database
            .connection
            .query_row("SELECT count(*) FROM theses", [], |row| row.get(0))
            .unwrap();
        assert_eq!(theses, 0);
        assert!(database.research_log().unwrap().is_empty());
        let start = chrono::NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        assert!(database.saved_summary("*", start).unwrap().is_none());
        drop(database);
        Database::validate_backup(&path).unwrap();

        let bytes = fs::read(&path).unwrap();
        drop(Database::open(&path).unwrap());
        assert_eq!(fs::read(&path).unwrap(), bytes, "reopening {version} wrote");
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_migration_that_fails_partway_leaves_the_older_database_as_it_was() {
    let path = std::env::temp_dir().join(format!("brief-v4-{}.sqlite3", uuid::Uuid::new_v4()));
    database_at(&path, 4, &crate::storage::empty_state().unwrap());
    // The scrub version 5 runs, after the schema steps, is refused.
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse BEFORE UPDATE ON sync_runs
               BEGIN SELECT RAISE(ABORT, 'refused'); END;
             INSERT INTO sync_runs(id, started_at, finished_at, outcome, warnings_json, error_code,
               details_json)
             VALUES ('old', '2026-09-01T00:00:00Z', '2026-09-01T00:00:01Z', 'failed', '[]', NULL,
               '{}');",
        )
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(Database::open(&path).is_err());
    assert_eq!(user_version(&path), 4);
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "nothing of the migration stayed"
    );
    fs::remove_file(path).unwrap();
}

/// The files a crash mid-write leaves: the database and the rollback journal of a write it
/// never committed.
pub(crate) fn crashed_mid_write(source: &Path, crashed: &Path) {
    let connection = Connection::open(source).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS crash_filler(bytes BLOB);
             PRAGMA cache_size = 1;
             BEGIN IMMEDIATE;",
        )
        .unwrap();
    for _ in 0..200 {
        connection
            .execute("INSERT INTO crash_filler VALUES (zeroblob(4096))", [])
            .unwrap();
    }
    let journal = |path: &Path| PathBuf::from(format!("{}-journal", path.display()));
    fs::copy(source, crashed).unwrap();
    fs::copy(journal(source), journal(crashed)).unwrap();
    connection.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn a_crash_mid_write_leaves_a_journal_the_version_check_rolls_back() {
    let directory = std::env::temp_dir().join(format!("brief-crash-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    let (source, crashed) = (
        directory.join("source.sqlite3"),
        directory.join("crashed.sqlite3"),
    );
    drop(Database::open(&source).unwrap());
    crashed_mid_write(&source, &crashed);
    assert!(PathBuf::from(format!("{}-journal", crashed.display())).exists());
    assert_eq!(
        Database::schema_version(&crashed).unwrap(),
        Some(DATABASE_SCHEMA_VERSION)
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_newer_database_is_refused_unchanged() {
    let path = std::env::temp_dir().join(format!("brief-newer-{}.sqlite3", uuid::Uuid::new_v4()));
    drop(Database::open(&path).unwrap());
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION + 1)
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(Database::open(&path).is_err());
    assert!(Database::validate_backup(&path).is_err());
    assert_eq!(
        Database::schema_version(&path).unwrap(),
        Some(DATABASE_SCHEMA_VERSION + 1)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
}

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
