use super::*;
use crate::finance_contract::decode;
use serde_json::json;

fn projection_time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

// Assertions cover the serialized renderer contract as well as calculations.
fn project_json(
    sync: &ProviderSync,
    links: &BTreeMap<String, String>,
    now: DateTime<Utc>,
) -> Result<Value, String> {
    serde_json::to_value(super::project(sync, links, now, now.date_naive())?)
        .map_err(|error| error.to_string())
}

use crate::providers::{PlaidData, SnapTradeData};

fn sync() -> ProviderSync {
    ProviderSync {
        sync_id: "sync".into(),
        balances_fresh: true,
        refreshed_account_ids: vec!["plaid:cash".into(), "snaptrade:brokerage".into()],
        previous_snapshot: serde_json::from_str(include_str!("../../../../src/data/empty.json"))
            .unwrap(),
        plaid: PlaidData {
            accounts: vec![json!({
                "account_id": "cash", "name": "Checking", "institution_name": "Bank", "type": "depository",
                "balance_fetched_at": "2026-09-09T07:29:00Z",
                "balances": { "current": "100.105", "iso_currency_code": "USD", "last_updated_datetime": null }
            })],
            transactions: vec![json!({
                "transaction_id": "coffee", "account_id": "cash", "amount": "5.125", "name": "Coffee", "merchant_name": "Coffee",
                "pending": false, "date": "2026-09-08", "datetime": "2026-09-08T09:10:00Z",
                "authorized_date": "2026-09-07", "authorized_datetime": "2026-09-07T15:42:00Z",
                "location": { "address": "123 Example St", "city": "San Francisco", "region": "CA", "postal_code": "94103" },
                "payment_channel": "in store",
                "logo_url": "https://plaid-merchant-logos.plaid.com/coffee.png", "website": "https://coffee.example",
                "personal_finance_category": {
                    "primary": "FOOD_AND_DRINK",
                    "detailed": "FOOD_AND_DRINK_COFFEE",
                    "confidence_level": "VERY_HIGH"
                },
                "counterparties": [{ "type": "payment_app", "name": "Zelle" }],
                "transaction_code": "digital_payment"
            })],
            ..PlaidData::default()
        },
        snaptrade: SnapTradeData {
            accounts: vec![json!({
                "id": "brokerage", "name": "Brokerage", "institution_name": "Broker", "balance": { "total": { "amount": "200.105", "currency": "USD" } },
                "sync_status": { "holdings": { "last_successful_sync": "2026-09-09T11:59:00Z" } }
            })],
            cash_balances: BTreeMap::from([(
                "brokerage".into(),
                vec![json!({
                    "currency": { "code": "USD" }, "cash": "200.105"
                })],
            )]),
            positions: BTreeMap::from([("brokerage".into(), vec![])]),
            balance_history: BTreeMap::from([(
                "brokerage".into(),
                vec![json!({ "date": "2026-09-08", "total_value": "190.00" })],
            )]),
            ..SnapTradeData::default()
        },
        benchmark_history: Value::Array(Vec::new()),
        security_history: BTreeMap::new(),
        statements: Vec::new(),
        corporate_actions: Vec::new(),
    }
}

