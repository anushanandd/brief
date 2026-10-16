//! Closed market sessions saved by date.
use super::*;

use crate::summary::Closed;

/// A synthetic filing event as saved.
fn event(id: &str, session: &str) -> crate::summary::Event {
    serde_json::from_value(serde_json::json!({
        "id": id, "family": "disclosure", "kind": "8-K", "subtype": "2.02", "symbol": "TEST",
        "accountId": null, "session": session, "at": null, "precision": "exact", "dates": {},
        "provenance": "sourced", "status": "confirmed", "source": null, "quantities": null,
        "effect": null, "z": null, "ingestedAt": "2026-10-06T21:00:00+00:00",
    }))
    .unwrap()
}

/// A synthetic session record as saved: one holding, one fund group.
fn session_record(session: &str, method: u32) -> crate::summary::SessionRecord {
    serde_json::from_value(serde_json::json!({
        "session": session, "method": method, "computedAt": "2026-10-06T21:00:00+00:00",
        "marketPct": -0.9, "marketZ": -1.1,
        "holdings": [{
            "ticker": "TEST", "changePct": -2.0, "marketPct": -1.1, "groupPct": -0.6,
            "ownPct": -0.3, "ownZ": -0.2, "sigma": 1.5, "change": -20.0, "market": -11.0,
            "group": -6.0, "own": -3.0,
            "fit": { "marketBeta": 1.2, "fund": "XLK", "groupBeta": 0.8, "rSquared": 0.5,
                     "window": 250, "shrunk": false },
        }],
        "groups": [{ "key": "XLK", "fund": "XLK", "name": "Technology", "members": ["TEST"],
                     "beyondPct": -0.7, "z": -1.3, "change": -6.0 }],
        "calibration": { "modeled": 1, "fundGroups": 1, "looseGroups": 0, "notable": 0 },
    }))
    .unwrap()
}

#[test]
fn closed_sessions_persist_by_date_are_replaced_only_by_a_new_method_and_are_erased() {
    let directory = directory();
    let mut storage = Storage::new(directory.clone()).unwrap();
    let day = |value: &str| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
    storage
        .save_market_sessions(&Closed {
            sessions: vec![
                session_record("2026-10-05", 1),
                session_record("2026-10-06", 1),
            ],
            events: vec![
                event("filing:1", "2026-10-06"),
                event("filing:2", "2026-10-09"),
            ],
        })
        .unwrap();
    // An event is saved only with its session.
    assert_eq!(
        storage
            .database
            .market_events(day("2026-10-01"), day("2026-10-09"))
            .unwrap()
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        ["filing:1"]
    );
    // A period after its start, through its end.
    let saved = storage
        .database
        .market_sessions(day("2026-10-05"), day("2026-10-06"))
        .unwrap();
    assert_eq!(saved, [session_record("2026-10-06", 1)]);
    // Saving results doesn't change the snapshot's revision.
    let revision = storage.data.revision;
    storage
        .save_market_sessions(&Closed {
            sessions: vec![session_record("2026-10-06", 2)],
            ..Closed::default()
        })
        .unwrap();
    // A session saved again replaces its events.
    assert!(storage
        .database
        .market_events(day("2026-10-01"), day("2026-10-09"))
        .unwrap()
        .is_empty());
    assert_eq!(storage.data.revision, revision);
    let replaced = storage
        .database
        .market_sessions(day("2026-10-01"), day("2026-10-09"))
        .unwrap();
    assert_eq!(replaced.len(), 2);
    assert_eq!(replaced[1].method, 2);
    // They survive reopening, and the calibration reads them.
    drop(storage);
    let mut storage = Storage::new(directory.clone()).unwrap();
    let calibration = storage.calibration().unwrap();
    assert_eq!(calibration.sessions, 2);
    assert_eq!(calibration.fund_groups, 1.0);
    // Erasure clears them with the rest of the finance data.
    storage.erase().unwrap();
    assert!(storage
        .database
        .market_sessions(day("2026-10-01"), day("2026-10-09"))
        .unwrap()
        .is_empty());
    assert!(storage
        .database
        .market_events(day("2026-10-01"), day("2026-10-09"))
        .unwrap()
        .is_empty());
    fs::remove_dir_all(directory).unwrap();
}
