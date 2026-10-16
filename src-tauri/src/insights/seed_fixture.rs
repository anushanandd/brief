use serde_json::{json, Value};

use super::*;
use crate::insights::calendar::NamedZone;

/// The browser preview has no native runtime, so it reads insights computed here from
/// the same synthetic seed. Regenerate with `BRIEF_UPDATE_CONTRACT_FIXTURE=1`.
#[test]
fn browser_seed_insights_match_native_insights() {
    let seed: Value = serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap();
    let mut snapshot = seed.clone();
    let spending_account_id = seed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["type"] == "credit")
        .and_then(|account| account["id"].as_str())
        .unwrap()
        .to_owned();
    let settings = FinanceSettings {
        spending_account_id: Some(spending_account_id.clone()),
        ..Default::default()
    };
    // The preview is pinned to the seed's own save time so it never goes stale.
    let now = DateTime::parse_from_rfc3339(seed["updatedAt"].as_str().unwrap())
        .unwrap()
        .to_utc();
    attach(&mut snapshot, &settings, now, &NamedZone::Utc).unwrap();
    let fixture = json!({
        "spendingAccountId": spending_account_id,
        "insights": snapshot["insights"],
        // The investment summary needs prices the preview doesn't have; this one is made
        // from synthetic closes by the same code (`crate::summary::preview`).
        "summary": crate::summary::preview::summary(),
        "recurringIds": snapshot["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|transaction| (transaction["id"].as_str().unwrap().to_owned(), transaction["recurringId"].clone()))
            .collect::<serde_json::Map<_, _>>(),
    });
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/data/seed-insights.json");
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap()),
        )
        .unwrap();
    } else {
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved, fixture,
            "Regenerate the browser seed insights with BRIEF_UPDATE_CONTRACT_FIXTURE=1"
        );
    }
}
