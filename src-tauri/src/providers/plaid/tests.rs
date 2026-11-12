use super::sync::{
    allowed_plaid_account, balance_endpoint, billed_endpoints, merge_plaid_data,
    plaid_history_start, validate_plaid_page, CACHED_BALANCES, INVESTMENT_TRANSACTIONS,
    LIABILITIES, LIVE_BALANCES, RECURRING,
};
use super::*;

/// AGENTS.md: never call billed Plaid endpoints on an ordinary or background refresh. The
/// background refresh asks for no live balances, and default settings turn on no product.
#[test]
fn an_ordinary_or_background_refresh_calls_no_billed_plaid_endpoint() {
    let settings = crate::settings::FinanceSettings::default();
    for options in [
        PlaidSyncOptions::default(),
        crate::refresh::plaid_options(&settings, false),
    ] {
        assert!(billed_endpoints(options).is_empty(), "{options:?}");
        assert_eq!(balance_endpoint(options), CACHED_BALANCES);
    }
    // Each billed product turns on its own endpoint and no other.
    for (options, endpoint) in [
        (
            PlaidSyncOptions {
                live_balances: true,
                ..Default::default()
            },
            LIVE_BALANCES,
        ),
        (
            PlaidSyncOptions {
                recurring: true,
                ..Default::default()
            },
            RECURRING,
        ),
        (
            PlaidSyncOptions {
                liabilities: true,
                ..Default::default()
            },
            LIABILITIES,
        ),
        (
            PlaidSyncOptions {
                investment_transactions: true,
                ..Default::default()
            },
            INVESTMENT_TRANSACTIONS,
        ),
    ] {
        assert_eq!(billed_endpoints(options), BTreeSet::from([endpoint]));
    }
    assert_eq!(
        balance_endpoint(PlaidSyncOptions {
            live_balances: true,
            ..Default::default()
        }),
        LIVE_BALANCES
    );
}

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

/// One change Plaid reports to a ledger.
#[derive(Clone)]
enum Event {
    Added(Value),
    Modified(Value),
    Removed(String),
}

/// A generated ledger's events: `count` rows added, then some amended, then some removed, in
/// that order, as Plaid sends a ledger's history. The same seed gives the same events.
fn ledger_events(seed: u64, count: usize) -> Vec<Event> {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let row = |id: usize, cents: u64, name: &str| {
        json!({
            "transaction_id": format!("txn-{seed}-{id}"),
            "account_id": "account-0000",
            "amount": cents as f64 / 100.0,
            "name": name,
            "date": "2026-10-01",
        })
    };
    let mut events = (0..count)
        .map(|id| Event::Added(row(id, 100 + next() % 10_000, "Example store")))
        .collect::<Vec<_>>();
    for id in 0..count {
        if next() % 3 == 0 {
            events.push(Event::Modified(row(
                id,
                100 + next() % 10_000,
                "Example store, posted",
            )));
        }
    }
    for id in 0..count {
        if next() % 4 == 0 {
            events.push(Event::Removed(format!("txn-{seed}-{id}")));
        }
    }
    events
}

/// `events` in pages of `size`, each naming the next cursor after `prefix`, the last one
/// `{prefix}end`.
fn pages(events: &[Event], size: usize, prefix: &str) -> Vec<Value> {
    let chunks = events.chunks(size.max(1)).collect::<Vec<_>>();
    let last = chunks.len().max(1);
    (0..last)
        .map(|index| {
            let chunk = chunks.get(index).copied().unwrap_or_default();
            let pick = |want: fn(&Event) -> Option<Value>| chunk.iter().filter_map(want).collect::<Vec<_>>();
            json!({
                "added": pick(|event| match event { Event::Added(row) => Some(row.clone()), _ => None }),
                "modified": pick(|event| match event { Event::Modified(row) => Some(row.clone()), _ => None }),
                "removed": pick(|event| match event {
                    Event::Removed(id) => Some(json!({ "transaction_id": id })),
                    _ => None,
                }),
                "next_cursor": if index + 1 == last { format!("{prefix}end") } else { format!("{prefix}{}", index + 1) },
                "has_more": index + 1 < last,
                "transactions_update_status": "HISTORICAL_UPDATE_COMPLETE",
            })
        })
        .collect()
}

/// Serves `/transactions/sync` from scripted walks: each attempt's pages, keyed by the cursor
/// asked for, and Plaid's mutation error on the calls listed in `refuse`.
struct Script {
    attempts: Vec<(Option<String>, String, Vec<Value>)>,
    attempt: usize,
    calls: usize,
    refuse: Vec<usize>,
}

