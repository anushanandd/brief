use serde_json::json;

use super::*;

fn purchase(descriptor: &str, issuer: serde_json::Value) -> Transaction {
    let mut transaction: Transaction = serde_json::from_value(json!({
        "id": "t", "merchant": "Example", "category": "Shopping", "date": "2026-05-04",
        "amount": -21.0, "account": "Card", "pending": false,
        "description": descriptor, "originalDescription": descriptor,
        "paymentChannel": "in store",
        "counterparties": [{"name": "Example", "type": "merchant", "confidence": "LOW"}],
        "issuer": issuer,
    }))
    .unwrap();
    transaction.presence = merchants::presence(&transaction);
    transaction.place = super::super::resolve(&transaction);
    transaction
}

fn listing(name: &str, address: &str, lat: f64, lon: f64) -> OverturePlace {
    OverturePlace {
        id: format!("{name}|{address}"),
        name: name.into(),
        category: Some("restaurant".into()),
        confidence: 0.9,
        address: Some(address.into()),
        lat,
        lon,
        ..OverturePlace::default()
    }
}

fn street_purchase() -> Transaction {
    purchase(
        "AplPay DSG STORE 140SUNNYVALE CA",
        json!({"name": "AplPay DSG STORE 1401", "address": "125 EAST EXAMPLE REAL", "city": "SUNNYVALE", "region": "CA"}),
    )
}

#[test]
fn the_brand_at_the_issuers_address_matches_and_its_neighbours_do_not() {
    let transaction = street_purchase();
    let need = need(&transaction).unwrap();
    assert!(need.key.starts_with("street|US|CA|SUNNYVALE|125|EXAMPLE"));
    let area = [
        listing(
            "Dick's Sporting Goods",
            "125 E Example Real",
            37.37,
            -122.03,
        ),
        listing(
            "Example Recruiting Office",
            "125 E Example Real",
            37.37,
            -122.03,
        ),
        listing("Dick's Sporting Goods", "900 Other St", 37.40, -122.05),
    ];
    let kept = candidates(&need, &area);
    assert_eq!(kept.len(), 2);
    let business = choose(&transaction, &need, &kept).unwrap();
    assert_eq!(business.name, "Dick's Sporting Goods");
}

#[test]
fn two_businesses_fitting_one_address_match_neither() {
    let transaction = purchase(
        "AplPay EXAMPLE SAN JOSE CA",
        json!({"name": "AplPay EXAMPLE", "address": "1 SAMPLE PLAZA", "city": "SAN JOSE", "region": "CA"}),
    );
    let need = need(&transaction).unwrap();
    let kept = [
        listing("Example Grill", "1 Sample Plaza", 37.33, -121.89),
        listing("Example Bar", "1 Sample Plaza", 37.33, -121.89),
    ];
    assert_eq!(choose(&transaction, &need, &kept), None);
    // A department gives way to the business itself.
    let departments = [
        listing("Example", "1 Sample Plaza", 37.33, -121.89),
        listing("Example Pharmacy", "1 Sample Plaza", 37.33, -121.89),
    ];
    assert_eq!(
        choose(&transaction, &need, &departments).map(|business| business.name),
        Some("Example".into())
    );
}

#[test]
fn a_head_office_at_the_address_never_matches() {
    let transaction = purchase(
        "AplPay EXAMPLE GRILL SAN JOSE CA",
        json!({"address": "1 EXAMPLE PLAZA", "city": "SAN JOSE", "region": "CA"}),
    );
    let need = need(&transaction).unwrap();
    let mut office = listing("Example Grill", "1 Example Plaza", 37.33, -121.89);
    office.category = Some("corporate_or_business_office".into());
    assert_eq!(choose(&transaction, &need, &[office]), None);
}

