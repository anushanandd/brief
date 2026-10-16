use serde_json::{json, Value};

use super::*;

fn transaction(fields: Value) -> Transaction {
    let mut value = json!({
        "id": "t", "merchant": "Cafe", "category": "Dining", "date": "2026-10-02",
        "amount": -6.5, "account": "Card", "accountId": "plaid:card", "pending": false,
        "description": null, "paymentChannel": "online", "categoryConfidence": "VERY_HIGH",
        "merchantEntityId": "cafe"
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn graded(fields: Value) -> Quality {
    grade(&transaction(fields), &BTreeSet::new(), &BTreeMap::new())
}

#[test]
fn a_statement_confirms_the_amount_or_disagrees_with_it() {
    let matched = BTreeSet::from(["t".to_string()]);
    let confirmed = grade(&transaction(json!({})), &matched, &BTreeMap::new());
    assert_eq!(
        confirmed.amount,
        evidence(Grade::Confirmed, Basis::Statement)
    );
    // Merchant and category rest on Plaid alone, so the whole is only supported.
    assert_eq!(confirmed.grade, Grade::Supported);

    let differs = BTreeMap::from([("t".to_string(), -8.0)]);
    let conflict = grade(&transaction(json!({})), &BTreeSet::new(), &differs);
    assert_eq!(conflict.grade, Grade::Conflict);
    assert_eq!(conflict.statement_amount, Some(-8.0));

    assert_eq!(
        graded(json!({ "pending": true })).amount.basis,
        Basis::Pending
    );
    assert_eq!(
        graded(json!({ "id": "statement:abc-0" })).amount.basis,
        Basis::StatementOnly
    );
}

#[test]
fn places_are_graded_by_whether_the_descriptor_and_plaid_agree() {
    let in_store = |city: &str| {
        graded(json!({
            "paymentChannel": "in store",
            "location": { "city": city },
            "place": { "city": "Menlo Park", "country": "US", "source": "descriptor", "precision": "city" }
        }))
        .location
        .unwrap()
    };
    assert_eq!(in_store("MENLO PARK").basis, Basis::SourcesAgree);
    assert_eq!(in_store("Palo Alto").grade, Grade::Conflict);
    let unplaced = graded(json!({ "paymentChannel": "in store" }));
    assert_eq!(
        unplaced.location,
        Some(evidence(Grade::Weak, Basis::Unreported))
    );
    // An online purchase has no place to grade.
    assert!(graded(json!({})).location.is_none());
}

#[test]
fn a_category_the_user_set_is_confirmed_and_an_unsure_one_is_weak() {
    assert_eq!(
        graded(json!({ "categorySource": "manual", "categoryConfidence": "LOW" })).category,
        evidence(Grade::Confirmed, Basis::Manual)
    );
    let unsure = graded(json!({ "categoryConfidence": "LOW", "merchantEntityId": null }));
    assert_eq!(unsure.category.grade, Grade::Weak);
    assert_eq!(
        unsure.merchant,
        Some(evidence(Grade::Weak, Basis::ProviderUnsure))
    );
    assert_eq!(unsure.grade, Grade::Weak);
}

#[test]
fn only_bank_and_card_transactions_are_graded() {
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../src/data/empty.json")).unwrap();
    snapshot.transactions = vec![
        transaction(json!({})),
        transaction(json!({ "id": "dividend", "accountId": "snaptrade:brokerage" })),
    ];
    apply(&mut snapshot);
    assert!(snapshot.transactions[0].quality.is_some());
    assert!(snapshot.transactions[1].quality.is_none());
}
