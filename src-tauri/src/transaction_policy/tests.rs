use super::*;
use serde_json::{json, Value};

/// The policy over JSON rows, as the renderer reads them: the rows are rewritten classified.
fn apply(
    rows: &mut [Value],
    accounts: &[Value],
    annotations: &BTreeMap<String, Annotation>,
    rules: &[CategoryRule],
) -> Result<(), String> {
    let mut transactions = rows
        .iter()
        .map(|row| serde_json::from_value::<Transaction>(row.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Invalid saved transaction")?;
    let names = accounts
        .iter()
        .filter_map(|account| Some((account["id"].as_str()?, account["name"].as_str()?)));
    super::apply(&mut transactions, names, annotations, rules);
    for (row, transaction) in rows.iter_mut().zip(&transactions) {
        *row = serde_json::to_value(transaction).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn transaction(merchant: &str, category: &str, amount: f64) -> Transaction {
    serde_json::from_value(
        json!({"id":"synthetic", "merchant":merchant, "category":category,
        "amount":amount, "account":"Example", "date":"2026-09-18", "pending":false}),
    )
    .unwrap()
}

#[test]
fn financial_policy_handles_overrides_signs_and_reimbursements() {
    for (merchant, category, amount, mark, kind, spending) in [
        (
            "Zelle receipt",
            "Transfer",
            100.0,
            Mark::Income,
            Kind::Income,
            false,
        ),
        (
            "Zelle payment",
            "Transfer",
            -100.0,
            Mark::Transfer,
            Kind::Transfer,
            false,
        ),
        (
            "Zelle receipt",
            "Reimbursement",
            100.0,
            Mark::Income,
            Kind::Income,
            false,
        ),
        (
            "INTEREST",
            "Income",
            10.0,
            Mark::Interest,
            Kind::Interest,
            false,
        ),
        (
            "Dividend TEST",
            "Income",
            10.0,
            Mark::Dividend,
            Kind::Dividend,
            false,
        ),
        (
            "Example payroll",
            "Income",
            10.0,
            Mark::Income,
            Kind::Income,
            false,
        ),
        (
            "Airline fee reimbursement",
            "Bank Fees",
            25.0,
            Mark::Refund,
            Kind::Reimbursement,
            false,
        ),
        ("Annual fee", "Bank Fees", -25.0, Mark::Fee, Kind::Fee, true),
        (
            "Bank fee reversal",
            "Fees",
            4.0,
            Mark::Refund,
            Kind::Fee,
            false,
        ),
        (
            "Tax withholding",
            "Other",
            -10.0,
            Mark::Initial,
            Kind::Tax,
            true,
        ),
        ("Cafe", "Dining", -20.0, Mark::Initial, Kind::Expense, true),
        ("Cafe", "Dining", 0.0, Mark::Initial, Kind::Other, false),
        (
            "AUTOPAY PAYMENT",
            "Loan Payments",
            -20.0,
            Mark::Payment,
            Kind::Transfer,
            false,
        ),
        (
            "Transfer money from a brokerage account",
            "Transfer",
            500.0,
            Mark::Transfer,
            Kind::Transfer,
            false,
        ),
        (
            "Shared expense repayment",
            "Other",
            20.0,
            Mark::Initial,
            Kind::Reimbursement,
            false,
        ),
        (
            "Synthetic lender",
            "Loan Disbursements",
            1_000.0,
            Mark::Transfer,
            Kind::Transfer,
            false,
        ),
    ] {
        let result = classify(&transaction(merchant, category, amount));
        assert_eq!(
            (result.mark, result.kind, result.spending),
            (mark, kind, spending),
            "{merchant}"
        );
    }
    let mut t = transaction("Example", "Other", 10.0);
    t.description = Some("ZELLE receipt".into());
    assert_eq!(classify(&t).kind, Kind::Income);
    t.description = Some("NotZelle receipt".into());
    assert!(!classify(&t).zelle);
}

#[test]
fn payment_app_outflows_are_spending_unless_recategorized() {
    // The engine already files Plaid's payment-app outflows under the people category;
    // a Zelle payment Plaid calls an account transfer is converted here.
    let mut venmo = transaction("Venmo", PEOPLE_CATEGORY, -42.0);
    venmo.category_detail = Some("TRANSFER_OUT_TRANSFER_OUT_FROM_APPS".into());
    let mut zelle = transaction("Example Person", "Transfers", -60.0);
    zelle.category_detail = Some("TRANSFER_OUT_ACCOUNT_TRANSFER".into());
    zelle.description = Some("Zelle payment to Example".into());
    let mut own = transaction("Savings", "Transfers", -500.0);
    own.category_detail = Some("TRANSFER_OUT_ACCOUNT_TRANSFER".into());
    let mut values = [venmo, zelle, own]
        .map(|t| serde_json::to_value(t).unwrap())
        .to_vec();
    for (index, value) in values.iter_mut().enumerate() {
        value["id"] = format!("t{index}").into();
    }
    let mut manual = values.clone();
    apply(&mut values, &[], &BTreeMap::new(), &[]).unwrap();
    for value in &values[..2] {
        assert_eq!(value["category"], PEOPLE_CATEGORY);
        assert_eq!(value["classification"]["kind"], "expense");
        assert_eq!(value["classification"]["spending"], true);
    }
    assert_eq!(values[1]["providerCategory"], "Transfers");
    assert_eq!(values[1]["subcategory"], "Zelle");
    assert_eq!(values[2]["classification"]["kind"], "transfer");
    // Choosing Transfer by hand keeps an own-account move out of spending.
    let annotations = BTreeMap::from([(
        "t0".into(),
        Annotation {
            category: Some("Transfer".into()),
            ..Default::default()
        },
    )]);
    apply(&mut manual, &[], &annotations, &[]).unwrap();
    assert_eq!(manual[0]["classification"]["kind"], "transfer");
}

#[test]
fn an_own_transfer_filed_as_income_or_spending_is_a_transfer() {
    let leg = |id: &str, account: &str, date: &str, amount: f64, category: &str, code: &str| {
        let mut t = transaction("Instant payment", category, amount);
        t.category_detail = Some(code.into());
        let mut value = serde_json::to_value(t).unwrap();
        value["id"] = id.into();
        value["accountId"] = account.into();
        value["date"] = date.into();
        value
    };
    let mut values = vec![
        leg(
            "sent",
            "plaid:checking",
            "2026-05-04",
            -250.0,
            "Transfers",
            "TRANSFER_OUT_ACCOUNT_TRANSFER",
        ),
        leg(
            "arrived",
            "plaid:brokerage-cash",
            "2026-05-02",
            250.0,
            "Income",
            "INCOME_OTHER_INCOME",
        ),
        // A paycheck has no matching leg from another account, so it stays income.
        leg(
            "paycheck",
            "plaid:checking",
            "2026-05-03",
            250.0,
            "Income",
            "INCOME_WAGES",
        ),
        // One sent leg pairs once, with the nearest arrival: this one stays income.
        leg(
            "again",
            "plaid:savings",
            "2026-05-01",
            250.0,
            "Income",
            "INCOME_OTHER_INCOME",
        ),
        // A move out of brokerage cash Plaid filed as spending is the other half of a
        // transfer that arrived elsewhere.
        leg(
            "moved",
            "plaid:brokerage-cash",
            "2026-08-13",
            -400.0,
            "Other",
            "GENERAL_SERVICES_OTHER_GENERAL_SERVICES",
        ),
        leg(
            "received",
            "plaid:checking",
            "2026-08-13",
            400.0,
            "Transfers",
            "TRANSFER_IN_ACCOUNT_TRANSFER",
        ),
    ];
    let mut ruled = values.clone();
    apply(&mut values, &[], &BTreeMap::new(), &[]).unwrap();
    assert_eq!(values[1]["classification"]["kind"], "transfer");
    assert_eq!(values[1]["category"], "Transfers");
    assert_eq!(values[1]["providerCategory"], "Income");
    assert_eq!(values[2]["classification"]["kind"], "income");
    assert_eq!(values[3]["classification"]["kind"], "income");
    assert_eq!(values[4]["classification"]["kind"], "transfer");
    assert_eq!(values[4]["classification"]["spending"], false);
    assert_eq!(values[4]["providerCategory"], "Other");

    // A category chosen by hand keeps its meaning.
    let annotations = BTreeMap::from([(
        "arrived".into(),
        Annotation {
            category: Some("Income".into()),
            ..Default::default()
        },
    )]);
    apply(&mut ruled, &[], &annotations, &[]).unwrap();
    assert_eq!(ruled[1]["classification"]["kind"], "income");
}

#[test]
fn someone_elses_money_is_neither_spending_nor_income() {
    let mut purchase = transaction("Pharmacy", "Medical", -45.0);
    purchase.id = "purchase".into();
    let mut refund = transaction("Pharmacy refund", "Medical", 5.0);
    refund.id = "refund".into();
    let mut repayment = transaction("Jane Example", "Income", 40.0);
    repayment.id = "repayment".into();
    repayment.description = Some("Zelle payment from Jane Example".into());
    let mut mine = repayment.clone();
    mine.id = "mine".into();
    let mut values = [purchase, refund, repayment, mine]
        .map(|t| serde_json::to_value(t).unwrap())
        .to_vec();
    let other = Annotation {
        other: Some(true),
        ..Default::default()
    };
    let annotations = BTreeMap::from([
        ("purchase".into(), other.clone()),
        (
            "refund".into(),
            Annotation {
                refund_of: Some("purchase".into()),
                ..Default::default()
            },
        ),
        ("repayment".into(), other),
    ]);
    apply(&mut values, &[], &annotations, &[]).unwrap();
    let [purchase, refund, repayment, mine] = values.as_slice() else {
        unreachable!()
    };
    assert_eq!(purchase["other"], true);
    assert_eq!(purchase["classification"]["kind"], "owed");
    assert_eq!(purchase["classification"]["spending"], false);
    assert_eq!(purchase["category"], "Medical", "the category is kept");
    // A refund of that purchase is theirs too, not a cut in your spending.
    assert_eq!(refund["other"], true);
    assert_eq!(refund["classification"]["kind"], "owed");
    // A Zelle paying you back is not income.
    assert_eq!(repayment["classification"]["kind"], "owed");
    assert_eq!(repayment["classification"]["mark"], "transfer");
    // Everything else stays yours.
    assert!(mine.get("other").is_none());
    assert_eq!(mine["classification"]["kind"], "income");
}

#[test]
fn plaid_codes_decide_meaning_and_labels_follow_the_category() {
    let classified = |category: &str, detail: &str, amount: f64| {
        let mut t = transaction("Example", category, amount);
        t.category_detail = Some(detail.into());
        classify(&t)
    };
    let interest = classified("Income", "INCOME_INTEREST_EARNED", 3.0);
    assert_eq!(
        (interest.mark, interest.kind),
        (Mark::Interest, Kind::Interest)
    );
    let card = classified("Loan Payments", "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT", -200.0);
    assert_eq!(
        (card.mark, card.kind, card.spending),
        (Mark::Payment, Kind::Transfer, false)
    );
    let tax = classified("Taxes", "GOVERNMENT_AND_NON_PROFIT_TAX_PAYMENT", -90.0);
    assert_eq!((tax.kind, tax.spending), (Kind::Tax, true));
    let coffee = classified("Dining", "FOOD_AND_DRINK_COFFEE", -5.0);
    assert_eq!((coffee.kind, coffee.spending), (Kind::Expense, true));
    // A refund in a spending category falls back to the text policy.
    let mut refund = transaction("Example refund", "Dining", 5.0);
    refund.category_detail = Some("FOOD_AND_DRINK_RESTAURANT".into());
    assert_eq!(classify(&refund).mark, Mark::Refund);

    let mut value = serde_json::to_value(transaction("Cafe", "Dining", -5.0)).unwrap();
    value["categoryDetail"] = "FOOD_AND_DRINK_COFFEE".into();
    value["subcategory"] = "Coffee".into();
    let mut values = vec![value];
    let annotations = BTreeMap::from([(
        "synthetic".into(),
        Annotation {
            category: Some("Travel".into()),
            ..Default::default()
        },
    )]);
    apply(&mut values, &[], &annotations, &[]).unwrap();
    assert!(values[0].get("subcategory").is_none());
    assert_eq!(values[0]["classification"]["kind"], "expense");
}

#[test]
fn issuer_statement_credits_are_credits_whatever_plaid_files_them_under() {
    let classified = |merchant: &str, category: &str, detail: &str, amount: f64| {
        let mut t = transaction(merchant, category, amount);
        t.category_detail = Some(detail.into());
        classify(&t)
    };
    for (merchant, category, detail) in [
        (
            "AMEX CLEAR PLUS CREDIT",
            "Transfers",
            "TRANSFER_IN_OTHER_TRANSFER_IN",
        ),
        (
            "AMEX Airline Fee Reimbursement",
            "Fees",
            "BANK_FEES_OTHER_BANK_FEES",
        ),
        ("Platinum Hotel Credit", "Travel", "TRAVEL_LODGING"),
        ("Platinum Future Perk Credit", "Other", "OTHER_OTHER"),
    ] {
        let credit = classified(merchant, category, detail, 25.0);
        assert_eq!(
            (credit.credit, credit.spending, credit.kind),
            (true, false, Kind::Reimbursement),
            "{merchant}"
        );
    }
    // A reversal is a charge, and an ordinary transfer in stays a transfer.
    let reversal = classified("Platinum Hotel Credit", "Travel", "TRAVEL_LODGING", -25.0);
    assert!(!reversal.credit);
    let transfer = classified(
        "Savings",
        "Transfers",
        "TRANSFER_IN_OTHER_TRANSFER_IN",
        25.0,
    );
    assert_eq!((transfer.kind, transfer.credit), (Kind::Transfer, false));
}

#[test]
fn posted_transactions_keep_annotations_made_while_pending() {
    let mut posted = serde_json::to_value(transaction("Cafe", "Dining", -20.0)).unwrap();
    posted["id"] = "plaid:posted".into();
    posted["pendingTransactionId"] = "plaid:pending".into();
    let mut values = vec![posted];
    let annotations = BTreeMap::from([(
        "plaid:pending".into(),
        Annotation {
            category: Some("Travel".into()),
            reviewed: Some(true),
            ..Default::default()
        },
    )]);
    apply(&mut values, &[], &annotations, &[]).unwrap();
    assert_eq!(values[0]["category"], "Travel");
    assert_eq!(values[0]["reviewed"], true);
}

#[test]
fn annotations_reclassify_and_legacy_identity_requires_unique_name() {
    let original = serde_json::to_value(transaction("Cafe", "Dining", -20.0)).unwrap();
    let mut values = vec![original.clone()];
    let accounts = vec![json!({"id":"one", "name":"Example"})];
    let annotations = BTreeMap::from([(
        "synthetic".into(),
        Annotation {
            category: Some("Transfer".into()),
            ..Default::default()
        },
    )]);
    apply(&mut values, &accounts, &annotations, &[]).unwrap();
    assert_eq!(values[0]["classification"]["kind"], "transfer");
    assert_eq!(values[0]["classification"]["spending"], false);
    assert_eq!(values[0]["accountId"], "one");
    let mut values = vec![original];
    apply(
        &mut values,
        &[accounts[0].clone(), json!({"id":"two", "name":"Example"})],
        &BTreeMap::new(),
        &[],
    )
    .unwrap();
    assert!(values[0].get("accountId").is_none());
    assert_eq!(values[0]["classification"]["kind"], "expense");
}

#[test]
fn browser_seed_contains_native_transaction_policy() {
    let seed: Value = serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap();
    let mut projected = seed.clone();
    let accounts = seed["accounts"].as_array().unwrap();
    apply(
        projected["transactions"].as_array_mut().unwrap(),
        accounts,
        &BTreeMap::new(),
        &[],
    )
    .unwrap();
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/data/seed.json"),
            format!("{}\n", serde_json::to_string_pretty(&projected).unwrap()),
        )
        .unwrap();
    } else {
        assert_eq!(
            projected, seed,
            "Regenerate the synthetic browser seed with BRIEF_UPDATE_CONTRACT_FIXTURE=1"
        );
    }
}

#[test]
fn tags_splits_and_refund_links_apply_at_read_time_without_changing_saved_rows() {
    let transaction = |id: &str, merchant: &str, category: &str, amount: f64| {
        json!({
            "id": id, "merchant": merchant, "category": category, "date": "2026-09-10",
            "amount": amount, "account": "Card", "accountId": "plaid:card", "pending": false,
        })
    };
    let mut values = vec![
        transaction("trip", "Example Market", "Groceries", -100.0),
        transaction("stale", "Example Market", "Groceries", -50.0),
        transaction("back", "Example Market Return", "Transfer In", 30.0),
        transaction("kept", "Example Market Return", "Transfer In", 10.0),
    ];
    let split = |category: &str, amount: f64| crate::storage::Split {
        category: category.into(),
        amount,
    };
    let annotations = BTreeMap::from([
        (
            "trip".to_owned(),
            Annotation {
                tags: Some(vec!["party".into()]),
                splits: Some(vec![split("Groceries", -70.0), split("Gifts", -30.0)]),
                ..Default::default()
            },
        ),
        (
            // Saved for another amount, so it no longer divides this transaction.
            "stale".to_owned(),
            Annotation {
                splits: Some(vec![split("Groceries", -20.0), split("Gifts", -20.0)]),
                ..Default::default()
            },
        ),
        (
            "back".to_owned(),
            Annotation {
                refund_of: Some("trip".into()),
                ..Default::default()
            },
        ),
        (
            // A category chosen by hand outranks the purchase's.
            "kept".to_owned(),
            Annotation {
                category: Some("Reimbursed".into()),
                refund_of: Some("trip".into()),
                ..Default::default()
            },
        ),
    ]);
    // Each read applies annotations to a copy of the committed rows.
    let committed = values.clone();
    apply(&mut values, &[], &annotations, &[]).unwrap();
    assert_eq!(values[0]["tags"], json!(["party"]));
    assert_eq!(
        values[0]["splits"][1],
        json!({ "category": "Gifts", "amount": -30.0 })
    );
    assert_eq!(values[0]["amount"], -100.0);
    assert!(values[1].get("splits").is_none());
    assert_eq!(values[2]["refundOf"], "trip");
    assert_eq!(values[2]["category"], "Groceries");
    assert_eq!(values[2]["providerCategory"], "Transfer In");
    assert_eq!(values[2]["classification"]["kind"], "reimbursement");
    assert_eq!(values[2]["classification"]["spending"], false);
    assert_eq!(values[3]["category"], "Reimbursed");
    assert_eq!(values[3]["classification"]["kind"], "reimbursement");

    // Without the annotations the same committed rows read as the provider reported them.
    let mut values = committed;
    apply(&mut values, &[], &BTreeMap::new(), &[]).unwrap();
    assert!(values[0].get("tags").is_none() && values[0].get("splits").is_none());
    assert!(values[2].get("refundOf").is_none());
    assert_eq!(values[2]["category"], "Transfer In");
}
