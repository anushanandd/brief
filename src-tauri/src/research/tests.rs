use chrono::TimeZone;

use super::*;

#[test]
fn sources_are_rechecked_monthly_and_changes_stay_flagged() {
    let first = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
    assert!(due(None, first));
    let check = checked(None, "https://bank.example", "Fee  is $55", first);
    assert_eq!(check.changed_at, None);
    assert!(!due(Some(&check), first + chrono::Duration::days(29)));
    let later = first + chrono::Duration::days(30);
    assert!(due(Some(&check), later));
    // Spacing and case alone aren't a change.
    let same = checked(Some(&check), "https://bank.example", "fee is $55", later);
    assert_eq!(same.changed_at, None);
    let changed = checked(Some(&same), "https://bank.example", "Fee is $65", later);
    assert_eq!(changed.changed_at, Some(later.to_rfc3339()));
    let after = later + chrono::Duration::days(30);
    let kept = checked(Some(&changed), "https://bank.example", "Fee is $65", after);
    assert_eq!(kept.changed_at, changed.changed_at);
}

#[test]
fn entries_record_what_came_back_or_why_not() {
    let now = Utc.with_ymd_and_hms(2026, 10, 17, 9, 0, 0).unwrap();
    let mut entry = Entry::begin(
        EntryKind::Search,
        "Example query".into(),
        Some("card:x".into()),
        now,
    );
    assert_eq!(entry.status, Status::Running);
    assert_eq!(entry.id.len(), 16);
    entry.finish(Err("Exa rejected the API key".into()), now);
    assert_eq!(entry.status, Status::Failed);
    assert_eq!(entry.error.as_deref(), Some("Exa rejected the API key"));
}