#[test]
fn native_finance_fixture_matches_typed_projection() {
    let mut input = sync();
    input.snaptrade.accounts[0]["balance"]["total"]["amount"] = json!(100.105);
    input.snaptrade.cash_balances.insert(
        "brokerage".into(),
        vec![json!({ "currency": { "code": "USD" }, "cash": 50 })],
    );
    input.snaptrade.positions.insert(
        "brokerage".into(),
        vec![json!({
            "units": 2, "price": 100, "cost_basis": 75, "currency": "USD",
            "instrument": { "kind": "stock", "symbol": "TEST", "description": "Synthetic Security" }
        })],
    );
    input.plaid.accounts[0]["balances"]["available"] = json!("90.10");
    input.plaid.accounts[0]["official_name"] = json!("Everyday Checking");
    input.plaid.accounts[0]["subtype"] = json!("checking");
    input.plaid.accounts[0]["holder_category"] = json!("personal");
    input.plaid.accounts[0]["institution_url"] = json!("https://www.bank.example");
    input.snaptrade.accounts[0]["institution_url"] = json!("https://us.broker.example/home");
    input.plaid.accounts.push(json!({
        "account_id": "card", "name": "Card", "institution_name": "Bank", "type": "credit", "subtype": "credit card",
        "balance_fetched_at": "2026-09-09T07:29:00Z",
        "balances": { "current": "300", "available": "1700", "limit": "2000", "iso_currency_code": "USD" }
    }));
    input.plaid.liabilities.push(json!({
        "account_id": "card", "is_overdue": false, "last_statement_balance": "250", "minimum_payment_amount": "35",
        "last_statement_issue_date": "2026-08-25", "next_payment_due_date": "2026-09-19",
        "aprs": [
            { "apr_type": "purchase_apr", "apr_percentage": 24.99, "balance_subject_to_apr": "120", "interest_charge_amount": "2.47" },
            { "apr_type": "cash_apr", "apr_percentage": 29.99, "balance_subject_to_apr": null, "interest_charge_amount": null }
        ]
    }));
    let coffee = &mut input.plaid.transactions[0];
    coffee["merchant_entity_id"] = json!("entity-coffee");
    coffee["original_description"] = json!("SQ *COFFEE 0412 SAN FRANCISCO CA");
    coffee["check_number"] = json!("1042");
    coffee["account_owner"] = json!("Synthetic Holder");
    coffee["location"]["lat"] = json!(37.7749);
    coffee["location"]["lon"] = json!(-122.4194);
    coffee["location"]["store_number"] = json!("0412");
    coffee["counterparties"] = json!([
        { "type": "merchant", "name": "Coffee", "confidence_level": "VERY_HIGH", "website": "coffee.example" },
        { "type": "payment_app", "name": "Square", "confidence_level": "HIGH" }
    ]);
    input.plaid.investment_accounts.push(json!({
        "account_id": "hsa", "name": "Health Savings", "institution_name": "Bank", "type": "investment", "subtype": "hsa",
        "balances": { "current": "500", "iso_currency_code": "USD" }
    }));
    input.plaid.holdings.push(json!({
        "account_id": "hsa", "security_id": "fund", "quantity": "10", "institution_price": "40", "institution_value": "400",
        "cost_basis": "350", "iso_currency_code": "USD", "vested_quantity": "6", "vested_value": "240"
    }));
    input.plaid.holdings.push(json!({
        "account_id": "hsa", "security_id": "bond", "quantity": "1", "institution_price": "100", "institution_value": "100",
        "iso_currency_code": "USD"
    }));
    input.plaid.securities.push(json!({
        "security_id": "fund", "ticker_symbol": "FUND", "name": "Synthetic Fund", "type": "etf", "subtype": "etf",
        "iso_currency_code": "USD", "isin": "US0000000001", "cusip": "000000001", "figi": "BBG000000001",
        "market_identifier_code": "XNAS", "sector": "Finance", "industry": "Investment Trusts"
    }));
    input.plaid.securities.push(json!({
        "security_id": "bond", "name": "Synthetic Treasury", "type": "fixed income", "subtype": "bill",
        "iso_currency_code": "USD",
        "fixed_income": { "yield_rate": { "percentage": 4.25, "type": "yield_to_maturity" }, "maturity_date": "2027-03-31", "issue_date": "2026-03-31", "face_value": 100 }
    }));
    input.plaid.investment_transactions = vec![
        json!({ "investment_transaction_id": "fund-buy", "account_id": "hsa", "security_id": "fund", "date": "2026-08-03",
                "name": "BUY FUND", "quantity": 2, "amount": 80, "price": 40, "fees": 0, "type": "buy", "subtype": "buy",
                "iso_currency_code": "USD" }),
        json!({ "investment_transaction_id": "fund-dividend", "account_id": "hsa", "security_id": "fund", "date": "2026-09-01",
                "name": "FUND DIVIDEND", "quantity": 0, "amount": -3.21, "price": 0, "type": "cash", "subtype": "qualified dividend",
                "iso_currency_code": "USD" }),
    ];
    input.plaid.investment_history_accounts = vec!["hsa".into()];
    input.plaid.recurring_streams = vec![json!({
        "stream_id": "stream-coffee", "account_id": "cash", "direction": "out", "description": "Coffee", "merchant_name": "Coffee",
        "frequency": "WEEKLY", "status": "MATURE", "is_active": true, "last_date": "2026-09-08", "predicted_next_date": "2026-09-15",
        "average_amount": { "amount": 5.13, "iso_currency_code": "USD" }, "transaction_ids": ["coffee"]
    })];
    input.corporate_actions = vec![
        serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "FUND", "exDate": "2026-09-20", "payableDate": "2026-09-25", "rate": 0.4, "special": false, "currency": "USD" })).unwrap(),
    ];
    input.snaptrade.activity_complete = true;
    input.snaptrade.history_complete = true;
    input.snaptrade.activities.insert("brokerage".into(), vec![
        json!({"id":"purchase", "type":"BUY", "amount":225, "trade_date":"2026-09-01T14:00:00Z", "units":3, "price":75, "symbol":{"symbol":"TEST"}}),
        json!({"id":"sale", "type":"SELL", "amount":100, "trade_date":"2026-09-08T20:15:00Z", "units":1, "price":100, "symbol":{"symbol":"TEST"}}),
    ]);
    let now = DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let first = super::project(&input, &BTreeMap::new(), now, now.date_naive()).unwrap();
    input.previous_snapshot = first;
    let later = now + Duration::hours(1);
    let mut snapshot = super::project(&input, &BTreeMap::new(), later, later.date_naive()).unwrap();
    snapshot.revision = Some(2);
    snapshot.transactions[0].benefit_confirmed = Some(false);
    snapshot.provider_status = Some(BTreeMap::from([(
        "plaid".into(),
        crate::finance_contract::ProviderSyncStatus {
            updated_at: Some(now.to_rfc3339()),
            error: None,
        },
    )]));
    let value = serde_json::to_value(snapshot).unwrap();
    // Explicit regeneration only; normal test runs never mutate the fixture.
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/data/fixtures/native-finance.json");
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        )
        .unwrap();
        return;
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../src/data/fixtures/native-finance.json"
    ))
    .unwrap();
    assert_eq!(value, fixture, "Native contract changed: regenerate the synthetic fixture and run pnpm test to check Zod compatibility");
}

