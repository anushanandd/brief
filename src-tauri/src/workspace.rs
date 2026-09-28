use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;

#[cfg(test)]
use crate::storage::Annotation;

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
// Legacy decisions remain readable so existing stores and backups are not invalidated.
pub struct Workspace {
    pub theses: BTreeMap<String, String>,
    pub rules: BTreeMap<String, String>,
    pub subscriptions: BTreeMap<String, String>,
    pub dismissed: BTreeMap<String, bool>,
    pub targets: BTreeMap<String, f64>,
    pub spending_accounts: Option<Vec<String>>,
    pub preferences: BTreeMap<String, String>,
}

pub const PREFERENCE_KEYS: &[&str] = &[
    "brief:spending-account-id",
    "brief:hidden-platinum-benefits",
    "brief.externalLogosEnabled",
    "brief.accountStartDates",
    "brief.defaultGraphWindow",
    "brief.chartAccountPreferences",
    "brief.marketUpdateIntervalSeconds",
    "brief.accountDisplayNames",
    "brief.holdingChartRange",
];

impl Workspace {
    pub fn validate(&self) -> Result<(), String> {
        if self.theses.len() > 1000
            || self.theses.iter().any(|(ticker, thesis)| {
                ticker.is_empty()
                    || ticker.len() > 32
                    || ticker.trim() != ticker
                    || ticker.chars().any(char::is_control)
                    || thesis.trim().is_empty()
                    || thesis.chars().count() > 1000
            })
            || self.rules.len() > 1000
            || self.subscriptions.len() > 1000
            || self.dismissed.len() > 5000
            || self.targets.len() > 1000
            || self.preferences.len() > PREFERENCE_KEYS.len()
            || self.spending_accounts.as_ref().is_some_and(|ids| {
                ids.len() > 500 || ids.iter().any(|id| id.is_empty() || id.len() > 220)
            })
            || self.rules.iter().any(|(key, value)| {
                key.trim().is_empty()
                    || key.len() > 500
                    || value.trim().is_empty()
                    || value.len() > 100
            })
            || self.subscriptions.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 1000
                    || !["confirmed", "ignored"].contains(&value.as_str())
            })
            || self
                .dismissed
                .keys()
                .any(|key| key.is_empty() || key.len() > 1500)
            || self.targets.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 24
                    || !value.is_finite()
                    || !(0.0..=100.0).contains(value)
            })
            || self.targets.values().sum::<f64>() > 100.00001
            || self.preferences.iter().any(|(key, value)| {
                !PREFERENCE_KEYS.contains(&key.as_str()) || value.len() > 100_000
            })
        {
            return Err("Invalid workspace settings".into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn annotations(
        &self,
        snapshot: &Value,
        saved: &BTreeMap<String, Annotation>,
    ) -> BTreeMap<String, Annotation> {
        let mut result = saved.clone();
        for transaction in snapshot["transactions"].as_array().into_iter().flatten() {
            let Some(id) = transaction["id"].as_str() else {
                continue;
            };
            let merchant = transaction["merchant"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if let Some(category) = self.rules.get(&merchant) {
                result
                    .entry(id.into())
                    .or_default()
                    .category
                    .get_or_insert_with(|| category.clone());
            }
        }
        result
    }
}

// Native save/open panels without another runtime dependency. No paths or financial
// content are interpolated into AppleScript; the user chooses the destination.
pub fn choose_csv_destination(filename: &str) -> Result<Option<std::path::PathBuf>, String> {
    if filename.len() > 240
        || !filename.ends_with(".csv")
        || filename.starts_with('.')
        || filename
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err("Invalid CSV filename".into());
    }
    let script = "on run argv\nreturn POSIX path of (choose file name with prompt \"Export account activity CSV\" default name (item 1 of argv))\nend run";
    let result = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .arg(filename)
        .output()
        .map_err(|_| "Could not open the macOS file picker")?;
    if !result.status.success() {
        if String::from_utf8_lossy(&result.stderr).contains("(-128)") {
            return Ok(None);
        }
        return Err("Could not complete file selection".into());
    }
    let path = String::from_utf8(result.stdout).map_err(|_| "Invalid selected path")?;
    Ok(Some(std::path::PathBuf::from(path.trim_end_matches('\n'))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use serde_json::json;
    use std::fs;

    #[test]
    fn csv_picker_rejects_invalid_names_before_opening() {
        for name in [
            "../other.csv",
            "folder/file.csv",
            "bad\nname.csv",
            "not-a-csv.txt",
        ] {
            assert!(choose_csv_destination(name).is_err());
        }
    }

    #[test]
    fn rules_preserve_individual_overrides_and_validate_targets_and_preferences() {
        let mut workspace = Workspace::default();
        workspace
            .rules
            .insert("example shop".into(), "Travel".into());
        let snapshot = json!({"transactions": [{"id": "a", "merchant": " Example Shop "}, {"id": "b", "merchant": "Example shop"}]});
        let saved = BTreeMap::from([(
            "a".into(),
            Annotation {
                category: Some("Dining".into()),
                reviewed: Some(true),
                ..Default::default()
            },
        )]);
        let annotations = workspace.annotations(&snapshot, &saved);
        assert_eq!(annotations["a"].category.as_deref(), Some("Dining"));
        assert_eq!(annotations["b"].category.as_deref(), Some("Travel"));
        workspace.targets = BTreeMap::from([("A".into(), 60.0), ("B".into(), 50.0)]);
        assert!(workspace.validate().is_err());
        workspace.targets.clear();
        workspace
            .preferences
            .insert("access_token".into(), "synthetic".into());
        assert!(workspace.validate().is_err());
    }

    #[test]
    fn portable_backup_round_trips_decisions_and_invalid_restore_preserves_state() {
        let root =
            std::env::temp_dir().join(format!("brief-portable-test-{}", uuid::Uuid::new_v4()));
        let directory = root.join("store");
        let backup = root.join("portable.sqlite3");
        let mut storage = Storage::new(directory.clone()).unwrap();
        let mut workspace = Workspace::default();
        workspace.rules.insert("example".into(), "Dining".into());
        workspace
            .theses
            .insert("TEST".into(), "Synthetic adoption hypothesis.".into());
        workspace
            .subscriptions
            .insert("cash|example".into(), "confirmed".into());
        workspace
            .preferences
            .insert("brief.holdingChartRange".into(), "86400".into());
        storage.save_workspace(workspace.clone()).unwrap();
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
        storage.export_backup(&backup).unwrap();
        assert!(storage.export_backup(&backup).is_err());
        let original_backup = fs::read(&backup).unwrap();
        storage.save_workspace(Workspace::default()).unwrap();
        let restored = storage.import_backup(&backup).unwrap();
        assert!(restored == workspace);
        assert_eq!(storage.data.annotations["synthetic"].reviewed, Some(true));
        assert_eq!(fs::read(&backup).unwrap(), original_backup);
        let invalid = root.join("invalid.sqlite3");
        fs::write(&invalid, b"not a database").unwrap();
        let revision = storage.data.revision;
        assert!(storage.import_backup(&invalid).is_err());
        assert_eq!(storage.data.revision, revision);
        assert!(storage.data.workspace == workspace);
        // Writes after restore must target the installed database, not the staging filename.
        workspace.dismissed.insert("synthetic-notice".into(), true);
        storage.save_workspace(workspace.clone()).unwrap();
        drop(storage);
        let reopened = Storage::new(directory).unwrap();
        assert!(reopened.data.workspace == workspace);
        assert!(fs::read_dir(root.join("store")).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("retained-")));
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn review_and_rule_commit_together_and_failed_validation_changes_neither() {
        let directory =
            std::env::temp_dir().join(format!("brief-review-test-{}", uuid::Uuid::new_v4()));
        let mut storage = Storage::new(directory.clone()).unwrap();
        storage.data.snapshot["transactions"] = json!([{"id": "synthetic", "merchant": "Example Shop", "category": "Other", "date": "2026-09-01", "amount": -10, "pending": false, "account": "Example"}]);
        let review = Annotation {
            category: Some("Dining".into()),
            reviewed: Some(true),
            benefit_confirmed: Some(false),
        };
        storage
            .review_transaction("synthetic", Some(review.clone()), true)
            .unwrap();
        assert_eq!(storage.data.workspace.rules["example shop"], "Dining");
        assert!(storage.data.annotations["synthetic"] == review);
        let invalid = Annotation {
            category: Some("".into()),
            ..review
        };
        assert!(storage
            .review_transaction("synthetic", Some(invalid), true)
            .is_err());
        assert_eq!(storage.data.workspace.rules["example shop"], "Dining");
        assert_eq!(
            storage.data.annotations["synthetic"].category.as_deref(),
            Some("Dining")
        );
        let connection =
            rusqlite::Connection::open(directory.join("finance-state.sqlite3")).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_workspace BEFORE INSERT ON workspace BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;").unwrap();
        assert!(storage
            .review_transaction(
                "synthetic",
                Some(Annotation {
                    category: Some("Travel".into()),
                    ..Default::default()
                }),
                true
            )
            .is_err());
        assert_eq!(storage.data.workspace.rules["example shop"], "Dining");
        let persisted: String = connection
            .query_row(
                "SELECT category FROM annotations WHERE transaction_id = 'synthetic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, "Dining");
        connection
            .execute_batch("DROP TRIGGER fail_workspace")
            .unwrap();
        drop(connection);
        storage
            .review_transaction("synthetic", None, false)
            .unwrap();
        assert!(!storage.data.annotations.contains_key("synthetic"));
        assert_eq!(storage.data.workspace.rules["example shop"], "Dining");
        assert_eq!(
            storage.snapshot().unwrap()["transactions"][0]["category"],
            "Other"
        );
        drop(storage);
        fs::remove_dir_all(directory).unwrap();
    }
}
