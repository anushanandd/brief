//! Net worth and account history reconstruction, and the statements that extend it.
use super::*;

#[test]
fn stock_plan_history_includes_zero_before_the_first_acquisition() {
    assert_eq!(
        holding_history(
            vec![Acquisition {
                date: "2026-09-08".into(),
                quantity: Decimal::from(2),
            }],
            &[
                ("2026-09-07".into(), Decimal::from(10)),
                ("2026-09-08".into(), Decimal::from(11)),
            ],
        ),
        vec![
            ("2026-09-07".into(), Decimal::ZERO),
            ("2026-09-08".into(), Decimal::from(22)),
        ]
    );
}

#[test]
fn reconstructs_net_worth_from_cash_transactions_and_investment_history() {
    let mut sync = sync();
    sync.plaid
        .transaction_history_start
        .insert("cash".into(), "2026-09-07".into());
    sync.snaptrade.balance_history.insert(
        "brokerage".into(),
        vec![
            json!({ "date": "2026-09-07", "total_value": 180 }),
            json!({ "date": "2026-09-08", "total_value": 190 }),
        ],
    );
    let snapshot = project_json(
        &sync,
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    assert_eq!(
        snapshot["netWorthHistory"],
        json!([
            { "date": "2026-09-07", "value": 285.24 },
            { "date": "2026-09-08", "value": 290.11 },
            { "date": "2026-09-09", "value": 300.22 }
        ])
    );
    assert_eq!(
        snapshot["accountBalanceHistory"][0],
        json!({
            "accountId": "plaid:cash",
            "name": "Checking",
            "institution": "Bank",
            "currentValue": 100.11,
            "historySource": "transaction-derived",
            "historyStart": "2026-09-07",
            "performanceMethod": "value-only",
            "points": [
                { "date": "2026-09-07", "value": 105.24, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                { "date": "2026-09-08", "value": 100.11, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                { "date": "2026-09-09", "value": 100.11, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null }
            ]
        })
    );
    assert_eq!(
        snapshot["observedNetWorthHistory"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(snapshot["netWorthHistoryEstimated"], true);
}

#[test]
fn cash_and_complete_positions_replace_a_lagging_reported_total() {
    let mut input = sync();
    input.snaptrade.accounts[0]["balance"]["total"]["amount"] = json!(300);
    input.snaptrade.cash_balances.insert(
        "brokerage".into(),
        vec![json!({ "currency": { "code": "USD" }, "cash": 50 })],
    );
    input.snaptrade.positions.insert("brokerage".into(), vec![
        json!({ "units": 2, "price": 100, "currency": "USD", "instrument": { "kind": "stock", "symbol": "TEST" } }),
        json!({ "units": 5, "price": 10, "currency": "USD", "cash_equivalent": true, "instrument": { "kind": "mutualfund", "symbol": "CASH" } }),
    ]);
    let snapshot = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(snapshot["netWorth"], 350.11);
    assert_eq!(snapshot["accounts"][2]["value"], 250.0);
    assert_eq!(snapshot["accounts"][2]["cashValue"], 50.0);
    assert_eq!(snapshot["accounts"][2]["investedValue"], 200.0);
    assert_eq!(
        snapshot["accounts"][2]["balanceSource"],
        "cash-and-positions"
    );

    input.snaptrade.accounts[0]["balance"]["total"]["amount"] = json!(500);
    let discrepancy = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(discrepancy["accounts"][2]["value"], 250.0);
    assert_eq!(discrepancy["accounts"][2]["reportedBalance"], 500.0);

    input.snaptrade.accounts[0]["balance"]["total"] = json!({});
    let without_reported = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(without_reported["accounts"][2]["value"], 250.0);
    assert!(without_reported["accounts"][2]["reportedBalance"].is_null());

    input.snaptrade.positions.get_mut("brokerage").unwrap().push(json!({
        "units": 1, "price": 10, "currency": "USD", "instrument": { "kind": "option", "symbol": "OPT" }
    }));
    let fallback = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(fallback["netWorth"], 100.11);
    assert!(fallback["accounts"][2]["value"].is_null());
    assert!(fallback["accounts"][2]["cashValue"].is_null());
    assert!(fallback["accounts"][2]["investedValue"].is_null());
    assert_eq!(fallback["accounts"][2]["balanceSource"], "unavailable");
    assert_eq!(fallback["netWorthIncomplete"], true);
}

#[test]
fn stale_brokerage_sync_does_not_save_a_false_daily_observation() {
    let mut input = sync();
    input.snaptrade.accounts[0]["sync_status"]["holdings"]["last_successful_sync"] =
        json!("2026-09-08T12:00:00Z");
    let snapshot = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    assert_eq!(snapshot["netWorthProvisional"], true);
    assert_eq!(snapshot["observedNetWorthHistory"], json!([]));
    assert!(snapshot["netWorthHistory"]
        .as_array()
        .unwrap()
        .iter()
        .all(|point| point["date"] != "2026-09-09"));
}

#[test]
fn reconstructs_credit_history_as_a_negative_liability() {
    let mut sync = sync();
    sync.plaid.accounts.push(json!({
        "account_id": "card", "name": "Card", "institution_name": "Bank", "type": "credit",
        "balances": { "current": 250, "iso_currency_code": "USD" }
    }));
    sync.plaid.transactions.push(json!({
        "transaction_id": "purchase", "account_id": "card", "amount": 40,
        "name": "Purchase", "pending": false, "date": "2026-09-08"
    }));
    sync.plaid
        .transaction_history_start
        .insert("card".into(), "2026-09-07".into());

    let snapshot = project_json(
        &sync,
        &BTreeMap::new(),
        DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .unwrap();
    let history = snapshot["accountBalanceHistory"]
        .as_array()
        .unwrap()
        .iter()
        .find(|history| history["accountId"] == "plaid:card")
        .unwrap();
    assert_eq!(history["currentValue"], -250.0);
    assert_eq!(
        history["points"],
        json!([
            { "date": "2026-09-07", "value": -210.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
            { "date": "2026-09-08", "value": -250.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
            { "date": "2026-09-09", "value": -250.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null }
        ])
    );
}

fn point(date: &str, value: f64) -> Point {
    Point {
        date: date.into(),
        value,
    }
}

/// A value-only performance series.
fn series(id: &str, source: &str, points: &[(&str, f64)]) -> Performance {
    Performance {
        account_id: id.into(),
        name: id.into(),
        institution: "Synthetic".into(),
        current_value: 0.0,
        history_start: None,
        performance_method: Some("value-only".into()),
        history_source: Some(source.into()),
        points: points
            .iter()
            .map(|(date, value)| PerformancePoint {
                date: (*date).into(),
                value: *value,
                net_deposits: None,
                sp500: None,
                market_change: None,
                market_change_pct: None,
            })
            .collect(),
    }
}

#[test]
fn transfers_on_skipped_brokerage_days_leave_net_worth_unchanged() {
    let point = |date: &str, value: i64| (date.to_string(), Decimal::from(value));
    // The brokerage reports every other day; the 500 withdrawal lands on a skipped day.
    let brokerage = series(
        "snaptrade:brokerage",
        "provider-estimated",
        &[("2026-09-01", 1000.0), ("2026-09-03", 510.0)],
    );
    let savings = vec![
        point("2026-09-01", 200),
        point("2026-09-02", 700),
        point("2026-09-03", 700),
    ];
    let flows = BTreeMap::from([(
        "snaptrade:brokerage".to_string(),
        BTreeMap::from([("2026-09-02".to_string(), Decimal::from(-500))]),
    )]);

    assert_eq!(
        reconstructed_net_worth_history(
            vec![savings.clone()],
            std::slice::from_ref(&brokerage),
            &flows
        ),
        vec![
            point("2026-09-01", 1200),
            point("2026-09-02", 1200),
            point("2026-09-03", 1210),
        ]
    );
    // Without known flows, the stale brokerage balance double-counts the transfer.
    assert_eq!(
        reconstructed_net_worth_history(vec![savings], &[brokerage], &BTreeMap::new())[1],
        point("2026-09-02", 1700)
    );
}

#[test]
fn totals_start_at_the_earliest_account_history() {
    let point = |date: &str, value: i64| (date.to_string(), Decimal::from(value));
    let checking = vec![
        point("2026-09-01", 100),
        point("2026-09-02", 100),
        point("2026-09-03", 120),
    ];
    // The card's history starts later; it counts from its first point.
    let card = vec![point("2026-09-02", -30), point("2026-09-03", -40)];
    assert_eq!(
        combine_value_histories(&[checking.clone(), card]),
        vec![
            point("2026-09-01", 100),
            point("2026-09-02", 70),
            point("2026-09-03", 80),
        ]
    );
    // Today's value alone says nothing about the past, so there is no total.
    assert!(combine_value_histories(&[checking, vec![point("2026-09-03", -40)]]).is_empty());
}

#[test]
fn combined_brokerages_start_at_the_earliest_account_history() {
    let older = series(
        "snaptrade:older",
        "provider-estimated",
        &[
            ("2026-09-01", 100.0),
            ("2026-09-02", 110.0),
            ("2026-09-03", 120.0),
        ],
    );
    let newer = series(
        "snaptrade:newer",
        "provider-estimated",
        &[("2026-09-02", 50.0), ("2026-09-04", 60.0)],
    );
    let values = |accounts: &[Performance]| {
        combined_performance(accounts)[0]
            .points
            .iter()
            .map(|point| (point.date.clone(), point.value))
            .collect::<Vec<_>>()
    };
    // Each account counts from its first point and carries its last value across skipped days.
    assert_eq!(
        values(&[older.clone(), newer]),
        [
            ("2026-09-01".to_string(), 100.0),
            ("2026-09-02".to_string(), 160.0),
            ("2026-09-03".to_string(), 170.0),
            ("2026-09-04".to_string(), 180.0),
        ]
    );
    // An account without history would enter as a jump, so only shared dates are summed.
    let unknown = series("snaptrade:unknown", "unavailable", &[("2026-09-03", 40.0)]);
    assert_eq!(
        values(&[older, unknown]),
        [("2026-09-03".to_string(), 160.0)]
    );
}

#[test]
fn allocated_shares_count_as_deposits_at_that_days_close() {
    let activity = |value: Value| serde_json::from_value::<SnapActivity>(value).unwrap();
    let activities = [
        activity(json!({
            "type": "TRANSFER", "amount": 0, "units": 10, "trade_date": "2026-06-23",
            "description": "ALLOCATE SHARES FOR 1", "symbol": { "symbol": "EXM" },
        })),
        activity(json!({ "type": "CONTRIBUTION", "amount": 500, "trade_date": "2026-06-24" })),
        // No close for this ticker, so the allocation is left out rather than guessed.
        activity(json!({
            "type": "TRANSFER", "amount": 0, "units": 3, "trade_date": "2026-06-25",
            "description": "Allocate shares for 2", "symbol": { "symbol": "UNPRICED" },
        })),
    ];
    let close = |ticker: &str, date: &str| {
        (ticker == "EXM" && date >= "2026-06-22").then(|| Decimal::new(1_250, 1))
    };
    assert_eq!(
        external_flows(&activities, &close),
        BTreeMap::from([
            ("2026-06-23".to_string(), Decimal::from(1_250)),
            ("2026-06-24".to_string(), Decimal::from(500)),
        ])
    );
}

#[test]
fn flow_points_fill_only_bounded_gaps() {
    let point = |date: &str, value: i64| (date.to_string(), Decimal::from(value));
    let flows = BTreeMap::from([
        ("2026-08-31".to_string(), Decimal::from(50)),
        ("2026-09-01".to_string(), Decimal::from(25)),
        ("2026-09-02".to_string(), Decimal::from(100)),
        ("2026-09-03".to_string(), Decimal::from(-40)),
        ("2026-09-06".to_string(), Decimal::from(10)),
    ]);
    assert_eq!(
        with_flow_points(
            vec![point("2026-09-01", 300), point("2026-09-04", 400)],
            &flows,
        ),
        vec![
            point("2026-09-01", 300),
            point("2026-09-02", 400),
            point("2026-09-03", 360),
            point("2026-09-04", 400),
        ]
    );
}

#[test]
fn transfers_posting_on_different_days_count_once_in_net_worth() {
    // The policy pairs the legs (`transaction_policy` tests which); history reads its pairs.
    let leg =
        |id: &str, pair: Option<&str>, account: &str, posted: &str, amount: f64| Transaction {
            id: id.into(),
            account_id: Some(account.into()),
            date: "2026-09-01".into(),
            posted_on: Some(posted.into()),
            amount,
            transfer_pair_id: pair.map(Into::into),
            ..Transaction::default()
        };
    let transactions = vec![
        // A card payment credits the card two days before the bank debit posts.
        leg("card", Some("bank"), "plaid:card", "2026-09-02", 100.0),
        leg("bank", Some("card"), "plaid:bank", "2026-09-04", -100.0),
        // Same-day legs need nothing, a pair with an untracked account counts nothing early,
        // and an unpaired leg is a movement of its own.
        leg("out", Some("in"), "plaid:bank", "2026-09-05", -40.0),
        leg("in", Some("out"), "snaptrade:brokerage", "2026-09-05", 40.0),
        leg("external", Some("sweep"), "plaid:other", "2026-09-08", 60.0),
        leg("sweep", Some("external"), "plaid:bank", "2026-09-09", -60.0),
        leg("alone", None, "plaid:bank", "2026-09-10", -70.0),
    ];
    let tracked = ["plaid:card", "plaid:bank", "snaptrade:brokerage"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let in_transit = in_transit_transfers(&transactions, &tracked);
    assert_eq!(
        in_transit,
        BTreeMap::from([
            ("2026-09-02".to_string(), Decimal::from(100)),
            ("2026-09-03".to_string(), Decimal::from(100)),
        ])
    );

    let observed = [point("2026-09-03", 1100.0)];
    let estimated = vec![
        ("2026-09-01".to_string(), Decimal::from(1000)),
        ("2026-09-02".to_string(), Decimal::from(1100)),
        ("2026-09-04".to_string(), Decimal::from(1000)),
    ];
    let (history, _) = merged_net_worth_history(estimated, &observed, &in_transit);
    assert!(history.iter().all(|point| point.value == 1000.0));
}

fn statement_file(account_id: &str, csv: &str) -> crate::statements::StatementFile {
    crate::statements::StatementFile {
        id: crate::statements::content_id(csv.as_bytes()),
        account_id: account_id.into(),
        file_name: "statement.csv".into(),
        imported_at: "2026-09-09T11:00:00Z".into(),
        contents: csv.as_bytes().to_vec(),
        confirmed_rows: Default::default(),
    }
}

#[test]
fn statements_fill_plaid_history_before_its_first_transaction() {
    let mut input = sync();
    input.statements = vec![statement_file(
        "plaid:cash",
        "Date,Description,Type,Amount\n\
         09/08/2026,COFFEE,Debit Card,-5.13\n\
         09/01/2026,PAYROLL EXAMPLE,Direct Deposit,100.00\n\
         08/30/2026,ZELLE PAYMENT TO A FRIEND,Zelle Payment,-25.00\n",
    )];
    let value = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    let imported = value["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|transaction| {
            transaction["id"]
                .as_str()
                .unwrap()
                .starts_with("statement:")
        })
        .collect::<Vec<_>>();
    // Plaid reports from 09/08, so the statement's 09/08 purchase isn't added again.
    assert_eq!(imported.len(), 2);
    let payroll = imported
        .iter()
        .find(|row| row["amount"] == json!(100.0))
        .unwrap();
    assert_eq!(payroll["accountId"], "plaid:cash");
    assert_eq!(payroll["account"], "Checking");
    assert_eq!(payroll["category"], "Income");
    assert_eq!(payroll["categoryDetail"], "INCOME_WAGES");
    assert_eq!(payroll["postedOn"], "2026-09-01");
    let zelle = imported
        .iter()
        .find(|row| row["amount"] == json!(-25.0))
        .unwrap();
    assert_eq!(zelle["category"], "Payments To People");
    // The reconstructed balance now reaches back to the statement's first day.
    let history = value["accountBalanceHistory"]
        .as_array()
        .unwrap()
        .iter()
        .find(|history| history["accountId"] == "plaid:cash")
        .unwrap();
    assert_eq!(history["points"][0]["date"], "2026-08-30");
    assert_eq!(history["points"][0]["value"], json!(5.24));
}

#[test]
fn investment_statements_add_trades_and_hide_rows_for_missing_accounts() {
    let mut input = sync();
    input.statements = vec![
        statement_file(
            "snaptrade:brokerage",
            "Activity/Trade Date,Activity Type,Description,Symbol,Quantity #,Price $,Amount $,Commission\n\
             08/14/2026,Qualified Dividend,EXAMPLE CORP,EXMP,,,2.10,0\n\
             08/03/2026,Sold,DEMO HOLDINGS,DEMO,-2,88.10,176.15,0.05\n\
             08/01/2026,Bought,SAMPLE FUND,SMPL,3,41.20,-123.60,0\n",
        ),
        statement_file("plaid:closed", "Date,Description,Amount\n03/02/2026,CAFE,-3.10\n"),
    ];
    let value = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    let trades = value["trades"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|trade| trade["accountId"] == "snaptrade:brokerage")
        .collect::<Vec<_>>();
    assert_eq!(trades.len(), 2);
    let sale = trades.iter().find(|trade| trade["type"] == "SELL").unwrap();
    assert_eq!(sale["units"], json!(2.0));
    assert_eq!(sale["amount"], json!(176.15));
    assert_eq!(sale["ticker"], "DEMO");
    let purchase = trades.iter().find(|trade| trade["type"] == "BUY").unwrap();
    assert_eq!(purchase["amount"], json!(-123.6));
    let transactions = value["transactions"].as_array().unwrap();
    assert!(transactions
        .iter()
        .any(|row| row["merchant"] == "Dividend · EXMP" && row["category"] == "Dividend"));
    assert!(transactions
        .iter()
        .any(|row| row["merchant"] == "Investment fee" && row["amount"] == json!(-0.05)));
    assert!(!transactions
        .iter()
        .any(|row| row["accountId"] == "plaid:closed"));
    assert!(value["syncWarnings"].as_array().unwrap().contains(&json!(
        "Statement history for an unavailable account is hidden"
    )));
}

#[test]
fn history_check_counts_only_material_gaps_between_estimates_and_observations() {
    let estimated = [
        ("2026-09-01".to_owned(), Decimal::from(10_000)),
        ("2026-09-02".to_owned(), Decimal::from(10_150)),
        ("2026-09-03".to_owned(), Decimal::from(12_000)),
        ("2026-09-04".to_owned(), Decimal::from(9_000)),
    ];
    let observed = [
        point("2026-09-01", 10_000.0),
        // $150 is under 2% of the value, so it is ordinary estimation noise.
        point("2026-09-02", 10_000.0),
        point("2026-09-03", 10_000.0),
        point("2026-09-05", 10_000.0),
    ];
    assert_eq!(
        serde_json::to_value(history_check(&estimated, &observed)).unwrap(),
        json!({
            "comparedDays": 3, "divergentDays": 1,
            "largestGap": 2_000.0, "largestGapDate": "2026-09-03"
        })
    );
    assert!(history_check(&estimated, &[]).is_none());
}

#[test]
fn an_observation_that_diverges_from_the_estimate_yields_to_it() {
    // A refresh saw the brokerage receive a transfer the bank hadn't yet sent.
    let estimated = vec![
        ("2026-09-01".to_owned(), Decimal::from(10_000)),
        ("2026-09-02".to_owned(), Decimal::from(10_000)),
        ("2026-09-03".to_owned(), Decimal::from(10_000)),
    ];
    let observed = [
        point("2026-09-01", 10_050.0),
        point("2026-09-02", 28_000.0),
        point("2026-09-04", 10_020.0),
    ];
    let (history, estimated) = merged_net_worth_history(estimated, &observed, &BTreeMap::new());
    assert_eq!(
        history
            .iter()
            .map(|point| (point.date.as_str(), point.value))
            .collect::<Vec<_>>(),
        [
            ("2026-09-01", 10_050.0),
            ("2026-09-02", 10_000.0),
            ("2026-09-03", 10_000.0),
            ("2026-09-04", 10_020.0),
        ]
    );
    assert!(estimated);
}

#[test]
fn an_issuers_merchant_details_reach_the_transactions_its_rows_describe() {
    let mut input = sync();
    input.statements = vec![statement_file(
        "plaid:cash",
        "Date,Description,Type,Amount,Address,City/State,Zip Code,Category\n\
         09/08/2026,COFFEE,Debit Card,-5.13,1 EXAMPLE ST,\"SAN JOSE\nCA\",95110,Restaurant-Bar & Café\n\
         09/01/2026,PAYROLL EXAMPLE,Direct Deposit,100.00,,,,\n\
         08/30/2026,EXAMPLE BOOKS,Debit Card,-12.00,2 SAMPLE AVE,\"DENVER\nCO\",80202,Merchandise & Supplies-Book Stores\n",
    )];
    let value = project_json(&input, &BTreeMap::new(), projection_time()).unwrap();
    let transactions = value["transactions"].as_array().unwrap();
    // Plaid's own purchase, which the statement row describes.
    let coffee = transactions
        .iter()
        .find(|row| {
            row["amount"] == json!(-5.13) && !row["id"].as_str().unwrap().starts_with("statement:")
        })
        .unwrap();
    assert_eq!(coffee["issuer"]["address"], "1 EXAMPLE ST");
    assert_eq!(coffee["issuer"]["category"], "Restaurant-Bar & Café");
    // A row Plaid never reported carries its own.
    let books = transactions
        .iter()
        .find(|row| row["amount"] == json!(-12.0))
        .unwrap();
    assert_eq!(books["issuer"]["city"], "DENVER");
    let payroll = transactions
        .iter()
        .find(|row| row["amount"] == json!(100.0))
        .unwrap();
    assert!(payroll.get("issuer").is_none());
}
