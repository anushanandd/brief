//! Dated and pre-migration copies, and the recovery that offers them, through `Storage`.
use super::*;
use crate::database::{
    tests::{database_at, user_version},
    DATABASE_SCHEMA_VERSION,
};
use crate::storage::generations as copies;

fn november(day: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap_or_default()
}

/// Commits one refresh whose snapshot says it was updated at `updated_at`.
fn commit(storage: &mut Storage, updated_at: &str) {
    let mut snapshot = json(&storage.data.snapshot);
    snapshot["updatedAt"] = updated_at.into();
    let id = storage
        .stage(
            &uuid::Uuid::new_v4().to_string(),
            updated_at,
            storage.data.revision,
            PlaidCache::default(),
            ProviderDataCache::default(),
            vec![],
        )
        .unwrap();
    storage.commit_value(&id, snapshot).unwrap();
}

#[test]
fn a_crash_mid_write_opens_normally_not_in_recovery() {
    let directory = directory();
    let path = directory.join("finance-state.sqlite3");
    let mut storage = Storage::new(directory.clone()).unwrap();
    commit(&mut storage, "2026-11-10T12:00:00Z");
    let revision = storage.data.revision;
    drop(storage);
    let crashed = directory.join("crashed.sqlite3");
    crate::database::tests::crashed_mid_write(&path, &crashed);
    fs::rename(&crashed, &path).unwrap();
    fs::rename(
        directory.join("crashed.sqlite3-journal"),
        directory.join("finance-state.sqlite3-journal"),
    )
    .unwrap();

    let storage = Storage::new(directory.clone()).unwrap();
    assert!(storage.recovery.is_none(), "{:?}", storage.recovery);
    assert_eq!(storage.data.revision, revision);
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_older_database_that_cant_be_copied_shows_as_it_is_not_as_unreadable() {
    let directory = directory();
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("finance-state.sqlite3");
    let mut state = empty_state().unwrap();
    state.revision = 4;
    database_at(&path, 20, &state);
    // A file where the copies' folder belongs, so no copy can be written.
    fs::write(directory.join(copies::DIRECTORY), b"").unwrap();

    let storage = Storage::new(directory.clone()).unwrap();
    let recovery = storage.recovery.clone().unwrap();
    assert!(
        recovery.message.contains("couldn't keep a copy"),
        "{}",
        recovery.message
    );
    assert!(!recovery.message.contains("could not be read"));
    assert!(recovery.can_restore);
    assert_eq!(storage.data.revision, 4);
    drop(storage);
    assert_eq!(user_version(&path), 20, "not migrated without its copy");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn older_databases_are_copied_at_their_version_before_they_migrate_and_only_once() {
    for version in [10, 15, 20] {
        let directory = directory();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("finance-state.sqlite3");
        let mut state = empty_state().unwrap();
        state.revision = 4;
        database_at(&path, version, &state);

        let storage = Storage::new(directory.clone()).unwrap();
        assert!(storage.recovery.is_none(), "from {version}");
        assert_eq!(storage.data.revision, 4);
        assert_eq!(user_version(&path), DATABASE_SCHEMA_VERSION);
        let kept = directory
            .join(copies::DIRECTORY)
            .join(format!("finance-state.pre-v{version}.sqlite3"));
        assert_eq!(
            user_version(&kept),
            version,
            "the copy stays at its version"
        );
        Database::validate_backup(&kept).unwrap();
        drop(storage);

        // A second launch finds the database current: nothing is copied or written.
        let bytes = fs::read(&path).unwrap();
        let storage = Storage::new(directory.clone()).unwrap();
        assert_eq!(storage.data.revision, 4);
        drop(storage);
        assert_eq!(fs::read(&path).unwrap(), bytes, "reopening {version} wrote");
        assert_eq!(
            fs::read_dir(directory.join(copies::DIRECTORY))
                .unwrap()
                .count(),
            1
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn a_newer_database_is_left_unchanged_and_recovery_offers_the_newest_copy_this_build_reads() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.today = || november(9);
    commit(&mut storage, "2026-11-09T12:00:00Z");
    storage.today = || november(10);
    commit(&mut storage, "2026-11-10T12:00:00Z");
    let path = storage.path.clone();
    let rolling = storage.backup_path();
    drop(storage);
    assert!(directory
        .join(copies::DIRECTORY)
        .join("finance-state.2026-11-10.sqlite3")
        .exists());

    // A newer build migrated the database and, on its next write, the rolling backup.
    for file in [&path, &rolling] {
        rusqlite::Connection::open(file)
            .unwrap()
            .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION + 1)
            .unwrap();
    }
    let newer = fs::read(&path).unwrap();
    let mut storage = Storage::new(directory.clone()).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        newer,
        "the newer database is not touched"
    );
    let recovery = storage.recovery.clone().unwrap();
    assert!(recovery.can_restore);
    assert!(
        recovery.message.contains("the daily copy from 2026-11-10"),
        "{}",
        recovery.message
    );
    // That copy holds the state the day began with: the first refresh.
    assert_eq!(storage.data.revision, 1);
    assert_eq!(storage.data.snapshot.updated_at, "2026-11-09T12:00:00Z");
    // Reading the copy didn't migrate or change it.
    assert_eq!(
        user_version(
            &directory
                .join(copies::DIRECTORY)
                .join("finance-state.2026-11-10.sqlite3")
        ),
        DATABASE_SCHEMA_VERSION
    );

    storage.recover(true).unwrap();
    assert!(storage.recovery.is_none());
    assert_eq!(load_database(&path).unwrap().1.revision, 1);
    // The newer original is kept beside it.
    assert!(fs::read_dir(&directory).unwrap().flatten().any(|entry| {
        entry.file_name().to_string_lossy().contains("retained-")
            && fs::read(entry.path()).unwrap() == newer
    }));
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn erasure_removes_every_copy_and_the_failure_log() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    storage.today = || november(10);
    commit(&mut storage, "2026-11-10T12:00:00Z");
    let release = directory
        .join(copies::RELEASES)
        .join("pre-release-abc1234-20261110-090000");
    fs::create_dir_all(&release).unwrap();
    fs::write(release.join("finance-state.sqlite3"), b"copy").unwrap();
    fs::write(directory.join(crate::panic_log::FILE), b"a panic\n").unwrap();

    storage.erase().unwrap();
    assert!(!directory.join(copies::DIRECTORY).exists());
    assert!(!directory.join(copies::RELEASES).exists());
    assert!(!directory.join(crate::panic_log::FILE).exists());
    drop(storage);
    fs::remove_dir_all(directory).unwrap();
}
