use serde_json::json;

use super::*;

fn place(descriptor: &str) -> Option<(String, Option<String>, Option<String>)> {
    parse_descriptor(descriptor).map(|place| {
        (
            place.country.to_owned(),
            place.region.map(str::to_owned),
            place.city.map(|city| city.name.clone()),
        )
    })
}

fn city(
    country: &str,
    region: &str,
    name: &str,
) -> Option<(String, Option<String>, Option<String>)> {
    Some((country.into(), Some(region.into()), Some(name.into())))
}

#[test]
fn reads_a_city_run_into_a_truncated_merchant_name() {
    // Card descriptors pad the merchant field to a fixed width and drop the space before the city.
    assert_eq!(
        place("AplPay EXAMPLE CAFE RMENLO PARK CA"),
        city("US", "CA", "Menlo Park")
    );
    assert_eq!(
        place("AplPay EXAMPLE STORE CUPERTINO CA"),
        city("US", "CA", "Cupertino")
    );
    assert_eq!(
        place("EXAMPLE SUBSCRIPTIOSAN FRANCISCO CA"),
        city("US", "CA", "San Francisco")
    );
}

#[test]
fn reads_cities_abroad_by_their_country_code_and_local_spelling() {
    assert_eq!(
        place("AplPay EXAMPLE SHOP INTERLAKEN CH"),
        Some(("CH".into(), Some("BE".into()), Some("Interlaken".into())))
    );
    assert_eq!(
        place("AplPay EXAMPLE KIOSK ZÜRICH CH")
            .unwrap()
            .2
            .as_deref(),
        Some("Zürich")
    );
    assert_eq!(
        place("AplPay EXAMPLE KIOSK ZUERICH CH")
            .unwrap()
            .2
            .as_deref(),
        Some("Zürich")
    );
    // DE is Delaware and Germany: the reading with a city wins.
    let munich = place("AplPay SUMUP*EXAMPLE MUENCHEN DE").unwrap();
    assert_eq!(
        (munich.0.as_str(), munich.2.as_deref()),
        ("DE", Some("Munich"))
    );
    // An unexpected code still finds a large city of that name.
    let berlin = place("AplPay EXAMPLE STAND BERLIN BE").unwrap();
    assert_eq!(
        (berlin.0.as_str(), berlin.2.as_deref()),
        ("DE", Some("Berlin"))
    );
}

#[test]
fn keeps_only_the_state_when_the_descriptor_names_no_city() {
    // A web address or a phone number names the merchant, not where the purchase was made.
    assert_eq!(
        place("EXAMPLE SERVICE help.example.com CA"),
        Some(("US".into(), Some("CA".into()), None))
    );
    assert_eq!(
        place("AplPay EXAMPLE SALON408-5550100 CA"),
        Some(("US".into(), Some("CA".into()), None))
    );
    assert_eq!(place("EXAMPLE STORE"), None);
    assert_eq!(place("EXAMPLE PAYMENT 12"), None);
}

fn transaction(descriptor: Option<&str>, location: serde_json::Value) -> Transaction {
    serde_json::from_value(json!({
        "id": "t", "merchant": "Example", "category": "Shopping", "date": "2026-09-01",
        "amount": -20.0, "account": "Card", "pending": false,
        "description": descriptor, "location": location,
    }))
    .unwrap()
}

#[test]
fn the_descriptor_wins_over_a_provider_location_that_names_another_city() {
    // A chain's corporate address can stand in for the store's.
    let resolved = resolve(&transaction(
        Some("AplPay EXAMPLE STORE 140SUNNYVALE CA"),
        json!({ "city": "Oregon", "region": "OH", "lat": 41.64, "lon": -83.48 }),
    ))
    .unwrap();
    assert_eq!(
        (
            resolved.city.as_deref(),
            resolved.region.as_deref(),
            resolved.source,
            resolved.precision
        ),
        (
            Some("Sunnyvale"),
            Some("CA"),
            PlaceSource::Descriptor,
            PlacePrecision::City
        )
    );
    // Agreeing on the city, the provider's own coordinates are kept.
    let exact = resolve(&transaction(
        Some("AplPay EXAMPLE STORE 140SUNNYVALE CA"),
        json!({ "city": "Sunnyvale", "region": "CA", "lat": 37.3711, "lon": -122.0375 }),
    ))
    .unwrap();
    assert_eq!(exact.precision, PlacePrecision::Exact);
    assert_eq!(exact.coordinates(), Some((37.3711, -122.0375)));
}