impl Script {
    // The reader's signature is the one `walk_with_restarts` takes.
    #[allow(clippy::result_large_err)]
    fn read(&mut self, body: Value) -> Result<(Value, usize), ProviderError> {
        self.calls += 1;
        if self.refuse.contains(&self.calls) {
            self.attempt += 1;
            return Err(ProviderError::refused(
                "Plaid",
                "/transactions/sync",
                "TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION",
            ));
        }
        let (start, prefix, pages) = &self.attempts[self.attempt.min(self.attempts.len() - 1)];
        let cursor = body["cursor"].as_str().map(str::to_owned);
        let index = if cursor == *start {
            0
        } else {
            cursor
                .as_deref()
                .and_then(|cursor| cursor.strip_prefix(prefix.as_str()))
                .and_then(|page| page.parse::<usize>().ok())
                .expect("a cursor this walk handed out")
        };
        Ok((pages[index].clone(), 0))
    }
}

fn walk(
    cursor: Option<String>,
    saved: BTreeMap<String, Value>,
    script: &mut Script,
) -> super::sync::SyncedTransactions {
    let script = std::cell::RefCell::new(script);
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(super::sync::walk_with_restarts(
            cursor,
            saved,
            "access-sandbox-0000",
            |body| std::future::ready(script.borrow_mut().read(body)),
        ))
        .unwrap()
}

#[test]
fn a_paged_cursor_walk_with_restarts_ends_where_one_page_does() {
    for seed in 1..=8 {
        let events = ledger_events(seed, 30 + seed as usize * 7);
        // The whole ledger in one page, from no cursor: the reference.
        let single = walk(
            None,
            BTreeMap::new(),
            &mut Script {
                attempts: vec![(None, "one-".into(), pages(&events, events.len(), "one-"))],
                attempt: 0,
                calls: 0,
                refuse: vec![],
            },
        );
        assert_eq!(single.requests, 1);

        // The same ledger in two syncs of small pages. The first sync stops partway through
        // the history; the second resumes from its cursor and is told, mid-walk, that the
        // ledger changed, so it starts over from that cursor and finishes.
        let split = events.len() / 3;
        let size = 1 + (seed as usize % 5);
        let first = walk(
            None,
            BTreeMap::new(),
            &mut Script {
                attempts: vec![(None, "a-".into(), pages(&events[..split], size, "a-"))],
                attempt: 0,
                calls: 0,
                refuse: vec![],
            },
        );
        assert_eq!(first.cursor, "a-end");
        let rest = &events[split..];
        // The interrupted walk's first page adds a row the changed ledger no longer has; none
        // of what it sent may survive the restart.
        let mut interrupted = vec![Event::Added(json!({
            "transaction_id": "txn-withdrawn", "account_id": "account-0000", "amount": 5.0,
            "name": "Example hold", "date": "2026-10-01",
        }))];
        interrupted.extend_from_slice(rest);
        let mut script = Script {
            attempts: vec![
                (
                    Some("a-end".into()),
                    "b-".into(),
                    pages(&interrupted, size, "b-"),
                ),
                (
                    Some("a-end".into()),
                    "c-".into(),
                    pages(rest, size + 1, "c-"),
                ),
            ],
            attempt: 0,
            calls: 0,
            refuse: vec![3],
        };
        let resumed = walk(Some(first.cursor), first.transactions, &mut script);
        assert_eq!(resumed.cursor, "c-end", "seed {seed}");
        assert_eq!(
            resumed.transactions, single.transactions,
            "seed {seed}: the paged walk and the single page disagree"
        );
        assert_eq!(resumed.requests, script.calls);
        // Removed rows are gone, amended ones carry their latest version.
        for event in &events {
            if let Event::Removed(id) = event {
                assert!(!resumed.transactions.contains_key(id));
            }
        }
    }
}

#[test]
fn a_ledger_that_keeps_changing_fails_the_walk_after_its_attempts() {
    let events = ledger_events(9, 12);
    let mut script = Script {
        attempts: vec![(None, "a-".into(), pages(&events, 2, "a-"))],
        attempt: 0,
        calls: 0,
        refuse: vec![2, 4, 6],
    };
    let script = std::cell::RefCell::new(&mut script);
    let result = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(super::sync::walk_with_restarts(
            None,
            BTreeMap::new(),
            "access-sandbox-0000",
            |body| std::future::ready(script.borrow_mut().read(body)),
        ));
    assert_eq!(
        result
            .err()
            .and_then(|error| error.code().map(str::to_owned))
            .as_deref(),
        Some("TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION")
    );
}