/// The committed JSON snapshot repriced at `market` as the live overlay sends it.
fn live(committed: &Value, market: &BTreeMap<String, MarketSnapshot>) -> Value {
    let snapshot = decode(committed.clone()).unwrap();
    serde_json::to_value(apply_market_snapshots(&snapshot, market).unwrap()).unwrap()
}

/// The committed JSON snapshot repriced at past quotes.
fn historical(committed: &Value, market: &BTreeMap<String, MarketSnapshot>) -> Value {
    let snapshot = decode(committed.clone()).unwrap();
    serde_json::to_value(apply_historical_market_snapshots(&snapshot, market).unwrap()).unwrap()
}

#[test]
fn live_projection_payload_is_independent_of_ledger_and_history_size() {
    let mut committed = project_json(&sync(), &BTreeMap::new(), projection_time()).unwrap();
    let before = live(&committed, &BTreeMap::new());
    let transaction = committed["transactions"][0].clone();
    assert!(transaction.is_object());
    committed["transactions"] = json!(vec![transaction; 10000]);
    committed["netWorthHistory"] = json!(vec![json!({"date": "2026-01-01", "value": 1}); 10000]);
    let after = live(&committed, &BTreeMap::new());
    assert_eq!(before, after);
    assert!(after.get("transactions").is_none());
    assert!(after.get("netWorthHistory").is_none());
}

