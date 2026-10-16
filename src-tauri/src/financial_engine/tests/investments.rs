//! Trades, lots, splits, dividends, and investment activity.
use super::*;

/// A trade of TEST in account `a`.
fn trade(id: &str, kind: &str, date: &str, amount: f64, units: f64) -> Trade {
    Trade {
        id: id.into(),
        r#type: kind.into(),
        date: date.into(),
        amount,
        account_id: "a".into(),
        ticker: Some("TEST".into()),
        units: Some(units),
        ..Trade::default()
    }
}

/// A position in account `a`.
fn position(ticker: &str, shares: f64, cost_basis: Option<f64>) -> Holding {
    Holding {
        account_id: "a".into(),
        ticker: ticker.into(),
        shares: Some(shares),
        cost_basis,
        ..Holding::default()
    }
}

#[test]
fn realized_gains_split_into_short_and_long_term_by_each_lots_holding_period() {
    let mut trades = vec![
        trade("old", "BUY", "2025-03-10", -500.0, 5.0),
        trade("new", "BUY", "2026-01-15", -800.0, 5.0),
        // The old lot passes its anniversary on 2026-03-10, so a sale that day is short-term
        // and one the next day is long-term.
        trade("edge", "SELL", "2026-03-10", 150.0, 1.0),
        trade("mixed", "SELL", "2026-03-11", 1_200.0, 6.0),
    ];
    let holdings = [position("TEST", 3.0, Some(480.0))];
    add_estimated_realized_gains(&mut trades, &holdings, &|_: &str| true);
    assert_eq!(trades[2].estimated_realized_gain, Some(50.0));
    assert_eq!(trades[2].estimated_short_term_gain, Some(50.0));
    assert_eq!(trades[2].estimated_long_term_gain, Some(0.0));
    // Four old shares at $100 sell long-term for $800; two new ones at $160 short-term for $400.
    assert_eq!(trades[3].estimated_realized_gain, Some(480.0));
    assert_eq!(trades[3].estimated_long_term_gain, Some(400.0));
    assert_eq!(trades[3].estimated_short_term_gain, Some(80.0));
}

#[test]
fn restates_pre_split_trades_only_when_that_explains_the_shares_held() {
    let splits = splits(&[
        serde_json::from_value::<CorporateAction>(json!({ "kind": "split", "symbol": "TEST", "exDate": "2026-06-10", "oldRate": 1, "newRate": 4 })).unwrap(),
    ]);
    let trade = |id: &str, kind: &str, date: &str, units: f64| Trade {
        price: Some(100.0),
        ..trade(id, kind, date, units * 100.0, units)
    };
    // Recorded in pre-split shares: 10 bought, then 8 post-split shares sold, 32 held.
    let mut recorded = vec![
        trade("buy", "BUY", "2026-01-05", 10.0),
        trade("sell", "SELL", "2026-07-01", 8.0),
    ];
    let held = [position("TEST", 32.0, None)];
    adjust_for_splits(&mut recorded, &held, &splits);
    assert_eq!(recorded[0].units, Some(40.0));
    assert_eq!(recorded[0].price, Some(25.0));
    assert_eq!(recorded[0].split_ratio, Some(4.0));
    assert!(recorded[1].split_ratio.is_none());

    // A broker that already restated its history is left alone.
    let mut restated = vec![
        trade("buy", "BUY", "2026-01-05", 40.0),
        trade("sell", "SELL", "2026-07-01", 8.0),
    ];
    adjust_for_splits(&mut restated, &held, &splits);
    assert_eq!(restated[0].units, Some(40.0));
    assert!(restated[0].split_ratio.is_none());
}

#[test]
fn keeps_recent_and_announced_dividends_for_held_symbols_only() {
    let actions = [
        serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "TEST", "exDate": "2026-08-01", "payableDate": "2026-08-10", "rate": 0.5, "special": false, "currency": "USD" })).unwrap(),
        serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "TEST", "exDate": "2024-01-01", "payableDate": "2024-01-10", "rate": 0.5, "special": false, "currency": "USD" })).unwrap(),
        serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "TEST", "exDate": "2026-08-01", "payableDate": null, "rate": 0.4, "special": false, "currency": "CAD" })).unwrap(),
        serde_json::from_value::<CorporateAction>(json!({ "kind": "dividend", "symbol": "OTHER", "exDate": "2026-08-01", "payableDate": "2026-08-10", "rate": 1, "special": false, "currency": "USD" })).unwrap(),
    ];
    let holdings = [position("test", 1.0, None)];
    let kept = dividend_history(
        &actions,
        &holdings,
        NaiveDate::from_ymd_opt(2026, 9, 9).unwrap(),
    );
    assert_eq!(
        serde_json::to_value(kept).unwrap(),
        json!([
            { "symbol": "TEST", "exDate": "2026-08-01", "payableDate": "2026-08-10", "rate": 0.5, "special": false }
        ])
    );
}