#[test]
fn a_city_match_needs_one_location_outside_any_chain() {
    let transaction = purchase("AplPay EXAMPLE NOODLE HOUSAN JOSE CA", json!(null));
    let need = need(&transaction).unwrap();
    assert!(need.key.starts_with("city|US|CA|SAN JOSE|EXAMPLE NOODLE"));
    let only = [listing(
        "Example Noodle House",
        "5 Sample St",
        37.33,
        -121.88,
    )];
    assert_eq!(
        choose(&transaction, &need, &only).map(|business| business.name),
        Some("Example Noodle House".into())
    );
    // The same place listed twice is still one place.
    let twice = [
        only[0].clone(),
        listing(
            "Example Noodle House",
            "5 Sample Street",
            37.3301,
            -121.8801,
        ),
    ];
    assert!(choose(&transaction, &need, &twice).is_some());
    // Two locations: no way to know which.
    let branches = [
        only[0].clone(),
        listing("Example Noodle House", "88 Far Ave", 37.30, -121.80),
    ];
    assert_eq!(choose(&transaction, &need, &branches), None);
    // A chain's lone location in the area may not be the only one in the city.
    let mut chain = only[0].clone();
    chain.brand = Some("Example Noodle House".into());
    assert_eq!(choose(&transaction, &need, &[chain]), None);
}

#[test]
fn remote_purchases_and_short_names_are_never_searched() {
    let remote = purchase(
        "EXAMPLEAI *CHAT SUBSSAN FRANCISCO CA",
        json!({"address": "1 EXAMPLE ST", "city": "SAN FRANCISCO", "region": "CA"}),
    );
    assert_eq!(need(&remote), None);
    let short = purchase("AplPay CAFE SAN JOSE CA", json!(null));
    assert_eq!(need(&short), None);
}

#[test]
fn a_match_moves_the_pin_and_names_an_unrecognized_merchant() {
    let mut transactions = vec![street_purchase()];
    let key = need(&transactions[0]).unwrap().key;
    let searches = BTreeMap::from([(
        key,
        vec![listing(
            "Dick's Sporting Goods",
            "125 E Example Real",
            37.371,
            -122.031,
        )],
    )]);
    apply(&mut transactions, &searches);
    let transaction = &transactions[0];
    assert_eq!(transaction.merchant, "Dick's Sporting Goods");
    assert_eq!(transaction.provider_merchant.as_deref(), Some("Example"));
    let place = transaction.place.as_ref().unwrap();
    assert_eq!(place.precision, PlacePrecision::Exact);
    assert_eq!(place.coordinates(), Some((37.371, -122.031)));
}

#[test]
fn a_numbered_store_is_never_found_by_name_alone() {
    let transaction = purchase("AplPay EXAMPLE MART #1196SUNNYVALE CA", json!(null));
    assert_eq!(need(&transaction), None);
}

#[test]
fn a_listing_in_a_neighbouring_city_is_another_store() {
    let transaction = purchase("AplPay EXAMPLE BEAUTY-VALLESANTA CLARA CA", json!(null));
    let need = need(&transaction).unwrap();
    let mut neighbour = listing("Example Beauty", "377 Sample Row", 37.32, -121.95);
    neighbour.locality = Some("San Jose".into());
    assert_eq!(choose(&transaction, &need, &[neighbour.clone()]), None);
    neighbour.locality = Some("Santa Clara".into());
    assert!(choose(&transaction, &need, &[neighbour]).is_some());
}

#[test]
fn a_name_must_be_spelled_out_not_merely_started() {
    let transaction = purchase(
        "AplPay EXAMPLE KISUNNYVALE CA",
        json!({"name": "AplPay EXAMPLE KITCHEN", "address": "812 W SAMPLE REAL", "city": "SUNNYVALE", "region": "CA"}),
    );
    let need = need(&transaction).unwrap();
    let next_door = listing("Example Supermarket", "812 W Sample Real", 37.37, -122.04);
    assert_eq!(choose(&transaction, &need, &[next_door]), None);
    let short = listing("Exa", "812 W Sample Real", 37.37, -122.04);
    assert_eq!(choose(&transaction, &need, &[short]), None);
    let itself = listing("Example Kitchen", "812 W Sample Real", 37.37, -122.04);
    assert!(choose(&transaction, &need, &[itself]).is_some());
}
