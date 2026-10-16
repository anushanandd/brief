use std::fs;

use super::*;
use crate::documents::{crypto::DocumentKey, Document, Field, Kind};
use crate::storage::documents::DocumentChange;

fn document(contents: &[u8]) -> Document {
    Document {
        id: crate::statements::content_id(contents),
        file_name: "W-2 2025 Example.jpeg".into(),
        media_type: "image/jpeg".into(),
        size: contents.len() as u64,
        imported_at: "2026-10-15T12:00:00Z".into(),
        kind: Kind::TaxForm,
        form: Some("W-2".into()),
        corrected: false,
        issuer: Some("Example".into()),
        tax_year: Some(2025),
        date: None,
        period_start: None,
        period_end: None,
        account_id: None,
        mask: None,
        fields: vec![Field {
            key: "box1".into(),
            label: "Wages".into(),
            amount: Some(1_000.0),
            ..Field::default()
        }],
        deposits: Vec::new(),
        checks: Vec::new(),
        statement_rows: None,
        statement_csv: None,
        statement_file_id: None,
        reviewed: false,
        person: None,
        unreadable: None,
        reader: 0,
        summary: None,
    }
}

#[test]
fn documents_are_sealed_kept_with_edits_and_removed_with_their_originals() {
    let root = directory();
    let mut storage = Storage::new(root.clone()).unwrap();
    let key = DocumentKey::generate();
    let contents = b"synthetic image".to_vec();
    let saved = document(&contents);
    let id = saved.id.clone();
    storage
        .add_documents(
            vec![(saved.clone(), key.seal(&id, &contents).unwrap())],
            &key,
        )
        .unwrap();
    // The original is kept sealed, and opens only with this key.
    assert_eq!(storage.document_contents(&id, &key).unwrap(), contents);
    assert!(storage
        .document_contents(&id, &DocumentKey::generate())
        .is_err());
    assert!(!fs::read(storage.documents_path().unwrap())
        .unwrap()
        .windows(contents.len())
        .any(|window| window == contents));

    storage
        .update_document(
            &id,
            DocumentChange {
                reviewed: Some(true),
                fields: Some(vec![Field {
                    key: "box1".into(),
                    label: "Wages".into(),
                    amount: Some(1_200.0),
                    edited: true,
                    ..Field::default()
                }]),
                ..DocumentChange::default()
            },
        )
        .unwrap();
    // A new reading keeps what the user typed.
    storage.replace_reading(saved).unwrap();
    let kept = &storage.data.documents[0];
    assert_eq!(kept.amount("box1"), Some(1_200.0));
    assert!(kept.reviewed);
    drop(storage);
    let mut storage = Storage::new(root.clone()).unwrap();
    assert_eq!(storage.data.documents.len(), 1);

    storage.delete_documents(std::slice::from_ref(&id)).unwrap();
    assert!(storage.data.documents.is_empty());
    assert!(storage.document_contents(&id, &key).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reading_a_statement_in_history_again_replaces_only_its_descriptions() {
    let directory = directory();
    let mut storage = super::statements::storage_with_plaid_cash(&directory);
    let csv = |grocer: &str, amount: &str| {
        format!(
            "Date,Description,Amount\n08/20/2026,\"PAYROLL EXAMPLE\",250.00\n\
             08/03/2026,\"{grocer}\",{amount}\n"
        )
    };
    let polluted = csv("GROCER Page of August 2026 Account Number:", "-31.25");
    let (_, draft) = storage
        .preview_statement("statement.pdf".into(), polluted.clone().into_bytes())
        .unwrap();
    storage.import_statement(draft, "plaid:cash").unwrap();
    let old_file = storage.data.statements[0].id.clone();
    let mut saved = Document {
        kind: Kind::Statement,
        statement_csv: Some(polluted),
        statement_file_id: Some(old_file.clone()),
        ..document(b"synthetic statement")
    };
    storage
        .add_documents(
            vec![(
                saved.clone(),
                DocumentKey::generate().seal(&saved.id, b"x").unwrap(),
            )],
            &DocumentKey::generate(),
        )
        .unwrap();
    let grocer = |storage: &Storage| {
        storage.snapshot().unwrap()["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["amount"] == -31.25)
            .map(|row| row["description"].as_str().unwrap().to_owned())
    };

    // A reading whose figures differ leaves the history alone.
    saved.statement_csv = Some(csv("GROCER", "-31.00"));
    storage.replace_reading(saved.clone()).unwrap();
    assert_eq!(storage.data.statements[0].id, old_file);

    saved.statement_csv = Some(csv("GROCER", "-31.25"));
    storage.replace_reading(saved).unwrap();
    assert_eq!(storage.data.statements.len(), 1);
    let new_file = storage.data.statements[0].id.clone();
    assert_ne!(new_file, old_file);
    assert_eq!(
        storage.data.documents[0].statement_file_id.as_deref(),
        Some(new_file.as_str())
    );
    assert_eq!(grocer(&storage).as_deref(), Some("GROCER"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_backup_carries_documents_and_opens_them_on_another_mac_with_its_passphrase() {
    let root = directory();
    let mut storage = Storage::new(root.join("this")).unwrap();
    let here = DocumentKey::generate();
    let contents = b"synthetic pay statement".to_vec();
    let saved = document(&contents);
    let id = saved.id.clone();
    storage
        .add_documents(vec![(saved, here.seal(&id, &contents).unwrap())], &here)
        .unwrap();
    let portable = root.join("portable.sqlite3");
    storage
        .export_backup(&portable, Some(&here), Some("correct horse"))
        .unwrap();
    let local = root.join("local.sqlite3");
    storage.export_backup(&local, Some(&here), None).unwrap();

    let there = DocumentKey::generate();
    let mut other = Storage::new(root.join("other")).unwrap();
    assert_eq!(
        other
            .inspect_backup(&portable, Some(&there))
            .unwrap()
            .document_key,
        "passphrase"
    );
    assert_eq!(
        other
            .inspect_backup(&local, Some(&there))
            .unwrap()
            .document_key,
        "otherMac"
    );
    assert_eq!(
        storage
            .inspect_backup(&local, Some(&here))
            .unwrap()
            .document_key,
        "thisMac"
    );
    // A wrong passphrase changes nothing.
    assert!(other
        .import_backup(&portable, Some(&there), Some("wrong"))
        .is_err());
    assert!(other.data.documents.is_empty());

    other
        .import_backup(&portable, Some(&there), Some("correct horse"))
        .unwrap();
    assert_eq!(other.document_contents(&id, &there).unwrap(), contents);

    // Without a passphrase, what Brief read comes back but the original stays locked.
    let mut third = Storage::new(root.join("third")).unwrap();
    third.import_backup(&local, Some(&there), None).unwrap();
    assert_eq!(third.data.documents.len(), 1);
    assert!(third.document_contents(&id, &there).is_err());
    fs::remove_dir_all(root).unwrap();
}
