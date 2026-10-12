//! Files from builds before the finance database, read once into it and then removed, and
//! the owner-only permissions on the local data directory and its SQLite files.
use std::{collections::BTreeMap, fs, path::Path};

use serde::de::DeserializeOwned;

use super::FinanceState;

pub(super) const LEGACY_FINANCE_FILES: [&str; 5] = [
    "finance-state.json",
    "finance-state.backup.json",
    "finance-snapshot.json",
    "plaid-cache.json",
    "provider-data.json",
];

pub(super) fn protect_directory(directory: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Could not protect local financial data: {error}"))?;
    }
    for name in [
        "finance-state.sqlite3",
        "finance-state.backup.sqlite3",
        "market-prices.sqlite3",
        "news.sqlite3",
        "startup-market.sqlite3",
    ] {
        crate::database::secure_file(&directory.join(name))?;
    }
    Ok(())
}

pub(super) fn remove_migrated_legacy_files(directory: &Path) {
    for name in LEGACY_FINANCE_FILES {
        let _ = fs::remove_file(directory.join(name));
    }
}

pub(super) fn clear_legacy_diagnostics(data: &mut FinanceState) {
    data.snapshot.sync_warnings.clear();
    for status in data
        .snapshot
        .provider_status
        .iter_mut()
        .flat_map(|statuses| statuses.values_mut())
    {
        status.error = None;
    }
    data.plaid_cache.clear_legacy_diagnostics();
    data.provider_data.clear_legacy_diagnostics();
}

pub(super) fn legacy_state(directory: &Path) -> Result<FinanceState, String> {
    let state_path = directory.join("finance-state.json");
    if state_path.exists() {
        let mut data = read(&state_path)?;
        clear_legacy_diagnostics(&mut data);
        return Ok(data);
    }
    let snapshot_path = directory.join("finance-snapshot.json");
    let mut data = FinanceState {
        schema_version: 1,
        revision: 0,
        snapshot: if snapshot_path.exists() {
            read(&snapshot_path)?
        } else {
            serde_json::from_str(include_str!("../../../src/data/empty.json"))
                .map_err(|error| error.to_string())?
        },
        plaid_cache: read_or_default(&directory.join("plaid-cache.json"))?,
        provider_data: read_or_default(&directory.join("provider-data.json"))?,
        annotations: BTreeMap::new(),
        account_links: BTreeMap::new(),
        settings: Default::default(),
        rules: Vec::new(),
        statements: Vec::new(),
    };
    clear_legacy_diagnostics(&mut data);
    Ok(data)
}

pub(super) fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Invalid local data in {}: {error}", path.display()))
}

pub(super) fn read_or_default<T: Default + DeserializeOwned>(path: &Path) -> Result<T, String> {
    if path.exists() {
        read(path)
    } else {
        Ok(T::default())
    }
}
