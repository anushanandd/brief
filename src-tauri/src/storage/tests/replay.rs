//! The change log replays to the saved state (Gate A), for a synthetic person's life.
use super::*;
use crate::{
    database::Database,
    storage::change::{self, Change, Outcome, ReplayError},
    synthetic::Life,
};

/// Fetches, projects, and commits one refresh through the same path the app takes.
fn refresh(storage: &mut Storage, refresh: change::Refresh) {
    let id = uuid::Uuid::new_v4().to_string();
    let revision = storage.data.revision;
    storage
        .stage_refresh(
            &id,
            &refresh.committed_at.to_rfc3339(),
            revision,
            Staged {
                refresh: refresh.clone(),
                diagnostics: SyncDiagnostics::default(),
            },
        )
        .unwrap();
    let snapshot = change::project_refresh(
        &storage.data.snapshot,
        &storage.data.statements,
        &storage.data.account_links,
        &refresh,
    )
    .unwrap();
    if let Err(error) = storage.commit(&id, snapshot) {
        panic!("{error}");
    }
}

/// What a check of the log finds now, through its own connection.
fn check(storage: &Storage) -> change::LogCheck {
    change::check(
        &Database::open_read_only(storage.database_path()).unwrap(),
        chrono::Utc::now(),
    )
}

fn assert_replays(storage: &Storage) {
    let check = check(storage);
    assert_eq!(check.outcome, Outcome::Matches, "{:?}", check.detail);
    let (replayed, _) = change::replay(&storage.database).unwrap();
    assert_eq!(
        change::state_hash(&replayed.unwrap()).unwrap(),
        change::state_hash(&storage.data).unwrap()
    );
}

const CHECKING_STATEMENT: &str = "Account Activity for Everyday Checking -0000\n\
    Date,Description,Amount\n\
    05/20/2026,EXAMPLE EMPLOYER PAYROLL,2450.00\n\
    05/02/2026,EXAMPLE PROPERTY RENT,-1650.00\n";

