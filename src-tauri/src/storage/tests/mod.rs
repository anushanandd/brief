use super::*;
use crate::finance_contract::edit_json;
use crate::providers::SnapTradeData;

/// A snapshot as JSON, for assertions about what the renderer and the database receive.
fn json(snapshot: &Snapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()
}

#[test]
fn old_history_corrections_are_removed_during_projection_upgrade() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.data.snapshot.updated_at = "2026-09-10T07:30:00Z".into();
    storage.data.snapshot.calculation_version = Some(22);
    storage.database.replace(&storage.data).unwrap();
    // A key from an older build, which the typed snapshot no longer carries.
    rusqlite::Connection::open(&storage.path)
        .unwrap()
        .execute(
            "INSERT INTO snapshot_scalar(key, value_json) VALUES ('historyCorrections', ?1)",
            [r#"[{"accountId": "snaptrade:brokerage", "date": "2026-09-08", "value": 200}]"#],
        )
        .unwrap();
    storage.data.provider_data.snaptrade = Some(SnapTradeData {
        accounts: vec![serde_json::json!({
            "id": "brokerage", "name": "Brokerage", "institution_name": "Broker",
            "balance": { "total": { "amount": 200, "currency": "USD" } }
        })],
        cash_balances: BTreeMap::from([(
            "brokerage".into(),
            vec![serde_json::json!({
                "currency": { "code": "USD" }, "cash": 200
            })],
        )]),
        positions: BTreeMap::from([("brokerage".into(), vec![])]),
        balance_history: BTreeMap::from([(
            "brokerage".into(),
            vec![
                serde_json::json!({ "date": "2026-09-07", "total_value": 200 }),
                serde_json::json!({ "date": "2026-09-08", "total_value": 9200 }),
            ],
        )]),
        ..SnapTradeData::default()
    });
    assert!(storage.upgrade_projection().unwrap());
    assert_eq!(
        storage
            .data
            .snapshot
            .net_worth_history
            .iter()
            .find(|point| point.date == "2026-09-08")
            .unwrap()
            .value,
        9200.0
    );
    let corrections: i64 = rusqlite::Connection::open(&storage.path)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM snapshot_scalar WHERE key = 'historyCorrections'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(corrections, 0);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn older_calculation_versions_rebuild_from_the_committed_cache() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.data.snapshot.updated_at = "2026-09-10T07:30:00Z".into();
    storage.data.snapshot.calculation_version = Some(4);
    edit_json(&mut storage.data.snapshot, |snapshot| {
        snapshot["benchmarkHistory"] = serde_json::json!([
            { "date": "2026-09-08", "value": 100 },
            { "date": "2026-09-10", "value": 101 }
        ]);
    });
    storage.data.provider_data.snaptrade = Some(SnapTradeData {
        accounts: vec![serde_json::json!({
            "id": "brokerage", "name": "Brokerage", "institution_name": "Broker",
            "balance": { "total": { "amount": 200, "currency": "USD" } }
        })],
        cash_balances: BTreeMap::from([(
            "brokerage".into(),
            vec![serde_json::json!({
                "currency": { "code": "USD" }, "cash": 200
            })],
        )]),
        positions: BTreeMap::from([("brokerage".into(), vec![])]),
        activities: BTreeMap::from([(
            "brokerage".into(),
            vec![serde_json::json!({
                "id": "transfer", "type": "TRANSFER", "amount": 10,
                "trade_date": "2026-09-09T15:30:00Z"
            })],
        )]),
        balance_history: BTreeMap::from([(
            "brokerage".into(),
            vec![serde_json::json!({
                "date": "2026-09-08", "total_value": 190
            })],
        )]),
        history_complete: true,
        activity_complete: true,
        ..SnapTradeData::default()
    });

    assert!(storage.upgrade_projection().unwrap());
    assert_eq!(
        storage.data.snapshot.calculation_version,
        Some(crate::financial_engine::CALCULATION_VERSION)
    );
    let account = &storage.data.snapshot.brokerage_performance[1];
    assert_eq!(account.points[1].net_deposits, Some(200.0));
    assert_eq!(account.points[1].sp500, Some(201.9));
    drop(storage);
    assert_eq!(
        Storage::new(directory.clone())
            .unwrap()
            .data
            .snapshot
            .calculation_version,
        Some(crate::financial_engine::CALCULATION_VERSION)
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn corrupted_primary_opens_read_only_backup_and_restores_without_discarding_original() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let mut snapshot = json(&storage.data.snapshot);
    snapshot["updatedAt"] = "2026-09-01T12:00:00Z".into();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    storage.commit_value(&id, snapshot.clone()).unwrap();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            1,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    storage.commit_value(&id, snapshot).unwrap();
    fs::write(&storage.path, b"synthetic corrupt state").unwrap();
    drop(storage);
    let mut storage = Storage::new(directory.clone()).unwrap();
    assert_eq!(storage.data.revision, 1);
    assert_eq!(storage.snapshot().unwrap()["recovery"]["canRestore"], true);
    // Rust's readers, such as Data health, see the same recovery the renderer does.
    assert!(storage.read_view().unwrap().snapshot.recovery.is_some());
    assert!(storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            1,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![]
        )
        .is_err());
    assert!(storage.save_annotations(BTreeMap::new(), true).is_ok());
    assert_eq!(fs::read(&storage.path).unwrap(), b"synthetic corrupt state");
    let restored = storage.recover(true).unwrap();
    assert!(restored.get("recovery").is_none());
    assert_eq!(load_database(&storage.path).unwrap().1.revision, 1);
    assert!(fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .any(
            |entry| entry.file_name().to_string_lossy().contains("retained-")
                && fs::read(entry.path()).unwrap() == b"synthetic corrupt state"
        ));
    drop(storage);
    assert!(Storage::new(directory.clone()).unwrap().recovery.is_none());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recovery_without_a_valid_backup_requires_an_explicit_fresh_start() {
    let directory = directory();
    let storage = Storage::new(directory.clone()).unwrap();
    fs::write(&storage.path, b"corrupt primary").unwrap();
    fs::write(storage.backup_path(), b"corrupt backup").unwrap();
    drop(storage);
    let mut storage = Storage::new(directory.clone()).unwrap();
    assert!(!storage.recovery.as_ref().unwrap().can_restore);
    assert!(storage.recover(true).is_err());
    assert!(storage.recover(false).is_ok());
    assert_eq!(storage.data.revision, 0);
    assert_eq!(
        fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("retained-"))
            .count(),
        2
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_missing_primary_does_not_overwrite_a_valid_retained_snapshot() {
    let directory = directory();
    let storage = Storage::new(directory.clone()).unwrap();
    fs::remove_file(&storage.path).unwrap();
    drop(storage);
    let storage = Storage::new(directory.clone()).unwrap();
    assert!(storage.recovery.as_ref().unwrap().can_restore);
    assert!(!storage.path.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn nullable_values_require_an_explicit_incomplete_subtotal() {
    let mut snapshot = empty_state().unwrap().snapshot;
    edit_json(&mut snapshot, |snapshot| {
        snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 100},
            {"id": "cash", "name": "Cash", "institution": "Bank", "type": "cash", "value": 100},
            {"id": "card", "name": "Card", "institution": "Bank", "type": "credit", "value": null}
        ]);
        snapshot["netWorth"] = 100.into();
        snapshot["holdings"] = serde_json::json!([{"ticker": "UNPRICED", "name": "Unpriced", "accountId": "cash", "shares": null, "price": null, "value": null, "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": "#000000"}]);
    });
    assert!(validate_snapshot(&snapshot).is_err());
    snapshot.net_worth_incomplete = true;
    assert!(validate_snapshot(&snapshot).is_ok());
}

#[test]
fn confirmed_account_links_are_one_to_one() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    edit_json(&mut storage.data.snapshot, |snapshot| {
        snapshot["accounts"] = serde_json::json!([
            {"id": "all", "name": "All", "institution": "Brief", "type": "combined", "value": 0},
            {"id": "plaid:one", "name": "One", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "plaid:two", "name": "Two", "institution": "Broker", "type": "brokerage", "value": 0},
            {"id": "snaptrade:same", "name": "Same", "institution": "Broker", "type": "brokerage", "value": 0}
        ]);
        snapshot["possibleDuplicateAccounts"] = serde_json::json!([
            {"plaidAccountId": "plaid:one", "snaptradeAccountId": "snaptrade:same", "description": "Candidate one"},
            {"plaidAccountId": "plaid:two", "snaptradeAccountId": "snaptrade:same", "description": "Candidate two"}
        ]);
    });
    storage
        .save_account_link("plaid:one".into(), Some("snaptrade:same".into()))
        .unwrap();
    assert!(storage
        .save_account_link("plaid:two".into(), Some("snaptrade:same".into()))
        .is_err());
    assert_eq!(storage.data.account_links.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_validation_retains_original_run_identity_and_committed_state() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let original = json(&storage.data.snapshot);
    let original_disk = json(&storage.database.load().unwrap().unwrap().snapshot);
    let started = "2026-09-18T12:00:00Z";
    storage
        .stage(
            "synthetic-run",
            started,
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    let mut invalid: Snapshot = crate::finance_contract::decode(original.clone()).unwrap();
    invalid.net_worth = 999.0;
    let error = storage.commit("synthetic-run", invalid).unwrap_err();
    assert_eq!(error.code(), "validation_failed");
    storage.fail_pending("synthetic-run", error.code());
    assert_eq!(json(&storage.data.snapshot), original);
    assert_eq!(
        json(&storage.database.load().unwrap().unwrap().snapshot),
        original_disk
    );
    let runs = serde_json::to_value(storage.sync_runs().unwrap()).unwrap();
    assert_eq!(runs[0]["id"], "synthetic-run");
    assert_eq!(runs[0]["startedAt"], started);
    assert_eq!(runs[0]["errorCode"], "validation_failed");
    assert_eq!(runs[0]["details"]["phase"], "validation");
    assert!(storage.pending.is_none());
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_committed_refresh_keeps_what_each_provider_used_and_each_stage_took() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let snapshot: Snapshot = crate::finance_contract::decode(json(&storage.data.snapshot)).unwrap();
    storage
        .stage(
            "measured-run",
            "2026-09-18T12:00:00Z",
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    let usage = std::collections::BTreeMap::from([(
        "Plaid".to_string(),
        crate::providers::http::Usage {
            requests: 7,
            retries: 1,
            bytes: 4_096,
            duration_ms: Some(1_250),
        },
    )]);
    storage.note_usage("measured-run", usage, 1_400);
    storage.note_usage("another-run", Default::default(), 1);
    storage.note_projection("measured-run", std::time::Duration::from_millis(80));
    assert!(storage.commit("measured-run", snapshot).is_ok());

    let runs = serde_json::to_value(storage.sync_runs().unwrap()).unwrap();
    let details = &runs[0]["details"];
    assert_eq!(runs[0]["outcome"], "committed");
    assert_eq!(
        details["runs"],
        serde_json::json!([
            { "provider": "Plaid", "requests": 7, "retries": 1, "bytes": 4096, "durationMs": 1250 }
        ])
    );
    assert_eq!(details["stages"]["fetchMs"], 1_400);
    assert_eq!(details["stages"]["projectMs"], 80);
    assert!(details["stages"]["commitMs"].is_u64());
    assert!(details["snapshotBytes"].as_u64().unwrap() > 0);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn explicit_erasure_replaces_finance_state_and_removes_retained_copies() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.data.snapshot.net_worth = 123.0;
    storage.database.replace(&storage.data).unwrap();
    storage.database.backup(&storage.backup_path()).unwrap();
    let retained = storage.path.with_extension("retained-test.sqlite3");
    fs::write(&retained, b"retained financial data").unwrap();
    let revision = storage.data.revision;

    let snapshot = storage.erase().unwrap();

    assert_eq!(snapshot["netWorth"], 0.0);
    assert_eq!(snapshot["revision"], revision + 1);
    assert!(!storage.backup_path().exists());
    assert!(!retained.exists());
    drop(storage);
    assert_eq!(
        Storage::new(directory.clone()).unwrap().snapshot().unwrap()["netWorth"],
        0.0
    );
    fs::remove_dir_all(directory).unwrap();
}

fn directory() -> PathBuf {
    std::env::temp_dir().join(format!("brief-storage-{}", uuid::Uuid::new_v4()))
}

#[test]
fn consecutive_small_edits_share_one_retained_copy_until_the_revision_changes() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let backup_annotations = |storage: &Storage| {
        Database::open(&storage.backup_path())
            .unwrap()
            .load()
            .unwrap()
            .unwrap()
            .annotations
    };
    let edit = |storage: &mut Storage, id: &str| {
        storage
            .save_annotations(
                BTreeMap::from([(
                    id.to_owned(),
                    Annotation {
                        reviewed: Some(true),
                        ..Default::default()
                    },
                )]),
                false,
            )
            .unwrap();
    };
    edit(&mut storage, "first");
    // The copy retained before the first edit predates it.
    assert!(backup_annotations(&storage).is_empty());
    edit(&mut storage, "second");
    assert!(
        backup_annotations(&storage).is_empty(),
        "a recent pre-edit copy is reused"
    );
    // A new revision always retains a fresh copy before the next edit.
    storage.data.revision += 1;
    edit(&mut storage, "third");
    assert_eq!(backup_annotations(&storage).len(), 2);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn settings_and_annotations_persist_apply_at_read_time_and_are_erased() {
    use crate::settings::FinanceSettings;

    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.data.snapshot =
        serde_json::from_str(include_str!("../../../../src/data/seed.json")).unwrap();
    let transaction = storage.data.snapshot.transactions[0].clone();
    let id = transaction.id.clone();

    let saved = storage
        .save_settings(
            FinanceSettings {
                spending_account_id: Some("plaid:card".into()),
                account_names: BTreeMap::from([("plaid:card".into(), " Card ".into())]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    assert_eq!(saved["settings"]["accountNames"]["plaid:card"], "Card");
    // A legacy import fills only values that are still unset.
    let imported = storage
        .save_settings(
            FinanceSettings {
                spending_account_id: Some("plaid:legacy".into()),
                hidden_benefit_ids: vec!["resy".into()],
                ..Default::default()
            },
            true,
        )
        .unwrap();
    assert_eq!(imported["settings"]["spendingAccountId"], "plaid:card");
    assert_eq!(imported["settings"]["hiddenBenefitIds"][0], "resy");

    let view = |snapshot: &Value| {
        snapshot["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id.as_str())
            .cloned()
            .unwrap()
    };
    storage
        .save_annotations(
            BTreeMap::from([(
                id.clone(),
                Annotation {
                    category: Some("Manual".into()),
                    reviewed: Some(true),
                    ..Default::default()
                },
            )]),
            false,
        )
        .unwrap();
    let manual = storage.snapshot().unwrap();
    assert_eq!(view(&manual)["category"], "Manual");
    assert_eq!(view(&manual)["categorySource"], "manual");
    assert_eq!(view(&manual)["providerCategory"], transaction.category);
    assert_eq!(view(&manual)["reviewed"], true);

    drop(storage);
    let mut reopened = Storage::new(directory.clone()).unwrap();
    assert_eq!(
        reopened.data.settings.spending_account_id.as_deref(),
        Some("plaid:card")
    );
    let erased = reopened.erase().unwrap();
    assert!(erased["settings"]["spendingAccountId"].is_null());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn refresh_is_atomic_validated_and_single_use() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let original = json(&storage.data.snapshot);
    let cache: PlaidCache = serde_json::from_value(
        serde_json::json!({"items": {"item": {"cursor": "next", "transactions": []}}}),
    )
    .unwrap();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            cache,
            ProviderDataCache::default(),
            vec!["Offline".into()],
        )
        .unwrap();
    assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
    assert!(storage
        .commit_value(&id, serde_json::json!({"netWorth": 999}))
        .is_err());
    let mut invalid = original.clone();
    invalid["netWorth"] = 999.into();
    assert!(storage.commit_value(&id, invalid).is_err());
    let result = storage.commit_value(&id, original.clone()).unwrap();
    assert_eq!(result["syncWarnings"][0], "Offline");
    assert_eq!(storage.database.load().unwrap().unwrap().revision, 1);
    assert_eq!(
        serde_json::to_value(&storage.database.load().unwrap().unwrap().plaid_cache).unwrap()
            ["items"]["item"]["cursor"],
        "next"
    );
    assert!(storage.commit_value(&id, original).is_err());
    assert!(storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![]
        )
        .is_err());
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn superseded_refresh_and_unknown_versions_are_rejected() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let old = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    assert!(storage
        .commit_value(&old, storage.data.snapshot.clone())
        .is_err());
    storage.data.schema_version = 2;
    storage.database.replace(&storage.data).unwrap();
    drop(storage);
    let recovery = Storage::new(directory.clone()).unwrap();
    assert!(recovery.recovery.is_some());
    assert!(recovery.ensure_writable().is_err());
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn migrates_legacy_data_then_removes_plaintext_sources() {
    let directory = directory();
    fs::create_dir_all(&directory).unwrap();
    let legacy = directory.join("finance-snapshot.json");
    let mut snapshot: Value =
        serde_json::from_str(include_str!("../../../../src/data/empty.json")).unwrap();
    snapshot["accounts"][0]["balanceAsOf"] = "2026-09-05".into();
    snapshot["syncWarnings"] = serde_json::json!(["raw account message"]);
    snapshot["providerStatus"] = serde_json::json!({
        "plaid": {"updatedAt": null, "error": "raw account message"}
    });
    fs::write(&legacy, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    fs::write(directory.join("news.sqlite3"), b"synthetic cache").unwrap();
    let storage = Storage::new(directory.clone()).unwrap();
    snapshot["syncWarnings"] = serde_json::json!([]);
    snapshot["providerStatus"]["plaid"]["error"] = Value::Null;
    let expected: Snapshot = crate::finance_contract::decode(snapshot).unwrap();
    assert_eq!(json(&storage.data.snapshot), json(&expected));
    assert!(!legacy.exists());
    assert!(storage.backup_path().exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&storage.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(storage.backup_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.join("news.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn inspecting_an_open_primary_uses_a_consistent_sqlite_backup() {
    let directory = directory();
    let storage = Storage::new(directory.clone()).unwrap();
    let info = storage.inspect_backup(&storage.path, None).unwrap();
    assert_eq!(info.revision, storage.data.revision);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_legacy_state_opens_recovery_without_creating_a_database() {
    let directory = directory();
    fs::create_dir_all(&directory).unwrap();
    let legacy = directory.join("finance-state.json");
    let mut state = empty_state().unwrap();
    state.snapshot.net_worth = 999.0;
    fs::write(&legacy, serde_json::to_vec(&state).unwrap()).unwrap();

    let storage = Storage::new(directory.clone()).unwrap();

    assert!(storage.recovery.is_some());
    assert!(!storage.path.exists());
    assert!(legacy.exists());
    assert!(storage.ensure_writable().is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_write_preserves_disk_memory_and_pending_refresh() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    let mut invalid = json(&storage.data.snapshot);
    invalid["possibleDuplicateAccounts"] = serde_json::json!([
        {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "one"},
        {"plaidAccountId": "same", "snaptradeAccountId": "one", "description": "duplicate"}
    ]);
    assert!(storage.commit_value(&id, invalid).is_err());
    assert_eq!(storage.data.revision, 0);
    assert_eq!(storage.database.load().unwrap().unwrap().revision, 0);
    storage
        .commit_value(&id, storage.data.snapshot.clone())
        .unwrap();
    assert_eq!(storage.data.revision, 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn annotations_survive_refresh_and_legacy_import_never_overwrites_edits() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    let edits = BTreeMap::from([(
        "transaction".into(),
        Annotation {
            category: Some("Travel".into()),
            reviewed: Some(true),
            benefit_confirmed: Some(true),
            ..Default::default()
        },
    )]);
    storage.save_annotations(edits, false).unwrap();
    let legacy = BTreeMap::from([(
        "transaction".into(),
        Annotation {
            category: Some("Other".into()),
            reviewed: Some(false),
            benefit_confirmed: Some(false),
            ..Default::default()
        },
    )]);
    storage.save_annotations(legacy, true).unwrap();
    storage
        .commit_value(&id, storage.data.snapshot.clone())
        .unwrap();
    drop(storage);
    let restored = Storage::new(directory.clone()).unwrap();
    assert_eq!(
        restored.data.annotations["transaction"].category.as_deref(),
        Some("Travel")
    );
    assert_eq!(
        restored.data.annotations["transaction"].reviewed,
        Some(true)
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn expired_refresh_does_not_commit() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            &chrono::Utc::now().to_rfc3339(),
            0,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    storage.pending.as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
    assert!(storage
        .commit_value(&id, storage.data.snapshot.clone())
        .is_err());
    assert_eq!(storage.data.revision, 0);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn only_one_app_instance_can_own_the_store() {
    let directory = directory();
    let storage = Storage::new(directory.clone()).unwrap();
    assert!(Storage::new(directory.clone()).is_err());
    drop(storage);
    assert!(Storage::new(directory.clone()).is_ok());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_repeated_read_reuses_the_derived_view_and_every_edit_is_visible_at_once() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let first = storage.snapshot().unwrap();
    // Nothing changed, so the second read is the same derived view, timestamp included.
    assert_eq!(storage.snapshot().unwrap(), first);
    assert!(first["settings"]["backgroundRefresh"] == false);

    let mut settings = storage.data.settings.clone();
    settings.background_refresh = true;
    let saved = storage.save_settings(settings, false).unwrap();
    assert_eq!(saved["settings"]["backgroundRefresh"], true);
    assert_eq!(
        storage.snapshot().unwrap()["settings"]["backgroundRefresh"],
        true
    );

    storage.erase().unwrap();
    assert_eq!(
        storage.snapshot().unwrap()["settings"]["backgroundRefresh"],
        false
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn portable_backup_round_trips_edits_and_invalid_restore_preserves_state() {
    use crate::settings::FinanceSettings;

    let root = directory();
    let directory = root.join("store");
    let backup = root.join("portable.sqlite3");
    let mut storage = Storage::new(directory.clone()).unwrap();
    let named = |name: &str| FinanceSettings {
        account_names: BTreeMap::from([("plaid:one".to_owned(), name.to_owned())]),
        ..Default::default()
    };
    storage.save_settings(named("Everyday"), false).unwrap();
    storage
        .save_annotations(
            BTreeMap::from([(
                "synthetic".into(),
                Annotation {
                    reviewed: Some(true),
                    ..Default::default()
                },
            )]),
            false,
        )
        .unwrap();
    storage.export_backup(&backup, None, None).unwrap();
    assert!(storage.export_backup(&backup, None, None).is_err());
    let original_backup = fs::read(&backup).unwrap();
    storage
        .save_settings(FinanceSettings::default(), false)
        .unwrap();
    storage.import_backup(&backup, None, None).unwrap();
    assert_eq!(storage.data.settings, named("Everyday"));
    assert_eq!(storage.data.annotations["synthetic"].reviewed, Some(true));
    assert_eq!(fs::read(&backup).unwrap(), original_backup);
    let invalid = root.join("invalid.sqlite3");
    fs::write(&invalid, b"not a database").unwrap();
    let revision = storage.data.revision;
    assert!(storage.import_backup(&invalid, None, None).is_err());
    assert_eq!(storage.data.revision, revision);
    assert_eq!(storage.data.settings, named("Everyday"));
    // Writes after restore must target the installed database, not the staging filename.
    storage.save_settings(named("Travel"), false).unwrap();
    drop(storage);
    let reopened = Storage::new(directory).unwrap();
    assert_eq!(reopened.data.settings, named("Travel"));
    assert!(fs::read_dir(root.join("store")).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("retained-")));
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

mod annotations;
mod documents;
mod sessions;
mod statements;
