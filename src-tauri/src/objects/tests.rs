use std::collections::BTreeSet;

use super::*;
use crate::{
    storage::{change, Staged, Storage},
    synthetic::Life,
};

/// The synthetic life after `days` refreshes, and its graph.
fn lived(days: usize) -> (std::path::PathBuf, Storage) {
    let directory = std::env::temp_dir().join(format!("brief-objects-{}", uuid::Uuid::new_v4()));
    let mut storage = Storage::new(directory.clone()).unwrap();
    let life = Life::new(3);
    for day in life.refresh_days().into_iter().take(days) {
        let refresh = life.refresh(day);
        let id = uuid::Uuid::new_v4().to_string();
        let revision = storage.data.revision;
        storage
            .stage_refresh(
                &id,
                &refresh.committed_at.to_rfc3339(),
                revision,
                Staged {
                    refresh: refresh.clone(),
                    diagnostics: Default::default(),
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
    (directory, storage)
}

fn graph_of(storage: &Storage) -> Graph {
    graph(&storage.data, &storage.read_view().unwrap())
}

#[test]
fn every_record_is_an_object_with_a_unique_id_and_links_lead_somewhere() {
    let (directory, storage) = lived(4);
    let graph = graph_of(&storage);
    let ids = graph
        .objects
        .iter()
        .map(|object| object.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), graph.objects.len());
    let view = storage.read_view().unwrap();
    for transaction in &view.snapshot.transactions {
        assert!(ids.contains(Kind::Transaction.id(&transaction.id).as_str()));
    }
    for account in &view.snapshot.accounts {
        assert!(ids.contains(Kind::Account.id(&account.id).as_str()));
    }
    for object in &graph.objects {
        assert_eq!(kind_of(&object.id), Some(object.kind), "{}", object.id);
    }
    for link in &graph.links {
        assert!(ids.contains(link.from.as_str()) && ids.contains(link.to.as_str()));
    }
    for fact in &graph.facts {
        assert!(ids.contains(fact.subject.as_str()), "{}", fact.subject);
    }
    // Every transaction names its account and counterparty.
    let linked = |relation| {
        graph
            .links
            .iter()
            .filter(|link| link.relation == relation)
            .map(|link| link.from.as_str())
            .collect::<BTreeSet<_>>()
    };
    let transactions = view
        .snapshot
        .transactions
        .iter()
        .filter(|transaction| transaction.account_id.is_some())
        .count();
    assert_eq!(
        linked(Relation::Account)
            .iter()
            .filter(|id| id.starts_with("transaction:"))
            .count(),
        transactions
    );
    drop(storage);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn ids_derive_from_names_so_every_read_gives_the_same_ones() {
    let (directory, mut storage) = lived(3);
    let before = graph_of(&storage);
    let organizations = |graph: &Graph| {
        graph
            .objects
            .iter()
            .filter(|object| object.kind == Kind::Organization)
            .map(|object| object.id.clone())
            .collect::<BTreeSet<_>>()
    };
    let earlier = organizations(&before);
    drop(storage);
    storage = Storage::new(directory.clone()).unwrap();
    let after = graph_of(&storage);
    assert_eq!(organizations(&after), earlier);
    assert!(!earlier.is_empty());
    drop(storage);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn reads_come_from_the_tables_which_never_drift_from_the_view() {
    let (directory, mut storage) = lived(3);
    let transaction = storage.read_view().unwrap().snapshot.transactions[0].clone();
    let id = Kind::Transaction.id(&transaction.id);
    let detail = storage.object(&id).unwrap().unwrap();
    assert_eq!(detail.object.title, transaction.merchant);
    assert!(detail
        .links
        .iter()
        .any(|link| link.relation == Relation::Account && link.from == id));
    assert!(detail.facts.iter().any(|fact| fact.predicate == "amount"));
    assert!(detail.provider.is_some(), "Plaid's own record of it");

    // A row changed behind Brief's back is rewritten before it's read.
    rusqlite::Connection::open(storage.database_path())
        .unwrap()
        .execute(
            "UPDATE graph_objects SET title = 'Changed' WHERE id = ?1",
            [&id],
        )
        .unwrap();
    assert_eq!(
        storage.object(&id).unwrap().unwrap().object.title,
        transaction.merchant
    );
    assert!(storage.object("transaction:missing").unwrap().is_none());

    storage.erase().unwrap();
    let cleared: i64 = rusqlite::Connection::open(storage.database_path())
        .unwrap()
        .query_row("SELECT COUNT(*) FROM graph_objects", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cleared, 0);
    drop(storage);
    std::fs::remove_dir_all(directory).unwrap();
}

/// The top-level field names of a ts-rs declaration, skipping comments and nested types.
fn declared_fields(decl: &str) -> Vec<String> {
    let body = &decl[decl.find('{').unwrap() + 1..decl.rfind('}').unwrap()];
    let mut fields = Vec::new();
    let (mut depth, mut name, mut expecting) = (0, String::new(), true);
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '/' && characters.peek() == Some(&'*') {
            while let Some(next) = characters.next() {
                if next == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    break;
                }
            }
            continue;
        }
        match character {
            '{' | '<' | '(' | '[' => depth += 1,
            '}' | '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => expecting = true,
            ':' | '?' if depth == 0 && expecting => {
                if !name.is_empty() {
                    fields.push(std::mem::take(&mut name));
                }
                expecting = false;
            }
            _ if depth == 0 && expecting && (character.is_alphanumeric() || character == '_') => {
                name.push(character)
            }
            _ if depth == 0 && expecting && !character.is_whitespace() => name.clear(),
            _ => {}
        }
    }
    fields
}

/// Coverage: everything Brief stores can be seen. Collections become objects, whose raw view
/// shows every field; the rest is a row of a table the Storage page opens. A field added
/// without a place in either list fails here, so nothing is stored invisibly.
#[test]
fn every_stored_field_has_a_place_to_be_seen() {
    use ts_rs::TS;
    let objects = ["accounts", "holdings", "trades", "transactions"];
    // finance-state.sqlite3 › snapshot_entity holds collections, snapshot_scalar the rest,
    // account_links the links; the Storage page opens each.
    let stored = [
        "calculationVersion",
        "revision",
        "syncWarnings",
        "providerStatus",
        "accountLinks",
        "provenance",
        "recovery",
        "updatedAt",
        "calendarDate",
        "netWorth",
        "netWorthIncomplete",
        "netWorthProvisional",
        "accountMovements",
        "transactionChanges",
        "spending",
        "netWorthHistory",
        "netWorthHistoryEstimated",
        "benchmarkHistory",
        "brokeragePerformance",
        "accountBalanceHistory",
        "observedNetWorthHistory",
        "possibleDuplicateAccounts",
        "recurringStreams",
        "dividendHistory",
        "historyCheck",
        "statementReconciliation",
        "contributionYears",
        "lastChange",
    ];
    let declared = declared_fields(&<crate::finance_contract::Snapshot as TS>::decl(
        &ts_rs::Config::default(),
    ));
    let placed = objects
        .iter()
        .chain(&stored)
        .copied()
        .collect::<BTreeSet<_>>();
    let unplaced = declared
        .iter()
        .filter(|field| !placed.contains(field.as_str()))
        .collect::<Vec<_>>();
    assert!(unplaced.is_empty(), "No place to see {unplaced:?}");
    assert_eq!(
        declared.len(),
        placed.len(),
        "A listed field no longer exists"
    );

    // The rest of the state, field by field: adding one stops this compiling.
    let crate::storage::FinanceState {
        schema_version: _, // Diagnostics
        revision: _,       // Diagnostics
        snapshot: _,       // above
        plaid_cache: _,    // state_meta, in the Storage page
        provider_data: _,  // each object's "As the provider sent it", and state_meta
        annotations: _,    // your facts on transactions
        account_links: _,  // Settings › Accounts, and account_links
        settings: _,       // Settings, and finance_settings
        statements: _,     // statement objects
        documents: _,      // document objects
    } = crate::storage::empty_state().unwrap();
}

#[test]
fn one_name_however_written_is_one_organization() {
    assert_eq!(
        organization_key("MORGAN STANLEY"),
        organization_key("Morgan  Stanley,")
    );
    assert_eq!(organization_key("  "), None);
    assert_eq!(organization_key("AT&T").as_deref(), Some("at-t"));
}

#[test]
fn edits_are_your_facts_and_pay_statements_date_an_employer() {
    let (directory, mut storage) = lived(3);
    let purchase = storage.data.snapshot.transactions[0].id.clone();
    storage
        .save_annotations(
            std::collections::BTreeMap::from([(
                purchase.clone(),
                crate::storage::Annotation {
                    note: Some("Team lunch".into()),
                    ..Default::default()
                },
            )]),
            false,
        )
        .unwrap();
    let payslip = |id: &str, start: &str, end: &str| -> crate::documents::Document {
        serde_json::from_value(json!({
            "id": id, "fileName": format!("{id}.pdf"), "mediaType": "application/pdf",
            "size": 1, "importedAt": "2026-09-01T00:00:00Z", "kind": "payslip",
            "issuer": "Example Employer Inc", "periodStart": start, "periodEnd": end,
        }))
        .unwrap()
    };
    storage.data.documents = vec![
        payslip("pay-1", "2026-06-01", "2026-06-15"),
        payslip("pay-2", "2026-08-16", "2026-08-31"),
    ];
    let graph = graph_of(&storage);
    let note = graph
        .facts
        .iter()
        .find(|fact| fact.subject == Kind::Transaction.id(&purchase) && fact.predicate == "note")
        .unwrap();
    assert_eq!(note.value, json!("Team lunch"));
    assert_eq!(note.sources, ["you"]);
    let employer = graph
        .facts
        .iter()
        .find(|fact| fact.subject == facts::YOU && fact.predicate == "employer")
        .unwrap();
    assert_eq!(employer.value, json!("organization:example-employer-inc"));
    assert_eq!(
        (employer.from.as_deref(), employer.to.as_deref()),
        (Some("2026-06-01"), Some("2026-08-31"))
    );
    assert_eq!(employer.sources, ["document:pay-1", "document:pay-2"]);
    drop(storage);
    std::fs::remove_dir_all(directory).unwrap();
}
