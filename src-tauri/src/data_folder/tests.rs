use serde_json::json;

use super::*;
use crate::objects::{Fact, Link, Relation, Tier};

fn object(kind: Kind, key: &str, title: &str, date: Option<&str>, amount: Option<f64>) -> Object {
    Object {
        id: kind.id(key),
        kind,
        title: title.into(),
        subtitle: None,
        date: date.map(Into::into),
        amount,
        text: String::new(),
        record: json!({ "amount": amount, "note": "Lunch, \"team\"" }),
    }
}

fn graph() -> Graph {
    Graph {
        objects: vec![
            object(Kind::Organization, "cafe", "Cafe", None, None),
            object(Kind::Person, "jane", "Jane Example", None, None),
            object(
                Kind::Transaction,
                "t1",
                "Cafe",
                Some("2026-03-04"),
                Some(-12.5),
            ),
        ],
        links: vec![
            Link {
                from: "transaction:t1".into(),
                relation: Relation::Counterparty,
                to: "organization:cafe".into(),
            },
            Link {
                from: "transaction:t1".into(),
                relation: Relation::Person,
                to: "person:jane".into(),
            },
        ],
        facts: vec![Fact {
            subject: "transaction:t1".into(),
            predicate: "amount".into(),
            value: json!(-12.5),
            from: None,
            to: None,
            sources: vec!["plaid".into()],
            grade: None,
            tier: Tier::Observed,
            proof: None,
        }],
    }
}

fn chosen() -> PathBuf {
    let path = std::env::temp_dir().join(format!("brief-folder-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn writes_linked_pages_and_csv_with_every_field() {
    let chosen = chosen();
    write(&chosen, &graph(), false, "2026-09-09T12:00:00Z").unwrap();
    let folder = target(&chosen);
    let transactions = fs::read_to_string(folder.join("transactions/2026.csv")).unwrap();
    assert!(transactions.starts_with("id,title,subtitle,date,amount,sources,record\n"));
    assert!(transactions.contains("transaction:t1,Cafe,,2026-03-04,-12.50,plaid,"));
    // The record's quotes and commas survive as one CSV field.
    assert!(transactions.contains(r#""{""amount"":-12.5,""note"":""Lunch, \""team\""""}""#));
    let cafe = fs::read_to_string(folder.join("organizations/cafe.md")).unwrap();
    assert!(cafe.starts_with("---\nid: \"organization:cafe\"\nkind: organization\n"));
    assert!(cafe.contains("- counterparty: Cafe (2026-03-04, -12.50)"));
    let jane = fs::read_to_string(folder.join("people/jane-example.md")).unwrap();
    assert!(jane.contains("- person: Cafe (2026-03-04, -12.50)"));
    assert!(fs::read_to_string(folder.join("facts.csv"))
        .unwrap()
        .contains("transaction:t1,amount,-12.5,,,plaid,,observed"));
    assert!(fs::read_to_string(folder.join("README.md"))
        .unwrap()
        .contains("2026-09-09T12:00:00Z"));

    // A rewrite replaces the copy whole: a file Brief no longer writes is gone.
    fs::write(folder.join("stale.csv"), "old").unwrap();
    write(&chosen, &graph(), true, "2026-09-10T12:00:00Z").unwrap();
    assert!(!folder.join("stale.csv").exists());
    let masked = fs::read_to_string(folder.join("transactions/2026.csv")).unwrap();
    assert!(!masked.contains("12.5"));
    assert!(masked.contains("•••"));
    fs::remove_dir_all(chosen).unwrap();
}

#[test]
fn never_replaces_or_removes_a_folder_brief_did_not_write() {
    let chosen = chosen();
    fs::create_dir_all(target(&chosen)).unwrap();
    fs::write(target(&chosen).join("mine.txt"), "keep").unwrap();
    assert!(write(&chosen, &graph(), false, "now").is_err());
    assert!(remove(&chosen).is_err());
    assert!(target(&chosen).join("mine.txt").exists());
    fs::remove_dir_all(target(&chosen)).unwrap();

    write(&chosen, &graph(), false, "now").unwrap();
    remove(&chosen).unwrap();
    assert!(!target(&chosen).exists());
    remove(&chosen).unwrap();
    assert!(write(&chosen.join("missing"), &graph(), false, "now").is_err());
    fs::remove_dir_all(chosen).unwrap();
}

#[test]
fn folders_macos_may_sync_are_named() {
    let home = Path::new("/Users/example");
    assert!(synced(Path::new("/Users/example/Documents/Finance"), home));
    assert!(synced(
        Path::new("/Users/example/Library/Mobile Documents/x"),
        home
    ));
    assert!(!synced(Path::new("/Users/example/Brief"), home));
}
