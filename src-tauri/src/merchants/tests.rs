use serde_json::{json, Value};

use super::*;

/// A card purchase as Plaid reports it, with synthetic amounts.
fn purchase(merchant: &str, descriptor: &str, confidence: &str, extra: Value) -> Transaction {
    let mut value = json!({
        "id": "t1", "merchant": merchant, "category": "Shopping", "date": "2026-05-04",
        "amount": -18.5, "account": "Card", "pending": false,
        "description": descriptor, "originalDescription": descriptor,
        "paymentChannel": "in store",
        "counterparties": [{"name": merchant, "type": "merchant", "confidence": confidence}],
    });
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn issuer(name: &str, category: &str) -> Value {
    json!({"issuer": {"name": name, "category": category}})
}

#[test]
fn another_brand_in_another_city_is_corrected_to_the_descriptor_and_its_alias() {
    let transaction = purchase(
        "Dollar General",
        "AplPay DSG STORE 140SUNNYVALE CA",
        "VERY_HIGH",
        json!({"location": {"city": "Oregon", "region": "OH"}}),
    );
    assert_eq!(
        corrected_name(&transaction).as_deref(),
        Some("Dick's Sporting Goods")
    );
}

#[test]
fn another_number_is_another_merchant() {
    let transaction = purchase(
        "Curry 42",
        "AplPay LS CURRY 36 BERLIN DE",
        "HIGH",
        json!({}),
    );
    assert_eq!(corrected_name(&transaction).as_deref(), Some("CURRY 36"));
}

#[test]
fn a_recognized_abbreviation_or_a_more_general_name_stands() {
    let dunkin = purchase(
        "Dunkin'",
        "AplPay DD/BR #347869OAK PARK IL",
        "VERY_HIGH",
        json!({}),
    );
    assert_eq!(corrected_name(&dunkin), None);
    let hotel = purchase(
        "Example Resorts",
        "AplPay EXAMPLE REG SEASEATTLE WA",
        "VERY_HIGH",
        issuer(
            "AplPay EXAMPLE REG SEATTLE F&B 0042",
            "Restaurant-Restaurant",
        ),
    );
    assert_eq!(corrected_name(&hotel), None);
    let chain = purchase(
        "Example Grill",
        "AplPay EXAMPLE GRILL SAN JOSE CA",
        "VERY_HIGH",
        json!({}),
    );
    assert_eq!(corrected_name(&chain), None);
}

#[test]
fn an_unrecognized_name_takes_the_issuers_fuller_one() {
    let transaction = purchase(
        "Pizza Bosan",
        "AplPay TST* PIZZA BOSAN JOSE CA",
        "LOW",
        issuer(
            "AplPay TST* PIZZA BOCCA LUPO - S 300795384",
            "Restaurant-Restaurant",
        ),
    );
    assert_eq!(
        corrected_name(&transaction).as_deref(),
        Some("PIZZA BOCCA LUPO")
    );
}

#[test]
fn an_unrecognized_name_loses_its_processor_prefix() {
    let transaction = purchase(
        "Ls Kumpira",
        "AplPay LS KUMPIRA BERLIN DE",
        "LOW",
        json!({}),
    );
    assert_eq!(corrected_name(&transaction).as_deref(), Some("KUMPIRA"));
}

#[test]
fn plaid_enrichment_absent_leaves_the_name_alone() {
    let mut transaction = purchase("Example Kiosk", "EXAMPLE KIOSK BERN CH", "LOW", json!({}));
    transaction.counterparties.clear();
    assert_eq!(corrected_name(&transaction), None);
}

#[test]
fn remote_purchases_are_told_by_brand_words_category_and_code() {
    let subscription = purchase(
        "Example AI",
        "EXAMPLEAI *CHAT SUBSSAN FRANCISCO CA",
        "VERY_HIGH",
        json!({}),
    );
    assert_eq!(presence(&subscription), Some(Presence::Remote));
    let order = purchase(
        "Example Grill",
        "AplPay EXAMPLE MEX NEWPORT BEACH CA",
        "VERY_HIGH",
        issuer("AplPay EXAMPLE MEX GR ONLINE", "Restaurant-Bar & Café"),
    );
    assert_eq!(presence(&order), Some(Presence::Remote));
    let shipped = purchase(
        "Example Direct",
        "EXAMPLE DIRECT #689 POTTSVILLE PA",
        "VERY_HIGH",
        issuer("", "Merchandise & Supplies-Internet Purchase"),
    );
    assert_eq!(presence(&shipped), Some(Presence::Remote));
    let flight = purchase(
        "Example Air",
        "EXAMPLE AIRLINES HOUSTON TX",
        "VERY_HIGH",
        json!({"categoryDetail": "TRAVEL_FLIGHTS"}),
    );
    assert_eq!(presence(&flight), Some(Presence::Remote));
}

#[test]
fn in_person_purchases_include_processors_plaid_called_online() {
    let counter = purchase(
        "Example Cafe",
        "AplPay EXAMPLE CAFE SAN JOSE CA",
        "HIGH",
        json!({}),
    );
    assert_eq!(presence(&counter), Some(Presence::InPerson));
    let pickup = purchase(
        "Example Taqueria",
        "AplPay TST* EXAMPLE TAQSAN JOSE CA",
        "HIGH",
        json!({"paymentChannel": "online"}),
    );
    assert_eq!(presence(&pickup), Some(Presence::InPerson));
    let ordered = purchase(
        "Example Shop",
        "EXAMPLE SHOP SAN JOSE CA",
        "HIGH",
        json!({"paymentChannel": "online"}),
    );
    assert_eq!(presence(&ordered), Some(Presence::Remote));
    let transfer = purchase(
        "Example Bank",
        "ONLINE TRANSFER",
        "HIGH",
        json!({"paymentChannel": "other"}),
    );
    assert_eq!(presence(&transfer), None);
}

#[test]
fn a_vending_processor_bills_from_its_own_address() {
    let transaction = purchase(
        "Example Vending",
        "AplPay CTLP*EXAMPLE VEND CHARLOTTE NC",
        "VERY_HIGH",
        json!({"issuer": {"address": "100 EXAMPLE RD", "city": "CHARLOTTE", "region": "NC"}}),
    );
    assert!(bills_elsewhere(&transaction));
    assert_eq!(presence(&transaction), Some(Presence::InPerson));
}

#[test]
fn a_city_printed_without_a_code_still_contradicts_a_business_plaid_placed_elsewhere() {
    let transaction = purchase(
        "Sample Couture LLC",
        "AplPay EXAMPLE PHARMA 0DORVAL",
        "VERY_HIGH",
        json!({"location": {"city": "North Miami", "region": "FL"}}),
    );
    assert_eq!(
        corrected_name(&transaction).as_deref(),
        Some("EXAMPLE PHARMA")
    );
}