#[test]
fn a_synthetic_life_replays_to_the_saved_state() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(7);
    for (index, day) in life.refresh_days().into_iter().enumerate() {
        refresh(&mut storage, life.refresh(day));
        match index {
            2 => {
                let purchase = storage
                    .data
                    .snapshot
                    .transactions
                    .iter()
                    .find(|transaction| transaction.amount < 0.0 && !transaction.pending)
                    .unwrap()
                    .id
                    .clone();
                storage
                    .save_annotations(
                        BTreeMap::from([(
                            purchase,
                            Annotation {
                                category: Some("Dining".into()),
                                note: Some("Lunch with a colleague".into()),
                                tags: Some(vec!["work".into()]),
                                ..Default::default()
                            },
                        )]),
                        false,
                    )
                    .unwrap();
            }
            4 => {
                let mut settings = storage.data.settings.clone();
                settings
                    .account_names
                    .insert("plaid:checking".into(), "Bills".into());
                storage.save_settings(settings, false).unwrap();
            }
            6 => {
                let (_, draft) = storage
                    .preview_statement(
                        "checking.csv".into(),
                        CHECKING_STATEMENT.as_bytes().to_vec(),
                    )
                    .unwrap();
                storage.import_statement(draft, "plaid:checking").unwrap();
            }
            8 => {
                let id = storage.data.annotations.keys().next().unwrap().clone();
                let saved = storage
                    .save_annotations(
                        BTreeMap::from([(
                            id,
                            Annotation {
                                reviewed: Some(true),
                                ..Default::default()
                            },
                        )]),
                        false,
                    )
                    .unwrap();
                // Undo puts the earlier annotation back.
                storage.restore_annotations(saved.previous).unwrap();
            }
            _ => {}
        }
        // Replay checks every changeset's result below; each write records the state saved.
        assert_eq!(
            storage.database.logged_state().unwrap(),
            Some(change::state_hash(&storage.data).unwrap())
        );
    }
    assert_replays(&storage);
    let summary = storage.log_summary().unwrap();
    assert_eq!(summary.checkpoints, 1);
    assert_eq!(summary.changesets as usize, life.refresh_days().len() + 6);

    // The statement's rows and the edits came through, so the replay covered them.
    assert_eq!(storage.data.statements.len(), 1);
    assert!(storage
        .data
        .snapshot
        .transactions
        .iter()
        .any(|transaction| transaction.id.starts_with("statement:")));
    assert_eq!(storage.data.annotations.len(), 1);

    // Reopened, the saved tables are the state the log records.
    let saved = change::state_hash(&storage.data).unwrap();
    drop(storage);
    let storage = Storage::new(directory.clone()).unwrap();
    assert_eq!(change::state_hash(&storage.data).unwrap(), saved);
    assert_eq!(storage.database.logged_state().unwrap(), Some(saved));
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn the_log_keeps_a_pending_charge_and_the_tip_that_replaced_it() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(7);
    for day in life.refresh_days() {
        refresh(&mut storage, life.refresh(day));
    }
    // Every Plaid transaction id any refresh carried.
    let mut seen = BTreeSet::new();
    for changeset in storage.database.since_checkpoint().unwrap() {
        if let Change::Refresh(refresh) = storage.database.change(&changeset).unwrap() {
            for transaction in &refresh.provider_data.plaid.unwrap().transactions {
                seen.insert(transaction["transaction_id"].as_str().unwrap().to_owned());
            }
        }
    }
    let saved = storage
        .data
        .provider_data
        .plaid
        .as_ref()
        .unwrap()
        .transactions
        .iter()
        .map(|transaction| transaction["transaction_id"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    // Plaid replaced each tipped charge, and dropped the hold; the log still has them.
    let replaced = seen.difference(&saved).collect::<Vec<_>>();
    assert!(replaced.iter().any(|id| id.starts_with("hold-")));
    assert!(replaced
        .iter()
        .any(|id| saved.contains(&format!("{id}-posted"))));
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_periodic_checkpoint_keeps_replay_short() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(3);
    refresh(&mut storage, life.refresh(life.refresh_days()[0]));
    let id = storage.data.snapshot.transactions[0].id.clone();
    for edit in 0..change::CHECKPOINT_INTERVAL + 5 {
        storage
            .save_annotations(
                BTreeMap::from([(
                    id.clone(),
                    Annotation {
                        note: Some(format!("Edit {edit}")),
                        ..Default::default()
                    },
                )]),
                false,
            )
            .unwrap();
    }
    let summary = storage.log_summary().unwrap();
    assert_eq!(summary.checkpoints, 2);
    assert!(summary.since_checkpoint < 10);
    assert_replays(&storage);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn replay_names_the_change_whose_result_differs() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(5);
    refresh(&mut storage, life.refresh(life.refresh_days()[0]));
    // A settings change saved with a state the change doesn't produce, as code that changed
    // a result without a new calculation version would.
    let mut settings = storage.data.settings.clone();
    settings
        .account_names
        .insert("plaid:card".into(), "Groceries".into());
    let mut wrong =
        change::apply(&storage.data, &Change::Settings(Box::new(settings.clone()))).unwrap();
    wrong.snapshot.net_worth += 1.0;
    storage
        .database
        .commit(&wrong, &Change::Settings(Box::new(settings)))
        .unwrap();
    let Err(ReplayError::Diverged { kind, .. }) = change::replay(&storage.database) else {
        panic!("replay should find the difference");
    };
    assert_eq!(kind, "settings");
    assert_eq!(check(&storage).outcome, Outcome::Differs);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

/// A few weeks of the synthetic life, saved and closed, with the state the log holds.
fn closed_life(seed: u64) -> (PathBuf, String) {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(seed);
    for day in life.refresh_days().into_iter().take(4) {
        refresh(&mut storage, life.refresh(day));
    }
    let logged = change::state_hash(&storage.data).unwrap();
    drop(storage);
    (directory, logged)
}

fn tables(directory: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(directory.join("finance-state.sqlite3")).unwrap()
}

#[test]
fn the_log_rebuilds_tables_that_stray_from_it() {
    let (directory, logged) = closed_life(11);
    tables(&directory)
        .execute(
            "UPDATE snapshot_scalar SET value_json = '1' WHERE key = 'netWorth'",
            [],
        )
        .unwrap();
    let storage = Storage::new(directory.clone()).unwrap();
    assert!(!storage.is_recovering());
    assert_eq!(change::state_hash(&storage.data).unwrap(), logged);
    // The tables were rebuilt, so the next open needs no replay.
    let saved = storage.database.load().unwrap().unwrap();
    assert_eq!(change::state_hash(&saved).unwrap(), logged);
    assert_replays(&storage);
    drop(storage);

    // Tables lost entirely come back from the log too.
    tables(&directory)
        .execute("DELETE FROM state_meta", [])
        .unwrap();
    let storage = Storage::new(directory.clone()).unwrap();
    assert!(!storage.is_recovering());
    assert_eq!(change::state_hash(&storage.data).unwrap(), logged);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_log_this_code_reads_differently_opens_at_its_replay_and_checkpoints_it() {
    let (directory, _) = closed_life(13);
    // A log whose latest change records a state this code doesn't produce, as after an
    // update that changed how a state is saved or computed.
    let mut storage = Storage::new(directory.clone()).unwrap();
    let settings = storage.data.settings.clone();
    let replayed =
        change::apply(&storage.data, &Change::Settings(Box::new(settings.clone()))).unwrap();
    let mut wrong = replayed.clone();
    wrong.snapshot.net_worth += 1.0;
    storage
        .database
        .commit(&wrong, &Change::Settings(Box::new(settings)))
        .unwrap();
    drop(storage);
    tables(&directory)
        .execute(
            "UPDATE snapshot_scalar SET value_json = '1' WHERE key = 'netWorth'",
            [],
        )
        .unwrap();
    let storage = Storage::new(directory.clone()).unwrap();
    assert!(!storage.is_recovering());
    // Neither the strayed tables nor the recorded result: the log's changes under this code.
    assert_eq!(
        change::state_hash(&storage.data).unwrap(),
        change::state_hash(&replayed).unwrap()
    );
    // The new checkpoint makes the log and tables agree, so later opens need no replay.
    assert_replays(&storage);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn erasure_starts_the_log_over_from_the_erased_state() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(9);
    for day in life.refresh_days().into_iter().take(3) {
        refresh(&mut storage, life.refresh(day));
    }
    storage.erase().unwrap();
    let summary = storage.log_summary().unwrap();
    assert_eq!((summary.changesets, summary.checkpoints), (1, 1));
    assert_replays(&storage);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_change_from_a_newer_version_is_refused() {
    let directory = directory();
    let storage = Storage::new(directory.clone()).unwrap();
    let mut changeset = storage.database.since_checkpoint().unwrap().remove(0);
    changeset.format += 1;
    assert_eq!(
        storage.database.change(&changeset).err().unwrap(),
        "The change log was written by a newer version of Brief"
    );
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}
