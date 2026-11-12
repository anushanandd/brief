use serde_json::{json, Value};

use super::*;
use crate::insights::calendar::NamedZone;

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

/// A native classification as the renderer's fixture writes one: the kind decides the mark.
fn classification(kind: &str) -> Value {
    let mark = match kind {
        "expense" | "other" | "tax" | "owed" => "initial",
        "reimbursement" => "refund",
        kind => kind,
    };
    json!({
        "kind": kind, "mark": mark, "spending": kind == "expense",
        "credit": kind == "reimbursement", "zelle": false,
    })
}

fn transaction(id: &str, amount: f64, patch: Value) -> Value {
    let mut value = json!({
        "id": id, "amount": amount, "merchant": "Example payroll", "category": "Income",
        "date": "2026-09-03", "accountId": "cash", "account": "Cash",
        "classification": classification("income"), "pending": false, "description": null,
    });
    for (key, field) in patch.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}

/// The empty snapshot saved on Sep 18 with cash, card, and euro accounts.
fn view(transactions: Vec<Value>, trades: Vec<Value>) -> View {
    let mut snapshot: Value =
        serde_json::from_str(include_str!("../../../../src/data/empty.json")).unwrap();
    snapshot["updatedAt"] = json!("2026-09-18T12:00:00Z");
    snapshot["accounts"] = json!([
        { "id": "cash", "name": "Cash", "institution": "Test", "type": "cash", "value": 1000.0, "currency": "USD" },
        { "id": "card", "name": "Card", "institution": "Test", "type": "credit", "value": -100.0, "currency": "USD" },
        { "id": "foreign", "name": "Foreign", "institution": "Test", "type": "cash", "value": 100.0, "currency": "EUR" },
    ]);
    snapshot["transactions"] = json!(transactions);
    snapshot["trades"] = json!(trades);
    View {
        snapshot: serde_json::from_value(snapshot).unwrap(),
        insights: None,
        account_names: Default::default(),
        spending_account_id: Some("card".into()),
    }
}

fn read(view: &View, request: ReportRequest) -> Reports {
    read_on(view, request, "2026-09-18")
}

fn read_on(view: &View, request: ReportRequest, today: &str) -> Reports {
    reports(view, &request, day(today), &NamedZone::Utc).unwrap()
}

fn request(chart: Chart, range: Option<Range>) -> ReportRequest {
    ReportRequest {
        chart: Some(chart),
        range,
        ..Default::default()
    }
}

fn ids(report: &Report) -> Vec<&str> {
    report
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect()
}

#[test]
fn income_counts_posted_usd_income_dated_in_the_window() {
    let view = view(
        vec![
            transaction("pay", 100.1, json!({})),
            transaction(
                "interest",
                0.2,
                json!({ "merchant": "Interest", "classification": classification("interest") }),
            ),
            transaction(
                "dividend",
                0.3,
                json!({ "merchant": "Dividend", "classification": classification("dividend") }),
            ),
            transaction("pending", 100.0, json!({ "pending": true })),
            transaction("foreign", 50.0, json!({ "accountId": "foreign" })),
            transaction("future", 50.0, json!({ "postedOn": "2026-09-19" })),
            transaction("posted-before", 30.0, json!({ "postedOn": "2026-08-31" })),
            transaction(
                "transfer",
                20.0,
                json!({ "merchant": "Transfer", "classification": classification("transfer") }),
            ),
        ],
        vec![],
    );
    let reports = read(
        &view,
        ReportRequest {
            from: Some(day("2026-09-01")),
            to: Some(day("2026-09-18")),
            ..request(Chart::Income, None)
        },
    );
    let report = reports.report.unwrap();
    assert_eq!(report.summary.total, Some(100.6));
    assert_eq!(
        ids(&report),
        ["spending:pay", "spending:interest", "spending:dividend"]
    );
    // The buckets add up to the total.
    assert_eq!(
        total(report.buckets.iter().map(|bucket| &bucket.value)),
        report.summary.total
    );
    // A card filter leaves no income.
    let card = read(
        &view,
        ReportRequest {
            accounts: Some(vec!["card".into()]),
            ..request(Chart::Income, None)
        },
    );
    assert!(card.report.unwrap().entries.is_empty());
}