#[test]
fn projects_exact_totals_and_keeps_posted_and_authorized_dates() {
    let snapshot = project_json(
        &sync(),
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    assert_eq!(snapshot["netWorth"], 300.22);
    assert_eq!(snapshot["transactions"][0]["date"], "2026-09-07");
    assert_eq!(snapshot["transactions"][0]["postedOn"], "2026-09-08");
    assert_eq!(
        snapshot["transactions"][0]["location"]["city"],
        "San Francisco"
    );
    assert_eq!(
        snapshot["transactions"][0]["location"]["postalCode"],
        "94103"
    );
    assert_eq!(snapshot["transactions"][0]["paymentChannel"], "in store");
    assert_eq!(
        snapshot["transactions"][0]["logoUrl"],
        "https://plaid-merchant-logos.plaid.com/coffee.png"
    );
    assert_eq!(snapshot["transactions"][0]["logoName"], "Coffee");
    assert_eq!(
        snapshot["transactions"][0]["categoryDetail"],
        "FOOD_AND_DRINK_COFFEE"
    );
    assert_eq!(
        snapshot["transactions"][0]["categoryConfidence"],
        "VERY_HIGH"
    );
    assert_eq!(
        snapshot["transactions"][0]["counterpartyType"],
        "payment_app"
    );
    assert_eq!(
        snapshot["transactions"][0]["transactionCode"],
        "digital_payment"
    );
    assert_eq!(
        snapshot["accounts"][1]["balanceFetchedAt"],
        "2026-09-09T07:29:00Z"
    );
    assert!(snapshot["accounts"][1]["balanceAsOf"].is_null());
    assert_eq!(
        snapshot["brokeragePerformance"][1]["historySource"],
        "provider-estimated"
    );
    assert!(snapshot["brokeragePerformance"][1]["points"][0]["netDeposits"].is_null());
    assert_eq!(snapshot["provenance"]["calculation"], "rust");
}

#[test]
fn card_statement_details_attach_only_to_matching_credit_accounts() {
    let mut sync = sync();
    sync.plaid.accounts.push(json!({
        "account_id": "card", "name": "Card", "institution_name": "Bank", "type": "credit",
        "balance_mode": "cached",
        "balances": { "current": "40.00", "iso_currency_code": "USD" }
    }));
    sync.plaid.liabilities = vec![
        json!({
            "account_id": "card",
            "aprs": [
                { "apr_type": "cash_apr", "apr_percentage": 29.99 },
                { "apr_type": "purchase_apr", "apr_percentage": 21.24 }
            ],
            "is_overdue": false,
            "last_statement_issue_date": "2026-08-25",
            "last_statement_balance": "123.45",
            "minimum_payment_amount": "35",
            "next_payment_due_date": "2026-09-19",
            "last_payment_date": "2026-08-20",
            "last_payment_amount": "100.00"
        }),
        json!({ "account_id": "cash", "last_statement_issue_date": "2026-08-25" }),
    ];
    let snapshot = project_json(
        &sync,
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    let account = |id: &str| {
        snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|account| account["id"] == id)
            .unwrap()
            .clone()
    };
    let card = account("plaid:card");
    assert_eq!(card["balanceMode"], "cached");
    assert_eq!(card["value"], -40.0);
    assert_eq!(card["liability"]["lastStatementDate"], "2026-08-25");
    assert_eq!(card["liability"]["lastStatementBalance"], 123.45);
    assert_eq!(card["liability"]["minimumPayment"], 35.0);
    assert_eq!(card["liability"]["nextPaymentDueDate"], "2026-09-19");
    assert_eq!(card["liability"]["purchaseApr"], 21.24);
    assert_eq!(card["liability"]["isOverdue"], false);
    assert!(account("plaid:cash").get("liability").is_none());
}

#[test]
fn plaid_location_skips_empty_fields_and_absent_places() {
    let location = PlaidLocation {
        address: Some("  ".into()),
        city: None,
        region: Some(" CA ".into()),
        postal_code: None,
        country: None,
        lat: Some(37.77),
        lon: None,
        store_number: None,
    };
    let projected = location.project().unwrap();
    assert_eq!(projected.region.as_deref(), Some("CA"));
    assert!(projected.address.is_none());
    // A coordinate without its pair, or off the globe, is no location.
    assert!(projected.lat.is_none());
    assert!(PlaidLocation {
        address: None,
        city: Some("  ".into()),
        region: None,
        postal_code: None,
        country: None,
        lat: Some(120.0),
        lon: Some(10.0),
        store_number: Some(" ".into()),
    }
    .project()
    .is_none());
    let pinned = PlaidLocation {
        address: None,
        city: None,
        region: None,
        postal_code: None,
        country: None,
        lat: Some(37.77),
        lon: Some(-122.42),
        store_number: Some("118".into()),
    }
    .project()
    .unwrap();
    assert_eq!((pinned.lat, pinned.lon), (Some(37.77), Some(-122.42)));
    assert_eq!(pinned.store_number.as_deref(), Some("118"));
}

#[test]
fn projects_activity_level_investment_fees_as_separate_transactions() {
    let mut input = sync();
    input.snaptrade.activities.insert(
        "brokerage".into(),
        vec![json!({
            "id": "deposit",
            "type": "CONTRIBUTION",
            "amount": 100,
            "fee": 1.25,
            "trade_date": "2026-09-08",
            "external_reference_id": "provider-reference"
        })],
    );
    let snapshot = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    let fee = snapshot["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|transaction| transaction["id"] == "snaptrade:brokerage:deposit:fee")
        .unwrap();

    assert_eq!(fee["category"], "Fees");
    assert_eq!(fee["amount"], -1.25);
    assert_eq!(fee["description"], "provider-reference");
}

#[test]
fn fractional_quantities_and_unit_prices_keep_fixed_scale_precision() {
    let mut sync = sync();
    sync.plaid.investment_accounts.push(json!({
        "account_id": "fractional", "name": "Fractional", "institution_name": "Broker",
        "type": "investment", "subtype": "brokerage", "balances": { "current": "0.15", "iso_currency_code": "USD" }
    }));
    sync.plaid.holdings.push(json!({
        "account_id": "fractional", "security_id": "security", "quantity": "0.00012345",
        "institution_price": "1234.56789123", "institution_value": "0.15", "iso_currency_code": "USD"
    }));
    sync.plaid.securities.push(json!({
        "security_id": "security", "ticker_symbol": "TEST", "name": "Test", "type": "equity", "iso_currency_code": "USD"
    }));
    let snapshot = project_json(&sync, &BTreeMap::new(), projection_time()).unwrap();
    let holding = snapshot["holdings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|holding| holding["ticker"] == "TEST")
        .unwrap();
    assert_eq!(holding["shares"], 0.00012345);
    assert_eq!(holding["price"], 1234.56789123);
    assert_eq!(holding["value"], 0.15);
}

#[test]
fn annotations_are_applied_by_the_native_view_projection() {
    let mut snapshot = project_json(
        &sync(),
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    snapshot["updatedAt"] = "2026-10-01T00:30:00Z".into();
    snapshot["calendarDate"] = "2026-09-30".into();
    let annotations = BTreeMap::from([(
        "coffee".into(),
        Annotation {
            category: Some("Travel".into()),
            reviewed: Some(true),
            benefit_confirmed: Some(true),
            ..Default::default()
        },
    )]);
    let view = |annotations: &BTreeMap<String, Annotation>| {
        let mut typed: Snapshot = serde_json::from_value(snapshot.clone()).unwrap();
        apply_annotations(&mut typed, annotations, &[]).unwrap();
        serde_json::to_value(typed).unwrap()
    };
    let projected = view(&annotations);
    assert_eq!(projected["transactions"][0]["category"], "Travel");
    assert_eq!(projected["transactions"][0]["benefitConfirmed"], true);
    assert_eq!(projected["spending"]["monthTotal"], 5.13);
    assert_eq!(projected["spending"]["categories"][0]["name"], "Travel");
    let mut transfer = annotations;
    transfer.get_mut("coffee").unwrap().category = Some("Transfer".into());
    let reclassified = view(&transfer);
    assert_eq!(
        reclassified["transactions"][0]["classification"]["kind"],
        "transfer"
    );
    assert_eq!(reclassified["spending"]["monthTotal"], 0.0);
}

#[test]
fn possible_duplicates_are_never_silently_removed() {
    let mut sync = sync();
    sync.plaid.investment_accounts.push(json!({
        "account_id": "plan", "name": "Stock Plan", "institution_name": "Broker", "type": "investment", "subtype": "stock plan", "mask": "1234",
        "balances": { "current": 50, "iso_currency_code": "USD" }
    }));
    sync.snaptrade.accounts[0]["number"] = "XXXX1234".into();
    let snapshot = project_json(&sync, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(
        snapshot["possibleDuplicateAccounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(snapshot["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|account| account["id"] == "plaid:plan"));

    let snapshot = project_json(
        &sync,
        &BTreeMap::from([("plaid:plan".into(), "snaptrade:brokerage".into())]),
        projection_time(),
    )
    .unwrap();
    assert!(!snapshot["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|account| account["id"] == "plaid:plan"));
}

#[test]
fn complete_history_applies_off_date_cash_transfers_to_comparisons() {
    let mut sync = sync();
    sync.snaptrade.history_complete = true;
    sync.snaptrade.activity_complete = true;
    sync.snaptrade.activities.insert(
        "brokerage".into(),
        vec![json!({
            "id": "deposit", "type": "TRANSFER", "amount": 10,
            "trade_date": "2026-09-09"
        })],
    );
    sync.benchmark_history = json!([
        { "date": "2026-09-08", "value": 100 },
        { "date": "2026-09-10", "value": 101 }
    ]);

    let snapshot = project_json(
        &sync,
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-10T07:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    let account = &snapshot["brokeragePerformance"][1];
    assert_eq!(account["performanceMethod"], "time-weighted");
    assert_eq!(account["points"][1]["netDeposits"], 200.0);
    assert_eq!(account["points"][1]["sp500"], 201.9);
    assert_eq!(account["points"][1]["marketChange"], 0.11);
    assert_eq!(account["points"][1]["marketChangePct"], 0.0579);
    let transfer = snapshot["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|transaction| transaction["category"] == "Transfer")
        .unwrap();
    assert_eq!(transfer["merchant"], "Transfer in");
    assert_eq!(transfer["amount"], 10.0);
    assert_eq!(transfer["accountId"], "snaptrade:brokerage");
}

#[test]
fn projects_brokerage_income_and_known_unrealized_gain() {
    let mut sync = sync();
    sync.snaptrade.positions.insert(
        "brokerage".into(),
        vec![json!({
            "units": 2.0049, "price": 100, "cost_basis": 75.0611003, "currency": "USD",
            "instrument": { "kind": "stock", "symbol": "TEST", "description": "Test Security" }
        })],
    );
    sync.snaptrade
        .positions_as_of
        .insert("brokerage".into(), Some("2026-09-09T07:30:00Z".into()));
    sync.snaptrade.activity_complete = true;
    sync.snaptrade.activities.insert(
        "brokerage".into(),
        vec![
            json!({
                "id": "purchase", "type": "BUY", "amount": 225,
                "trade_date": "2025-12-01", "units": 3, "price": 75,
                "symbol": { "symbol": "TEST" }
            }),
            json!({
                "id": "dividend", "type": "DIVIDEND", "amount": 12.34,
                "trade_date": "2026-06-01", "settlement_date": "2026-06-02",
                "symbol": { "symbol": "TEST" }
            }),
            json!({
                "id": "sale", "type": "SELL", "amount": 180,
                "trade_date": "2026-07-01", "units": 1, "price": 180,
                "symbol": { "symbol": "TEST" }
            }),
            json!({
                "id": "reinvestment", "type": "DIVIDEND", "amount": -0.5,
                "trade_date": "2026-08-01", "units": 0.005, "price": 100,
                "description": "Test Security dividend reinvestment",
                "symbol": { "symbol": "TEST" }
            }),
        ],
    );

    let snapshot = project_json(
        &sync,
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    let account = snapshot["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "snaptrade:brokerage")
        .unwrap();
    assert_eq!(account["knownCostBasis"], 150.49);
    assert_eq!(account["knownUnrealizedGain"], 50.0);
    assert_eq!(account["knownUnrealizedGainPct"], 33.2248);
    assert_eq!(account["investmentIncomeYtd"], 12.34);
    assert_eq!(account["salesYtd"], 1);
    assert_eq!(account["saleProceedsYtd"], 180.0);
    assert_eq!(account["estimatedRealizedGainYtd"], 105.0);
    assert_eq!(account["realizedGainCoverage"], "complete");
    assert_eq!(account["costBasisCoverage"], "complete");
    assert_eq!(snapshot["holdings"][0]["unrealizedGain"], 50.0);
    assert!(snapshot["holdings"][0]["marketAsOf"].is_null());
    assert_eq!(
        snapshot["trades"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|trade| trade["type"] == "REINVESTMENT")
            .count(),
        1
    );
    let dividend = snapshot["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|transaction| transaction["merchant"] == "Dividend · TEST")
        .unwrap();
    assert_eq!(dividend["postedOn"], "2026-06-02");
    let sale = snapshot["trades"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trade| trade["type"] == "SELL")
        .unwrap();
    assert_eq!(sale["realizedCostBasis"], 75.0);
    assert_eq!(sale["estimatedRealizedGain"], 105.0);
    assert_eq!(sale["estimatedRealizedGainPct"], 140.0);
    assert_eq!(sale["realizedGainMethod"], "estimated-fifo");
}

#[test]
fn matching_portfolios_offer_a_duplicate_candidate_without_provider_identity() {
    let mut sync = sync();
    sync.plaid.investment_accounts.push(json!({
        "account_id": "other-brokerage", "name": "Investments", "institution_name": "Provider A", "type": "investment", "subtype": "brokerage",
        "balances": { "current": 200, "iso_currency_code": "USD" }
    }));
    sync.plaid.holdings.push(json!({
        "account_id": "other-brokerage", "security_id": "test-security", "quantity": 2,
        "institution_price": 100, "institution_value": 200, "iso_currency_code": "USD"
    }));
    sync.plaid.securities.push(json!({
        "security_id": "test-security", "ticker_symbol": "TEST", "name": "Test Security", "type": "equity", "iso_currency_code": "USD"
    }));
    sync.snaptrade.positions.insert(
        "brokerage".into(),
        vec![json!({
            "units": 2, "price": 100, "currency": "USD",
            "instrument": { "kind": "stock", "symbol": "TEST", "description": "Test Security" }
        })],
    );

    let snapshot = project_json(&sync, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(
        snapshot["possibleDuplicateAccounts"][0]["plaidAccountId"],
        "plaid:other-brokerage"
    );
    assert_eq!(
        snapshot["possibleDuplicateAccounts"][0]["snaptradeAccountId"],
        "snaptrade:brokerage"
    );
}

#[test]
fn malformed_optional_records_are_quarantined() {
    let mut sync = sync();
    sync.plaid
        .transactions
        .push(json!({ "not": "a transaction" }));
    let snapshot = project_json(&sync, &BTreeMap::new(), projection_time()).unwrap();
    assert!(snapshot["syncWarnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning.as_str().unwrap().contains("malformed")));
    assert_eq!(snapshot["transactions"].as_array().unwrap().len(), 1);
}

#[test]
fn currency_isolation_marks_totals_incomplete_without_conversion() {
    let mut sync = sync();
    sync.plaid.accounts.push(json!({
        "account_id": "cad", "name": "Canadian", "institution_name": "Bank", "type": "depository",
        "balances": { "current": 999, "iso_currency_code": "CAD" }
    }));
    let snapshot = project_json(&sync, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(snapshot["netWorth"], 300.22);
    assert_eq!(snapshot["netWorthIncomplete"], true);
    assert!(snapshot["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|account| { account["id"] == "plaid:cad" && account["value"].is_null() }));
}

#[test]
fn projection_is_independent_of_provider_record_order() {
    let mut first = sync();
    first.plaid.accounts.push(json!({
        "account_id": "another", "name": "Savings", "institution_name": "Bank", "type": "depository",
        "balances": { "current": 25, "iso_currency_code": "USD" }
    }));
    first.plaid.transactions.push(json!({
        "transaction_id": "breakfast", "account_id": "cash", "amount": 3, "name": "Breakfast",
        "pending": false, "date": "2026-09-08"
    }));
    let mut second = first.clone();
    second.plaid.accounts.reverse();
    second.plaid.transactions.reverse();
    let now = DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let first = project_json(&first, &BTreeMap::new(), now).unwrap();
    let second = project_json(&second, &BTreeMap::new(), now).unwrap();
    assert_eq!(first["accounts"], second["accounts"]);
    assert_eq!(first["transactions"], second["transactions"]);
    assert_eq!(first["netWorth"], second["netWorth"]);
}

#[test]
fn live_quotes_use_price_freshness_not_position_freshness() {
    let mut committed = project_json(&sync(), &BTreeMap::new(), projection_time()).unwrap();
    committed["accounts"][1]["positionsAsOf"] = "2026-09-09T16:00:00Z".into();
    committed["holdings"] = json!([{
        "ticker": "ONE", "name": "Synthetic security", "color": "#000000",
        "accountId": "plaid:cash", "shares": 2, "price": 50,
        "value": 100, "costBasis": 80, "quoteEligible": true
    }]);
    let old = BTreeMap::from([(
        "ONE".into(),
        MarketSnapshot {
            symbol: "ONE".into(),
            price: 60.0,
            previous_close: 55.0,
            previous_close_as_of: Some("2026-09-08T20:00:00Z".into()),
            daily_change_pct: 9.09,
            weekly_change_pct: Some(20.0),
            weekly_reference_price: Some(50.0),
            weekly_reference_date: Some("2026-09-02".into()),
            as_of: "2026-09-09T15:00:00Z".into(),
        },
    )]);
    let projected = live(&committed, &old);
    assert_eq!(projected["netWorth"], 320.22);
    assert_eq!(projected["holdings"][0]["value"], 120.0);
    assert_eq!(
        projected["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|account| account["id"] == "plaid:cash")
            .unwrap()["value"],
        120.11
    );
    assert_eq!(projected["holdings"][0]["dailyChangePct"], 9.09);
    assert_eq!(projected["holdings"][0]["weeklyChangePct"], 20.0);

    committed["holdings"][0]["marketAsOf"] = "2026-09-09T16:00:00Z".into();
    assert_eq!(live(&committed, &old)["netWorth"], committed["netWorth"]);
    assert_eq!(historical(&committed, &old)["netWorth"], 320.22);

    let mut fresh = old;
    fresh.get_mut("ONE").unwrap().as_of = "2026-09-09T17:00:00Z".into();
    let projected = live(&committed, &fresh);
    assert_eq!(projected["netWorth"], 320.22);
    assert_eq!(projected["holdings"][0]["weeklyReferencePrice"], 50.0);
    assert_eq!(
        projected["holdings"][0]["weeklyReferenceDate"],
        "2026-09-02"
    );
    assert_eq!(committed["netWorth"], 300.22);

    // A combined valuation must include accounts lacking an individual history row.
    committed["holdings"][0]["accountId"] = "snaptrade:brokerage".into();
    committed["brokeragePerformance"] = json!([{
        "accountId": "total", "name": "Total", "institution": "Synthetic",
        "currentValue": 100.0, "points": []
    }]);
    let snap_before = committed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "snaptrade:brokerage")
        .unwrap();
    let cash_before = snap_before["cashValue"].as_f64().unwrap();
    let invested_before = snap_before["investedValue"].as_f64().unwrap();
    let projected = live(&committed, &fresh);
    assert_eq!(projected["brokeragePerformance"][0]["currentValue"], 120.0);
    let snap_after = projected["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "snaptrade:brokerage")
        .unwrap();
    assert_eq!(snap_after["cashValue"], cash_before);
    assert_eq!(snap_after["investedValue"], invested_before + 20.0);
}

mod history;
mod investments;