#[test]
fn the_provider_location_fills_in_what_the_descriptor_leaves_out() {
    // A state alone: the provider's city in that state is more precise.
    let state_only = resolve(&transaction(
        Some("EXAMPLE SERVICE help.example.com CA"),
        json!({ "city": "Cupertino", "region": "CA" }),
    ))
    .unwrap();
    assert_eq!(
        (state_only.city.as_deref(), state_only.source),
        (Some("Cupertino"), PlaceSource::Bank)
    );
    // No descriptor place: the provider's, at the city's centre.
    let bank = resolve(&transaction(
        Some("EXAMPLE STORE"),
        json!({ "city": "Denver", "region": "CO" }),
    ))
    .unwrap();
    assert_eq!(
        (bank.source, bank.precision),
        (PlaceSource::Bank, PlacePrecision::City)
    );
    assert!(bank
        .coordinates()
        .is_some_and(|(lat, _)| (lat - 39.74).abs() < 0.1));
    // Neither: no place.
    assert_eq!(
        resolve(&transaction(Some("EXAMPLE STORE"), json!(null))),
        None
    );
}

fn with_issuer(
    descriptor: &str,
    issuer: serde_json::Value,
    location: serde_json::Value,
) -> Transaction {
    let mut transaction = transaction(Some(descriptor), location);
    transaction.issuer = Some(serde_json::from_value(issuer).unwrap());
    transaction
}

#[test]
fn the_issuers_street_address_names_the_store_in_the_descriptors_city() {
    let resolved = resolve(&with_issuer(
        "AplPay EXAMPLE STORE 140SUNNYVALE CA",
        json!({"address": "125 EXAMPLE AVE", "city": "SUNNYVALE", "region": "CA", "postalCode": "94087", "country": "US"}),
        json!(null),
    ))
    .unwrap();
    assert_eq!(
        (
            resolved.address.as_deref(),
            resolved.postal_code.as_deref(),
            resolved.city.as_deref(),
            resolved.source,
            resolved.precision
        ),
        (
            Some("125 EXAMPLE AVE"),
            Some("94087"),
            Some("Sunnyvale"),
            PlaceSource::Issuer,
            PlacePrecision::Street
        )
    );
    // Plaid's coordinates for the same street address are the store's.
    let exact = resolve(&with_issuer(
        "AplPay EXAMPLE STORE 140SUNNYVALE CA",
        json!({"address": "125 EXAMPLE AVE", "city": "SUNNYVALE", "region": "CA"}),
        json!({"address": "125 E Example Ave", "city": "Sunnyvale", "lat": 37.37, "lon": -122.03}),
    ))
    .unwrap();
    assert_eq!(exact.precision, PlacePrecision::Exact);
    assert_eq!(exact.coordinates(), Some((37.37, -122.03)));
}

#[test]
fn an_issuer_address_in_another_city_is_where_the_merchant_bills_from() {
    let resolved = resolve(&with_issuer(
        "AplPay EXAMPLE MEX NEWPORT BEACH CA",
        json!({"address": "1401 EXAMPLE ST 500", "city": "DENVER", "region": "CO"}),
        json!(null),
    ))
    .unwrap();
    assert_eq!(
        (resolved.city.as_deref(), resolved.source, resolved.address),
        (Some("Newport Beach"), PlaceSource::Descriptor, None)
    );
}

fn in_store(descriptor: &str, location: serde_json::Value) -> Transaction {
    let mut transaction = transaction(Some(descriptor), location);
    transaction.payment_channel = Some("in store".into());
    transaction
}

#[test]
fn a_purchase_abroad_printed_without_a_code_ends_with_its_city() {
    // The issuer printed the store's city with no province or country; Plaid matched another
    // business and gave its address.
    let resolved = resolve(&in_store(
        "AplPay EXAMPLE PHARMA 0DORVAL",
        json!({ "address": "1 Example Blvd", "city": "North Miami", "region": "FL" }),
    ))
    .unwrap();
    assert_eq!(
        (
            resolved.city.as_deref(),
            resolved.country.as_str(),
            resolved.source
        ),
        (Some("Dorval"), "CA", PlaceSource::Descriptor)
    );
    // Only a purchase reads it that way, and only a name no other city shares.
    assert_eq!(
        resolve(&transaction(
            Some("DEP EXAMPLE PAYROLL DEPOSIT"),
            json!(null)
        )),
        None
    );
    assert_eq!(
        resolve(&in_store("EXAMPLE PLAZA SAN JOSE", json!(null))),
        None
    );
}

#[test]
fn reads_canadian_provinces_and_ignores_a_banks_date_trailer() {
    assert_eq!(
        place("ATM Withdrawal EXAMPLE BANK DORVAL QC Transaction Date : 10/05/2026"),
        Some(("CA".into(), Some("10".into()), Some("Dorval".into())))
    );
    // A town merged into a city, which GeoNames lists without a population.
    assert_eq!(
        place("ATM Withdrawal EXAMPLE BANK HOSPITALIE LACHINE QC"),
        Some(("CA".into(), Some("10".into()), Some("Lachine".into())))
    );
    // A province alone, without a known city.
    assert_eq!(
        place("ATM Withdrawal EXAMPLE BANK SMALLPLACE QC"),
        Some(("CA".into(), Some("10".into()), None))
    );
    // NL is the Netherlands as well as Newfoundland: the reading with a city wins.
    assert_eq!(
        place("AplPay EXAMPLE SHOP AMSTERDAM NL").map(|place| place.0),
        Some("NL".into())
    );
}