#[test]
fn maps_plaid_investment_activity_to_trades_and_income_rows() {
    let mut input = sync();
    input.plaid.investment_accounts.push(json!({
        "account_id": "ira", "name": "IRA", "institution_name": "Broker", "type": "investment", "subtype": "ira",
        "balances": { "current": "1000", "iso_currency_code": "USD" }
    }));
    input.plaid.securities.push(json!({ "security_id": "s", "ticker_symbol": "TEST", "name": "Test", "type": "equity", "iso_currency_code": "USD" }));
    let record = |id: &str, kind: &str, subtype: &str, amount: f64, quantity: f64| {
        json!({
            "investment_transaction_id": id, "account_id": "ira", "security_id": "s", "date": "2026-09-01",
            "name": id, "quantity": quantity, "amount": amount, "price": 10, "fees": 0, "type": kind, "subtype": subtype,
            "iso_currency_code": "USD"
        })
    };
    input.plaid.investment_transactions = vec![
        record("buy", "buy", "buy", 100.0, 10.0),
        record("drip", "buy", "dividend reinvestment", 5.0, 0.5),
        record("sell", "sell", "sell", -60.0, -6.0),
        record("qualified", "cash", "qualified dividend", -5.0, 0.0),
        record("distribution", "cash", "long-term capital gain", -7.0, 0.0),
        record("fee", "fee", "account fee", 2.0, 0.0),
        record("contribution", "cash", "contribution", -500.0, 0.0),
        record("mistake", "buy", "buy", 50.0, 5.0),
        json!({ "investment_transaction_id": "undo", "account_id": "ira", "security_id": "s", "date": "2026-09-02",
                "amount": -50, "type": "cancel", "subtype": "cancel", "cancel_transaction_id": "mistake", "iso_currency_code": "USD" }),
        json!({ "investment_transaction_id": "euro", "account_id": "ira", "security_id": "s", "date": "2026-09-02",
                "amount": -9, "type": "cash", "subtype": "dividend", "iso_currency_code": "EUR" }),
    ];
    let snapshot = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    let trades = snapshot["trades"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|trade| trade["accountId"] == "plaid:ira")
        .map(|trade| {
            (
                trade["id"].as_str().unwrap(),
                trade["type"].as_str().unwrap(),
                trade["amount"].as_f64().unwrap(),
                trade["units"].as_f64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        trades,
        [
            ("plaid-investment:buy", "BUY", -100.0, 10.0),
            ("plaid-investment:drip", "REINVESTMENT", -5.0, 0.5),
            ("plaid-investment:sell", "SELL", 60.0, 6.0),
        ]
    );
    let rows = snapshot["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|transaction| transaction["accountId"] == "plaid:ira")
        .map(|transaction| {
            (
                transaction["merchant"].as_str().unwrap().to_owned(),
                transaction["amount"].as_f64().unwrap(),
                transaction
                    .get("dividendType")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 4);
    assert!(rows.contains(&("Dividend · TEST".into(), 5.0, Some("qualified".into()))));
    assert!(rows.contains(&(
        "Capital gain distribution · TEST".into(),
        7.0,
        Some("qualified".into())
    )));
    assert!(rows.contains(&("Investment fee".into(), -2.0, None)));
    assert!(rows.contains(&("Transfer in".into(), 500.0, None)));
}

#[test]
fn provider_numbers_in_exponent_form_are_still_decimals() {
    assert_eq!(decimal_value(&json!(1e-7)), Some(Decimal::new(1, 7)));
    assert_eq!(decimal_value(&json!("2.5e-3")), Some(Decimal::new(25, 4)));
    assert_eq!(decimal_value(&json!(12.34)), Some(Decimal::new(1234, 2)));
    assert_eq!(decimal_value(&json!("not a number")), None);
}
