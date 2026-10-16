use super::sync::{
    allowed_plaid_account, merge_plaid_data, plaid_history_start, validate_plaid_page,
};
use super::*;

#[test]
fn plaid_rejects_cursor_cycles_and_unbounded_pages_but_accepts_noop_completion() {
    let mut seen = BTreeSet::from(["saved".into()]);
    assert!(validate_plaid_page("next", true, 0, &mut seen).is_ok());
    assert!(validate_plaid_page("saved", true, 1, &mut seen).is_err());
    assert!(validate_plaid_page("last", true, 99, &mut seen).is_err());
    assert!(validate_plaid_page("saved", false, 0, &mut seen).is_ok());
    assert!(validate_plaid_page("last", false, 99, &mut seen).is_ok());
}

#[test]
fn link_results_require_current_credentials_and_an_active_session() {
    let providers = Providers::new().unwrap();
    let (cancel, _) = tokio::sync::watch::channel(false);
    let mut session = PendingSession {
        link: PendingLink::SnapTrade {
            existing_connection_ids: BTreeSet::from(["existing".into()]),
        },
        generation: 0,
        expires: Instant::now() + StdDuration::from_secs(300),
        polling: false,
        cancel,
    };
    assert!(validate_link_session(&session, 0).is_ok());
    assert!(validate_link_session(&session, 1).is_err());
    session.expires = Instant::now() - StdDuration::from_secs(1);
    assert!(validate_link_session(&session, 0).is_err());
    assert!(providers
        .mutate_link_vault("cancelled", |_| panic!("Cancelled session must not mutate"))
        .is_err());
    assert!(!plaid_link_completed(
        &json!({"link_sessions": [{"finished_at": null, "on_success": {"public_token": "synthetic"}}]})
    ));
    assert!(plaid_link_completed(
        &json!({"link_sessions": [{"finished_at": "2026-09-01T12:00:00Z", "events": [{"event_name": "HANDOFF"}]}]})
    ));
}