#[test]
fn a_sale_without_a_cost_leaves_its_total_and_bucket_unavailable() {
    let trade = |id: &str, kind: &str, date: &str, gain: Option<f64>| {
        json!({
            "id": id, "type": kind, "date": date, "account": "Cash", "accountId": "cash",
            "amount": 80.0, "estimatedRealizedGain": gain,
        })
    };
    let view = view(
        vec![],
        vec![
            trade("known", "SELL", "2026-07-02", Some(-20.0)),
            trade("unknown", "SELL", "2026-08-02", None),
            trade("buy", "BUY", "2026-09-01", None),
        ],
    );
    let report = read(
        &view,
        ReportRequest {
            from: Some(day("2026-07-01")),
            to: Some(day("2026-09-18")),
            ..request(Chart::Realized, None)
        },
    )
    .report
    .unwrap();
    assert_eq!(report.summary.total, None);
    assert_eq!(
        report
            .buckets
            .iter()
            .map(|bucket| bucket.value)
            .collect::<Vec<_>>(),
        [Some(-20.0), None, Some(0.0)]
    );
    assert_eq!(report.entries.len(), 2);
    // A month with an unknown sale has no scale to draw.
    assert_eq!(
        (report.buckets[1].positive, report.buckets[1].negative),
        (0.0, 0.0)
    );
}

#[test]
fn fees_net_their_refunds_and_cash_flow_reconciles_with_its_sankey() {
    let view = view(
        vec![
            transaction("pay", 100.0, json!({})),
            transaction(
                "fee",
                -10.0,
                json!({ "merchant": "Bank fee", "classification": classification("fee") }),
            ),
            transaction(
                "reversed-fee",
                4.0,
                json!({ "merchant": "Bank fee reversal", "classification": {
                    "kind": "fee", "mark": "refund", "spending": false, "credit": false, "zelle": false,
                } }),
            ),
            transaction(
                "purchase",
                -20.0,
                json!({ "merchant": "Cafe", "category": "Dining", "classification": classification("expense") }),
            ),
            transaction(
                "repayment",
                30.0,
                json!({ "merchant": "Shared expense repayment", "classification": classification("reimbursement") }),
            ),
        ],
        vec![],
    );
    let fees = read(&view, request(Chart::Fees, None));
    assert_eq!(fees.report.unwrap().summary.total, Some(6.0));
    let cash = read(&view, request(Chart::CashFlow, None));
    assert_eq!(cash.report.unwrap().summary.total, Some(104.0));
    let flow = cash.flow.unwrap();
    // A fee refund reduces fees, and a repayment with no spending to offset counts as income.
    assert_eq!((flow.inflow, flow.outflow), (130.0, 26.0));
    assert_eq!((flow.retained, flow.gap), (104.0, 0.0));
    assert_eq!(
        flow.sources
            .iter()
            .map(|item| (item.label.as_str(), item.value))
            .collect::<Vec<_>>(),
        [("Payroll", 100.0), ("Refunds and repayments", 30.0)]
    );
    assert_eq!(flow.purchases[0].label, "Dining");
    let charges = flow.charges.unwrap();
    assert_eq!(
        (charges.label.as_str(), charges.value),
        ("Fees and taxes", 6.0)
    );
}

#[test]
fn a_refund_nets_out_of_the_category_it_repaid() {
    let view = view(
        vec![
            transaction("pay", 500.0, json!({})),
            transaction(
                "shoes",
                -120.0,
                json!({ "merchant": "Shoe shop", "category": "Shopping", "classification": classification("expense") }),
            ),
            transaction(
                "returned",
                40.0,
                json!({ "merchant": "Shoe shop", "category": "Shopping", "classification": classification("reimbursement") }),
            ),
        ],
        vec![],
    );
    let flow = read(&view, request(Chart::CashFlow, None)).flow.unwrap();
    // The return isn't income: Shopping is what was spent, less what came back.
    assert_eq!(flow.sources.len(), 1);
    assert_eq!(
        (flow.purchases[0].label.as_str(), flow.purchases[0].value),
        ("Shopping", 80.0)
    );
    assert_eq!(
        flow.purchases[0]
            .children
            .iter()
            .map(|child| child.value)
            .collect::<Vec<_>>(),
        [120.0, -40.0]
    );
    assert_eq!(
        (flow.inflow, flow.outflow, flow.retained),
        (500.0, 80.0, 420.0)
    );
    assert!(flow.charges.is_none());
}