#[test]
fn partial_plaid_merge_retains_unrelated_cached_accounts() {
    let mut old = PlaidData {
        accounts: vec![
            json!({"account_id": "bank", "value": 100}),
            json!({"account_id": "broken", "value": 50}),
        ],
        transactions: vec![
            json!({"account_id": "bank", "transaction_id": "removed"}),
            json!({"account_id": "broken", "transaction_id": "saved"}),
        ],
        transaction_history_start: BTreeMap::from([
            ("bank".into(), "2026-01-01".into()),
            ("broken".into(), "2026-02-01".into()),
        ]),
        ..Default::default()
    };
    merge_plaid_data(
        &mut old,
        PlaidData {
            accounts: vec![json!({"account_id": "bank", "value": 110})],
            transactions: vec![json!({"account_id": "bank", "transaction_id": "new"})],
            transaction_history_start: BTreeMap::from([("bank".into(), "2026-03-01".into())]),
            ..Default::default()
        },
    );
    assert_eq!(old.accounts.len(), 2);
    assert_eq!(old.accounts[0]["value"], 50);
    assert_eq!(old.accounts[1]["value"], 110);
    assert_eq!(
        old.transactions
            .iter()
            .map(|value| value["transaction_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["saved", "new"]
    );
    assert_eq!(
        old.transaction_history_start,
        BTreeMap::from([
            ("bank".into(), "2026-03-01".into()),
            ("broken".into(), "2026-02-01".into()),
        ])
    );
}

#[test]
fn plaid_imports_supported_cash_and_card_accounts() {
    for (kind, subtype) in [
        ("depository", "checking"),
        ("depository", "cash management"),
        ("depository", "money market"),
        ("credit", "credit card"),
    ] {
        assert!(allowed_plaid_account(
            &json!({ "type": kind, "subtype": subtype })
        ));
    }
    for (kind, subtype) in [("investment", "brokerage"), ("loan", "mortgage")] {
        assert!(!allowed_plaid_account(
            &json!({ "type": kind, "subtype": subtype })
        ));
    }
}

#[test]
fn plaid_uses_new_user_ids_and_supports_legacy_user_tokens() {
    let new_user = PlaidUser {
        client_id: "client".into(),
        user_id: Some("usr_123".into()),
        user_token: None,
    };
    assert_eq!(
        plaid_user_identifier(&new_user).unwrap(),
        ("user_id", "usr_123")
    );

    let legacy_user = PlaidUser {
        client_id: "client".into(),
        user_id: Some("legacy-id".into()),
        user_token: Some("user-production-123".into()),
    };
    assert_eq!(
        plaid_user_identifier(&legacy_user).unwrap(),
        ("user_token", "user-production-123")
    );
}

#[test]
fn plaid_history_claims_the_window_only_after_the_historical_pull() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
    assert_eq!(
        plaid_history_start(false, Some("2024-10-06".into()), today),
        None
    );
    assert_eq!(
        plaid_history_start(true, None, today),
        Some("2024-10-06".into())
    );
    assert_eq!(
        plaid_history_start(true, Some("2024-09-29".into()), today),
        Some("2024-09-29".into())
    );
}

#[test]
fn plaid_link_exit_reports_error_codes_only_after_every_session_ends() {
    assert_eq!(
        plaid_link_exit(&json!({"link_sessions": [{
            "finished_at": "2026-09-01T12:00:00Z",
            "exit": { "error": {
                "error_code": "INSTITUTION_NOT_RESPONDING",
                "error_message": "synthetic free text"
            }}
        }]})),
        Some("Plaid could not connect (INSTITUTION_NOT_RESPONDING)".into())
    );
    assert_eq!(
        plaid_link_exit(&json!({"link_sessions": [{
            "finished_at": "2026-09-01T12:00:00Z",
            "on_exit": { "error": null, "metadata": { "status": "OAUTH" } }
        }]})),
        Some("Plaid Link closed before connecting".into())
    );
    assert_eq!(
        plaid_link_exit(&json!({"link_sessions": [
            { "finished_at": "2026-09-01T12:00:00Z", "exit": { "error": null } },
            { "finished_at": null }
        ]})),
        None
    );
    assert_eq!(plaid_link_exit(&json!({"link_sessions": []})), None);
}

#[test]
fn link_requests_only_consent_to_liabilities_and_repairs_keep_the_items_products() {
    let banks = plaid_link_request(false, None);
    assert_eq!(banks["products"], json!(["transactions"]));
    assert_eq!(
        banks["additional_consented_products"],
        json!(["liabilities"])
    );
    assert_eq!(banks["transactions"]["days_requested"], 730);
    let investments = plaid_link_request(true, None);
    assert_eq!(investments["products"], json!(["investments"]));
    assert!(investments.get("additional_consented_products").is_none());
    let repair = plaid_link_request(false, Some("access-synthetic".into()));
    assert_eq!(repair["access_token"], "access-synthetic");
    for field in [
        "products",
        "enable_multi_item_link",
        "account_filters",
        "transactions",
    ] {
        assert!(repair.get(field).is_none(), "{field}");
    }
    assert_eq!(
        repair["additional_consented_products"],
        json!(["liabilities"])
    );
}

#[test]
fn extracts_multi_item_hosted_link_tokens() {
    let tokens = plaid_public_tokens(&json!({
        "link_sessions": [{
            "results": { "item_add_results": [
                { "public_token": "public-b" },
                { "public_token": "public-a" }
            ]}
        }]
    }));
    assert_eq!(tokens, vec!["public-a", "public-b"]);
}

#[test]
fn statements_are_offered_only_for_bank_accounts_at_institutions_that_support_them() {
    use super::sync::institution_supports;
    let institution = |products: Value| json!({ "institution": { "products": products } });
    assert!(institution_supports(
        &institution(json!(["transactions", "statements"])),
        "statements"
    ));
    assert!(!institution_supports(
        &institution(json!(["transactions", "liabilities"])),
        "statements"
    ));
    assert!(!institution_supports(&json!({}), "statements"));

    let item = |investments: bool, statements: bool| PlaidItem {
        item_id: "item".into(),
        access_token: "access-synthetic".into(),
        institution_id: None,
        institution_name: None,
        investments,
        statements,
    };
    let cache = |supported: Option<bool>, account_type: &str| PlaidItemCache {
        statements_supported: supported,
        data: Some(PlaidData {
            accounts: vec![json!({ "account_id": "a", "type": account_type })],
            ..PlaidData::default()
        }),
        ..PlaidItemCache::default()
    };
    // A checking account at a bank that supports Statements can add them.
    assert_eq!(
        statements_state(&item(false, false), Some(&cache(Some(true), "depository"))),
        Some(false)
    );
    // A card-only connection, or a bank without Statements, is offered nothing.
    assert_eq!(
        statements_state(&item(false, false), Some(&cache(Some(true), "credit"))),
        None
    );
    assert_eq!(
        statements_state(&item(false, false), Some(&cache(Some(false), "depository"))),
        None
    );
    assert_eq!(
        statements_state(&item(false, false), Some(&cache(None, "depository"))),
        None
    );
    assert_eq!(statements_state(&item(false, false), None), None);
    // Investments never are; added statements stay shown.
    assert_eq!(
        statements_state(&item(true, false), Some(&cache(Some(true), "depository"))),
        None
    );
    assert_eq!(statements_state(&item(false, true), None), Some(true));
}