#[test]
fn cash_flow_shows_its_largest_categories_and_folds_the_rest_into_one() {
    let mut transactions = vec![transaction("pay", 5_000.0, json!({}))];
    for (index, category) in [
        "Rent",
        "Travel",
        "Dining",
        "Groceries",
        "Shopping",
        "Gifts",
        "Pets",
    ]
    .iter()
    .enumerate()
    {
        transactions.push(transaction(
            &format!("spent-{index}"),
            -(700.0 - index as f64 * 100.0),
            json!({ "merchant": "Example", "category": category, "classification": classification("expense") }),
        ));
    }
    let flow = read(&view(transactions, vec![]), request(Chart::CashFlow, None))
        .flow
        .unwrap();
    let shown = flow
        .purchases
        .iter()
        .map(|item| (item.label.as_str(), item.value))
        .collect::<Vec<_>>();
    assert_eq!(
        shown,
        [
            ("Rent", 700.0),
            ("Travel", 600.0),
            ("Dining", 500.0),
            ("Groceries", 400.0),
            ("Shopping", 300.0),
            ("Other categories", 300.0),
        ]
    );
    let other = flow.purchases.last().unwrap();
    assert_eq!(
        other
            .parts
            .iter()
            .map(|part| (part.label.as_str(), part.value))
            .collect::<Vec<_>>(),
        [("Gifts", 200.0), ("Pets", 100.0)]
    );
    assert_eq!(other.children.len(), 2);
    // Folding moves nothing: everything spent is still out.
    assert_eq!(flow.outflow, 2_800.0);
}

#[test]
fn cash_flow_groups_income_as_the_income_report_does_and_purchases_by_category() {
    let spent = |id: &str, amount: f64, merchant: &str, category: &str| {
        transaction(
            id,
            amount,
            json!({ "merchant": merchant, "category": category, "classification": classification("expense") }),
        )
    };
    let view = view(
        vec![
            transaction("pay-1", 1_500.0, json!({ "merchant": "Acme Payroll" })),
            transaction("pay-2", 500.0, json!({ "merchant": "Acme Payroll" })),
            transaction(
                "interest",
                25.0,
                json!({ "merchant": "Marcus", "classification": classification("interest") }),
            ),
            spent("meal-1", -80.0, "Cafe", "Dining"),
            spent("meal-2", -20.0, "Deli", "Dining"),
            spent("flight", -300.0, "Airline", "Travel"),
        ],
        vec![],
    );
    let flow = read(&view, request(Chart::CashFlow, None)).flow.unwrap();
    let summary = |items: &[FlowItem]| {
        items
            .iter()
            .map(|item| {
                (
                    item.label.clone(),
                    item.value,
                    item.children
                        .iter()
                        .map(|child| (child.id.clone(), child.label.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    let pair = |id: &str, label: &str| (id.to_owned(), label.to_owned());
    assert_eq!(
        summary(&flow.sources),
        [
            (
                "Payroll".to_owned(),
                2_000.0,
                vec![
                    pair("source-transaction:pay-1", "Acme Payroll"),
                    pair("source-transaction:pay-2", "Acme Payroll")
                ]
            ),
            (
                "Interest".to_owned(),
                25.0,
                vec![pair("source-transaction:interest", "Marcus")]
            ),
        ]
    );
    assert_eq!(
        summary(&flow.purchases),
        [
            (
                "Travel".to_owned(),
                300.0,
                vec![pair("purchase-transaction:flight", "Airline")]
            ),
            (
                "Dining".to_owned(),
                100.0,
                vec![
                    pair("purchase-transaction:meal-1", "Cafe"),
                    pair("purchase-transaction:meal-2", "Deli")
                ]
            ),
        ]
    );
}

#[test]
fn a_split_purchase_flows_to_each_of_its_categories() {
    let purchase = |id: &str, amount: f64, patch: Value| {
        let mut value = transaction(
            id,
            amount,
            json!({ "merchant": "Example Market", "category": "Groceries", "classification": classification("expense") }),
        );
        for (key, field) in patch.as_object().unwrap() {
            value[key] = field.clone();
        }
        value
    };
    let view = view(
        vec![
            purchase(
                "split",
                -100.0,
                json!({ "splits": [
                    { "category": "Groceries", "amount": -70.0 },
                    { "category": "Gifts", "amount": -30.0 },
                ] }),
            ),
            purchase("whole", -20.0, json!({})),
        ],
        vec![],
    );
    let flow = read(&view, request(Chart::CashFlow, None)).flow.unwrap();
    assert_eq!(
        flow.purchases
            .iter()
            .map(|item| (item.label.as_str(), item.value, item.children.len()))
            .collect::<Vec<_>>(),
        [("Groceries", 90.0, 2), ("Gifts", 30.0, 1)]
    );
    // More went out than came in.
    assert_eq!((flow.gap, flow.retained), (120.0, 0.0));
}

#[test]
fn a_report_compares_the_window_before_only_when_history_reaches_it() {
    let both = view(
        vec![
            transaction("old", 50.0, json!({ "date": "2026-07-21" })),
            transaction("current", 75.0, json!({})),
        ],
        vec![],
    );
    let month = read(&both, request(Chart::Income, Some(Range::Month)));
    assert_eq!(
        month.window,
        ReportWindow {
            start: day("2026-08-20"),
            end: day("2026-09-18"),
            saved_end: day("2026-09-18"),
            previous_start: day("2026-07-21"),
            previous_end: day("2026-08-19"),
            range: Range::Month,
            custom: false,
            valid: true,
        }
    );
    let summary = month.report.unwrap().summary;
    assert_eq!(
        (
            summary.total,
            summary.previous,
            summary.change,
            summary.percent
        ),
        (Some(75.0), Some(50.0), Some(25.0), Some(50.0))
    );
    let quarter = read(&both, request(Chart::Income, Some(Range::Quarter))).window;
    assert_eq!(
        (quarter.start, quarter.previous_start, quarter.previous_end),
        (day("2026-06-21"), day("2026-03-23"), day("2026-06-20"))
    );
    let week = read(&both, request(Chart::Income, Some(Range::Week))).window;
    assert_eq!(
        (week.start, week.previous_start),
        (day("2026-09-12"), day("2026-09-05"))
    );
    // A year is the trailing 365 days, as on every chart and Home, not the calendar year.
    let year = read(&both, request(Chart::Income, Some(Range::Year))).window;
    assert_eq!(
        (year.start, year.previous_start),
        (day("2025-09-19"), day("2024-09-19"))
    );
    let one = read(&both, request(Chart::Income, Some(Range::Day))).window;
    assert_eq!(
        (one.start, one.previous_start),
        (day("2026-09-18"), day("2026-09-17"))
    );
    // Without the older entry, there is nothing to compare with.
    let current = view(vec![transaction("current", 75.0, json!({}))], vec![]);
    let summary = read(&current, request(Chart::Income, Some(Range::Month)))
        .report
        .unwrap()
        .summary;
    assert_eq!((summary.previous, summary.change), (None, None));
    // Custom days compare with as many days before them; a reversed pair is invalid.
    let custom = read(
        &both,
        ReportRequest {
            from: Some(day("2026-08-10")),
            to: Some(day("2026-08-12")),
            ..request(Chart::Income, None)
        },
    )
    .window;
    assert_eq!(
        (custom.previous_start, custom.previous_end, custom.custom),
        (day("2026-08-07"), day("2026-08-09"), true)
    );
    let reversed = read(
        &both,
        ReportRequest {
            from: Some(day("2026-09-18")),
            to: Some(day("2026-08-01")),
            ..request(Chart::Income, None)
        },
    );
    assert!(!reversed.window.valid);
    assert!(reversed.report.unwrap().buckets.is_empty());
}

#[test]
fn a_report_reads_its_average_largest_and_active_days_and_stacks_without_netting() {
    let view = view(
        vec![
            transaction("a", 10.1, json!({ "date": "2026-09-01" })),
            transaction(
                "b",
                -30.4,
                json!({ "date": "2026-09-01", "classification": classification("fee") }),
            ),
            transaction("c", 5.3, json!({ "date": "2026-09-03" })),
            transaction("outside", 1000.0, json!({ "date": "2026-08-01" })),
        ],
        vec![],
    );
    let report = read(
        &view,
        ReportRequest {
            from: Some(day("2026-09-01")),
            to: Some(day("2026-09-03")),
            ..request(Chart::CashFlow, None)
        },
    )
    .report
    .unwrap();
    assert_eq!(
        (report.average, report.largest, report.active_days),
        (Some(-5.0), Some(-30.4), 2)
    );
    // Income and fees stack apart, never netted, each with its own total.
    let first = &report.buckets[0];
    assert_eq!((first.positive, first.negative), (10.1, -30.4));
    assert_eq!(
        first
            .groups
            .iter()
            .map(|group| (group.name.as_str(), group.negative, group.total))
            .collect::<Vec<_>>(),
        [("Fees", true, -30.4), ("Income", false, 10.1)]
    );
    assert_eq!(report.unit, Unit::Day);
    // Nothing in the window reads as nothing, not zero.
    let empty = read(
        &view,
        ReportRequest {
            from: Some(day("2026-09-10")),
            to: Some(day("2026-09-11")),
            ..request(Chart::CashFlow, None)
        },
    )
    .report
    .unwrap();
    assert_eq!(
        (empty.average, empty.largest, empty.active_days),
        (None, None, 0)
    );
}

#[test]
fn income_groups_by_kind_of_payment_and_other_reports_by_account_name() {
    let view_with = |transactions: Vec<Value>| view(transactions, vec![]);
    let view = view_with(vec![
        transaction(
            "broker",
            10.0,
            json!({ "merchant": "TRANSFER MONEY FROM BROKERAGE XXXXX0000 REF A" }),
        ),
        transaction(
            "zelle",
            30.0,
            json!({ "merchant": "ZELLE PAYMENT FROM EXAMPLE PERSON A", "classification": {
                "kind": "income", "mark": "income", "spending": false, "credit": false, "zelle": true,
            } }),
        ),
        transaction("payroll", 50.0, json!({})),
        transaction(
            "interest",
            2.0,
            json!({ "merchant": "Interest payment", "classification": classification("interest") }),
        ),
        transaction("payer", 4.0, json!({ "merchant": "Example   long  payer" })),
    ]);
    let groups = read(&view, request(Chart::Income, None))
        .report
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.group)
        .collect::<Vec<_>>();
    assert_eq!(
        groups,
        [
            "Brokerage transfers",
            "Zelle payments",
            "Payroll",
            "Interest",
            "Example long payer"
        ]
    );
    // Interest groups by the name the user gave the account, else the feed's.
    let mut named = view_with(vec![transaction(
        "interest",
        2.0,
        json!({ "classification": classification("interest"), "account": "Provider account" }),
    )]);
    assert_eq!(
        read(&named, request(Chart::Interest, None))
            .report
            .unwrap()
            .entries[0]
            .group,
        "Provider account"
    );
    named
        .account_names
        .insert("cash".into(), "Everyday account".into());
    let reports = read(&named, request(Chart::Interest, None));
    assert_eq!(reports.report.unwrap().entries[0].group, "Everyday account");
    assert_eq!(reports.groups, ["Everyday account"]);
    assert_eq!(
        reports.accounts,
        [AccountTotal {
            account_id: "cash".into(),
            total: Some(2.0),
        }]
    );
}

#[test]
fn past_a_stale_snapshot_days_are_unknown_and_compare_with_as_many_days_before() {
    let mut stale = view(
        vec![
            transaction("older", 10.0, json!({ "date": "2026-07-01" })),
            transaction("covered", 40.0, json!({ "date": "2026-08-22" })),
            transaction("uncovered", 900.0, json!({ "date": "2026-08-24" })),
            transaction("saved", 75.0, json!({ "date": "2026-09-21" })),
        ],
        vec![],
    );
    stale.snapshot.updated_at = "2026-09-21T12:00:00Z".into();
    let reports = read_on(
        &stale,
        request(Chart::Income, Some(Range::Month)),
        "2026-09-25",
    );
    assert_eq!(
        (
            reports.window.start,
            reports.window.end,
            reports.window.saved_end
        ),
        (day("2026-08-27"), day("2026-09-25"), day("2026-09-21"))
    );
    let report = reports.report.unwrap();
    // The four days after the save have no figures, rather than zero.
    let last = &report.buckets[report.buckets.len() - 5..];
    assert_eq!(
        last.iter().map(|bucket| bucket.value).collect::<Vec<_>>(),
        [Some(75.0), None, None, None, None]
    );
    // Only the 26 saved days count, against the first 26 days before.
    assert_eq!(
        (
            report.summary.total,
            report.summary.previous,
            report.summary.percent
        ),
        (Some(75.0), Some(40.0), Some(87.5))
    );
}

#[test]
fn card_credits_are_the_saved_cards_posted_credits_net_of_reversals() {
    let mut view = view(vec![], vec![]);
    let mut seed: crate::finance_contract::Snapshot =
        serde_json::from_str(include_str!("../../../../src/data/seed.json")).unwrap();
    let card = seed
        .accounts
        .iter()
        .find(|account| account.r#type == "credit")
        .unwrap()
        .id
        .clone();
    let settings = crate::settings::FinanceSettings {
        spending_account_id: Some(card.clone()),
        card_rules: vec![crate::insights::benefits::example_rules()],
        ..Default::default()
    };
    let now = chrono::DateTime::parse_from_rfc3339(&seed.updated_at.clone())
        .unwrap()
        .to_utc();
    let insights = crate::insights::derive(&mut seed, &settings, now, &NamedZone::Utc).unwrap();
    let activity = &insights.benefits.as_ref().unwrap().activity;
    let expected = total(
        activity
            .iter()
            .filter(|item| {
                !item.pending
                    && matches!(
                        item.kind,
                        crate::insights::benefits::ActivityKind::Credit
                            | crate::insights::benefits::ActivityKind::Reversal
                    )
            })
            .map(|item| Some(item.amount))
            .collect::<Vec<_>>()
            .iter(),
    );
    let today = insights.today;
    view.snapshot = seed;
    view.insights = Some(insights);
    view.spending_account_id = Some(card.clone());
    let all_time = |view: &View| {
        read_on(
            view,
            request(Chart::CardCredits, Some(Range::All)),
            &today.to_string(),
        )
        .report
        .unwrap()
    };
    let report = all_time(&view);
    assert!(!report.entries.is_empty());
    assert_eq!(report.summary.total, expected);
    // Another saved card has no card credits to report.
    view.spending_account_id = Some("cash".into());
    assert!(all_time(&view).entries.is_empty());
}

#[test]
fn every_report_is_summed_for_the_strip_and_an_empty_account_choice_counts_nothing() {
    let view = view(
        vec![
            transaction("cash-income", 100.0, json!({})),
            transaction("card-income", 25.0, json!({ "accountId": "card" })),
        ],
        vec![],
    );
    let all = read(&view, ReportRequest::default());
    assert!(all.report.is_none());
    let income = |reports: &Reports| {
        reports
            .summaries
            .iter()
            .find(|summary| summary.chart == Chart::Income)
            .unwrap()
            .total
    };
    assert_eq!(all.summaries.len(), Chart::ALL.len());
    assert_eq!(income(&all), Some(125.0));
    let none = read(
        &view,
        ReportRequest {
            accounts: Some(Vec::new()),
            ..Default::default()
        },
    );
    assert_eq!(income(&none), Some(0.0));
}

/// Whole cents, so sums are compared exactly.
fn cents(value: f64) -> i64 {
    crate::numeric::minor_units(value)
}

/// A synthetic life's last snapshot with every third card purchase split in two at a generated
/// point, read through the same path as the app (`storage::derive_view`).
fn synthetic_view(seed: u64) -> (View, NaiveDate) {
    let life = crate::synthetic::Life::new(seed);
    let mut state = crate::storage::empty_state().unwrap();
    for day in life.refresh_days() {
        let refresh = life.refresh(day);
        let snapshot = crate::storage::change::project_refresh(
            &state.snapshot,
            &[],
            &state.account_links,
            &crate::insights::benefits::Catalog::from_settings(&state.settings),
            &refresh,
        )
        .unwrap();
        state = crate::storage::change::finish_refresh(&state, &refresh, snapshot).unwrap();
    }
    // A splitmix-style walk, so each seed splits different purchases at different points.
    let mut walk = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        walk ^= walk << 13;
        walk ^= walk >> 7;
        walk ^= walk << 17;
        walk
    };
    for (index, transaction) in state.snapshot.transactions.iter().enumerate() {
        let whole = cents(transaction.amount);
        if index % 3 != 0 || whole > -200 || transaction.pending {
            continue;
        }
        let part = -(1 + i64::try_from(next() % u64::try_from(-whole - 1).unwrap()).unwrap());
        state.annotations.insert(
            transaction.id.clone(),
            crate::storage::Annotation {
                splits: Some(vec![
                    crate::storage::Split {
                        category: "Generated A".into(),
                        amount: crate::numeric::from_minor(part),
                    },
                    crate::storage::Split {
                        category: "Generated B".into(),
                        amount: crate::numeric::from_minor(whole - part),
                    },
                ]),
                ..Default::default()
            },
        );
    }
    let now = chrono::DateTime::parse_from_rfc3339(&state.snapshot.updated_at)
        .unwrap()
        .to_utc();
    let (view, _) =
        crate::storage::derive_view(&state, None, &BTreeMap::new(), now, &NamedZone::Utc).unwrap();
    (view, now.date_naive())
}

#[test]
fn every_breakdown_sums_to_its_total_to_the_cent() {
    use crate::insights::spending::{PeriodKind, PeriodRequest};
    let mut periods = 0;
    for seed in 1..=6 {
        let (view, today) = synthetic_view(seed);
        let insights = view.insights.as_ref().unwrap();
        assert!(view
            .snapshot
            .transactions
            .iter()
            .any(|transaction| !transaction.splits.is_empty()));

        // Each Spending period's categories, split parts included, sum to its total.
        for scope in &insights.spending {
            for kind in [
                PeriodKind::Week,
                PeriodKind::Month,
                PeriodKind::Quarter,
                PeriodKind::Year,
                PeriodKind::All,
            ] {
                for offset in 0..4 {
                    let request = PeriodRequest {
                        scope: scope.scope.clone(),
                        owner: scope.owner,
                        kind,
                        offset,
                    };
                    let Some(detail) =
                        crate::insights::spending_period(&view, &request, &NamedZone::Utc)
                    else {
                        break;
                    };
                    let period = &detail.period;
                    let categories: i64 = period
                        .categories
                        .iter()
                        .map(|category| cents(category.spent.total))
                        .sum();
                    assert_eq!(
                        categories,
                        cents(period.spent.total),
                        "seed {seed} {} {kind:?} {offset}",
                        scope.scope
                    );
                    periods += 1;
                }
            }
        }

        // Reports: the breakdown, the buckets, each account, and the cash flow agree.
        for chart in Chart::ALL {
            for range in Range::ALL {
                let request = ReportRequest {
                    chart: Some(chart),
                    range: Some(range),
                    ..Default::default()
                };
                let reports = reports(&view, &request, today, &NamedZone::Utc).unwrap();
                let report = reports.report.as_ref().unwrap();
                let context = format!("seed {seed} {chart:?} {range:?}");
                let Some(total) = report.summary.total.map(cents) else {
                    continue;
                };
                let breakdown: i64 = report
                    .breakdown
                    .iter()
                    .map(|group| cents(group.value.unwrap()))
                    .sum();
                assert_eq!(breakdown, total, "{context}: breakdown");
                let buckets: i64 = report
                    .buckets
                    .iter()
                    .map(|bucket| bucket.value.map_or(0, cents))
                    .sum();
                assert_eq!(buckets, total, "{context}: buckets");
                let accounts: i64 = reports
                    .accounts
                    .iter()
                    .map(|account| account.total.map_or(0, cents))
                    .sum();
                assert_eq!(accounts, total, "{context}: accounts");
                if let Some(flow) = &reports.flow {
                    let items = |items: &[FlowItem]| -> i64 {
                        items
                            .iter()
                            .map(|item| {
                                let children: i64 =
                                    item.children.iter().map(|child| cents(child.value)).sum();
                                assert_eq!(children, cents(item.value), "{context}: {}", item.id);
                                children
                            })
                            .sum()
                    };
                    assert_eq!(
                        items(&flow.sources),
                        cents(flow.inflow),
                        "{context}: inflow"
                    );
                    assert_eq!(
                        items(&flow.purchases)
                            + flow
                                .charges
                                .as_ref()
                                .map_or(0, |charges| items(std::slice::from_ref(charges))),
                        cents(flow.outflow),
                        "{context}: outflow"
                    );
                    // The surplus or shortfall equals the report's total.
                    assert_eq!(
                        cents(flow.retained) - cents(flow.gap),
                        total,
                        "{context}: what was left"
                    );
                    assert!(
                        flow.sources.len() <= FLOW_GROUPS + 1
                            && flow.purchases.len() <= FLOW_GROUPS + 1,
                        "{context}: groups shown"
                    );
                }
            }
        }
    }
    assert!(periods > 50, "only {periods} periods were checked");
}
