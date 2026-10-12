use serde_json::json;

use super::*;
use crate::insights::calendar::NamedZone;
use crate::transaction_policy::{Classification, Kind, Mark};

fn txn(
    id: &str,
    date: &str,
    merchant: &str,
    category: &str,
    amount: f64,
    kind: Kind,
) -> Transaction {
    let mut value = json!({
        "id": id, "merchant": merchant, "category": category, "date": date,
        "amount": amount, "account": "Checking", "accountId": "plaid:checking",
        "pending": false, "categorySource": "provider"
    });
    value["classification"] = serde_json::to_value(Classification {
        mark: Mark::Initial,
        spending: kind == Kind::Expense,
        kind,
        credit: false,
        zelle: false,
    })
    .unwrap();
    serde_json::from_value(value).unwrap()
}

fn account(id: &str, kind: &str, value: f64) -> Account {
    serde_json::from_value(json!({
        "id": id, "name": id, "institution": "Bank", "type": kind, "value": value,
        "currency": "USD", "balanceFetchedAt": "2026-09-16T12:00:00Z"
    }))
    .unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-16T15:00:00Z")
        .unwrap()
        .to_utc()
}

fn history() -> Vec<Transaction> {
    let mut transactions = Vec::new();
    // Monthly rent and a streaming subscription for five months.
    for (index, month) in ["04", "05", "06", "07", "08", "09"].iter().enumerate() {
        transactions.push(txn(
            &format!("rent-{month}"),
            &format!("2026-{month}-01"),
            "Example Property",
            "Rent And Utilities",
            -1_500.0,
            Kind::Expense,
        ));
        if index > 0 {
            transactions.push(txn(
                &format!("dining-{month}"),
                &format!("2026-{month}-10"),
                "Corner Cafe",
                "Food And Drink",
                -40.0,
                Kind::Expense,
            ));
        }
    }
    transactions.push(txn(
        "dining-big",
        "2026-09-12",
        "Corner Cafe",
        "Food And Drink",
        -260.0,
        Kind::Expense,
    ));
    transactions.push(txn(
        "dup-a",
        "2026-09-13",
        "Hardware Store",
        "Home Improvement",
        -55.0,
        Kind::Expense,
    ));
    transactions.push(txn(
        "dup-b",
        "2026-09-14",
        "Hardware Store",
        "Home Improvement",
        -55.0,
        Kind::Expense,
    ));
    transactions.push(txn(
        "move",
        "2026-09-14",
        "Transfer",
        "Transfer",
        -500.0,
        Kind::Transfer,
    ));
    transactions
}

#[test]
fn fixed_merchant_charges_count_as_subscriptions_but_rent_does_not() {
    let mut transactions = Vec::new();
    for month in ["06", "07", "08", "09"] {
        for (id, merchant, category, amount) in [
            ("assistant", "Example Assistant", "Services", -20.0),
            ("video", "Example Video", "Entertainment", -15.99),
            ("rent", "Example Property", "Rent", -1_500.0),
        ] {
            transactions.push(txn(
                &format!("{id}-{month}"),
                &format!("2026-{month}-05"),
                merchant,
                category,
                amount,
                Kind::Expense,
            ));
        }
    }
    let insights = compute(
        &transactions,
        &[account("plaid:checking", "cash", 1_000.0)],
        &FinanceSettings::default(),
        "2026-09-16T12:00:00Z",
        now(),
        &NamedZone::Utc,
    );
    let titles = insights
        .subscriptions
        .items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, ["Example Assistant", "Example Video"]);
    assert_eq!(insights.subscriptions.monthly, -35.99);
}

#[test]
fn month_end_charges_that_post_the_next_day_stay_monthly() {
    let transactions = [
        ("2026-06-30", "2026-07-01"),
        ("2026-07-31", "2026-07-31"),
        ("2026-08-31", "2026-08-31"),
        ("2026-09-30", "2026-10-01"),
    ]
    .iter()
    .enumerate()
    .map(|(index, (charged, posted))| {
        let mut transaction = txn(
            &format!("video-{index}"),
            charged,
            "Example Video",
            "Entertainment",
            -15.99,
            Kind::Expense,
        );
        transaction.posted_on = Some((*posted).into());
        transaction
    })
    .collect::<Vec<_>>();
    let mut checking = account("plaid:checking", "cash", 1_000.0);
    checking.balance_fetched_at = Some("2026-10-06T12:00:00Z".into());
    let insights = compute(
        &transactions,
        &[checking],
        &FinanceSettings::default(),
        "2026-10-06T12:00:00Z",
        DateTime::parse_from_rfc3339("2026-10-06T15:00:00Z")
            .unwrap()
            .to_utc(),
        &NamedZone::Utc,
    );
    let video = &insights.subscriptions.items[0];
    assert_eq!(video.frequency, "Monthly");
    assert_eq!(video.next_date.to_string(), "2026-10-31");
}

#[test]
fn plan_paces_categories_and_covers_expected_outflows_in_cents() {
    let transactions = history();
    let settings = FinanceSettings::default();
    let accounts = vec![account("plaid:checking", "cash", 1_200.10)];
    let insights = compute(
        &transactions,
        &accounts,
        &settings,
        "2026-09-16T12:00:00Z",
        now(),
        &NamedZone::Utc,
    );
    let plan = &insights.plan;
    let dining = plan
        .categories
        .iter()
        .find(|c| c.category == "Food And Drink")
        .unwrap();
    assert_eq!(dining.spent, 300.0);
    assert_eq!(dining.typical, 40.0);
    let home = plan
        .categories
        .iter()
        .find(|c| c.category == "Home Improvement")
        .unwrap();
    // $110 in 16 of 30 days paces to $206.25.
    assert_eq!(home.projected, 206.25);
    assert_eq!(plan.monthly_spending, Some(1_540.0));
    assert_eq!(plan.runway_months, Some(0.7));
    assert_eq!(plan.coverage.cash, 1_200.1);
    // Rent is expected on Oct 1 and exceeds cash, so coverage is short and ranked first.
    assert_eq!(plan.coverage.expected_outflows, 1_500.0);
    assert!(matches!(
        insights.attention[0].item,
        attention::Item::CoverageShort { .. }
    ));
    assert_eq!(insights.attention.len(), 1);
}

#[test]
fn a_refund_is_matched_only_when_one_purchase_explains_it() {
    let today = NaiveDate::from_ymd_opt(2026, 9, 16).unwrap();
    let suggestions = |transactions: &[Transaction]| {
        refunds::suggestions(transactions, "2026-09-16T12:00:00Z", today, &NamedZone::Utc)
    };
    let purchase = |id: &str, date: &str, amount: f64| {
        txn(
            id,
            date,
            "Example Outfitters",
            "Shopping",
            amount,
            Kind::Expense,
        )
    };
    let refund = |amount: f64| {
        txn(
            "refund",
            "2026-09-10",
            "Example Outfitters",
            "Shopping",
            amount,
            Kind::Reimbursement,
        )
    };
    let matched = |purchase: &str| {
        vec![refunds::RefundSuggestion {
            refund_id: "refund".into(),
            purchase_id: purchase.into(),
        }]
    };
    // The only purchase large enough explains a partial refund.
    assert_eq!(
        suggestions(&[purchase("jacket", "2026-08-20", -120.0), refund(40.0)]),
        matched("jacket")
    );
    // Among several, only an exact amount is unambiguous.
    let two = [
        purchase("jacket", "2026-08-20", -120.0),
        purchase("boots", "2026-08-25", -80.0),
    ];
    assert_eq!(
        suggestions(&[two[0].clone(), two[1].clone(), refund(80.0)]),
        matched("boots")
    );
    assert!(suggestions(&[two[0].clone(), two[1].clone(), refund(30.0)]).is_empty());
    // Too small, too old, later than the refund, or already linked: no suggestion.
    assert!(suggestions(&[purchase("small", "2026-08-20", -20.0), refund(40.0)]).is_empty());
    assert!(suggestions(&[purchase("old", "2026-04-01", -120.0), refund(40.0)]).is_empty());
    assert!(suggestions(&[purchase("later", "2026-09-12", -120.0), refund(40.0)]).is_empty());
    let mut linked = refund(40.0);
    linked.refund_of = Some("jacket".into());
    assert!(suggestions(&[purchase("jacket", "2026-08-20", -120.0), linked]).is_empty());
}

#[test]
fn a_card_credit_belongs_to_whoever_its_purchase_was_for() {
    let classified = |kind: &str, mark: &str, credit: bool| json!({ "mark": mark, "kind": kind, "spending": kind == "expense", "credit": credit, "zelle": false });
    let card_txn = |id: &str, date: &str, merchant: &str, amount: f64| {
        let (kind, mark, credit) = if amount < 0.0 {
            ("expense", "initial", false)
        } else {
            ("reimbursement", "refund", true)
        };
        json!({
            "id": id, "merchant": merchant, "category": "Entertainment", "date": date,
            "amount": amount, "account": "Card", "accountId": "plaid:card", "pending": false,
            "classification": classified(kind, mark, credit),
        })
    };
    let mut theirs = card_txn("theirs", "2026-07-15", "Disney Plus", -13.99);
    theirs["other"] = true.into();
    theirs["classification"] = classified("owed", "initial", false);
    let mut snapshot = json!({
        "updatedAt": "2026-09-16T12:00:00Z",
        "netWorth": -100.0, "holdings": [], "netWorthHistory": [],
        "spending": { "monthTotal": 0, "categories": [] },
        "accounts": [{ "id": "plaid:card", "name": "Card", "institution": "Bank", "type": "credit", "value": -100.0, "currency": "USD" }],
        "transactions": [
            theirs,
            card_txn("their-credit", "2026-07-18", "Platinum Digital Entertainment Credit", 13.99),
            card_txn("mine", "2026-08-15", "Disney Plus", -13.99),
            card_txn("my-credit", "2026-08-18", "Platinum Digital Entertainment Credit", 13.99),
        ],
    });
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:card".into()),
        ..Default::default()
    };
    let now = DateTime::parse_from_rfc3339("2026-09-16T12:00:00Z")
        .unwrap()
        .to_utc();
    attach(&mut snapshot, &settings, now, &NamedZone::Utc).unwrap();
    let transaction = |id: &str| {
        snapshot["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == id)
            .unwrap()
            .clone()
    };
    // The credit on their purchase is theirs: neither your spending nor your income.
    assert_eq!(transaction("their-credit")["other"], true);
    assert_eq!(
        transaction("their-credit")["classification"]["kind"],
        "owed"
    );
    assert!(transaction("my-credit").get("other").is_none());
    assert_eq!(
        transaction("my-credit")["classification"]["kind"],
        "reimbursement"
    );
    // Both used the allowance; only yours paid back your fee.
    let fee = &snapshot["insights"]["benefits"]["fee"];
    assert_eq!(fee["recovered"], 13.99);
    assert_eq!(fee["forOthers"], 13.99);
}

#[test]
fn split_parts_count_under_their_own_categories() {
    let mut transactions = history();
    let big = transactions
        .iter_mut()
        .find(|t| t.id == "dining-big")
        .unwrap();
    big.splits = vec![
        crate::storage::Split {
            category: "Food And Drink".into(),
            amount: -60.0,
        },
        crate::storage::Split {
            category: "Gifts".into(),
            amount: -200.0,
        },
    ];
    let plan = plan::plan(
        &transactions,
        &[],
        &[],
        "2026-09-16T12:00:00Z",
        NaiveDate::from_ymd_opt(2026, 9, 16).unwrap(),
        &NamedZone::Utc,
    );
    let spent = |category: &str| {
        plan.categories
            .iter()
            .find(|item| item.category == category)
            .map(|item| item.spent)
    };
    // $40 on the 10th plus the $60 part; the $200 part is a gift, and the total is unchanged.
    assert_eq!(spent("Food And Drink"), Some(100.0));
    assert_eq!(spent("Gifts"), Some(200.0));
    assert_eq!(plan.spent, 1_910.0);
}

#[test]
fn the_brief_lists_each_thing_that_needs_action_once() {
    let card_txn = |id: &str, date: &str, merchant: &str, amount: f64| -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": merchant, "category": "Entertainment", "date": date,
            "amount": amount, "account": "Card", "accountId": "plaid:card", "pending": false,
        }))
        .unwrap()
    };
    let mut transactions = history();
    // The card used its monthly streaming credit in August and not yet in September.
    transactions.push(card_txn("disney", "2026-08-15", "Disney Plus", -13.99));
    transactions.push(card_txn(
        "disney-credit",
        "2026-08-18",
        "Digital Entertainment Credit",
        13.99,
    ));
    let account = |id: &str, kind: &str, value: f64| -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": kind, "value": value,
            "currency": "USD", "balanceFetchedAt": "2026-09-26T12:00:00Z"
        }))
        .unwrap()
    };
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:card".into()),
        ..Default::default()
    };
    let brief = |cash: f64, settings: &FinanceSettings| {
        compute(
            &transactions,
            &[
                account("plaid:checking", "cash", cash),
                account("plaid:card", "credit", -100.0),
            ],
            settings,
            "2026-09-26T12:00:00Z",
            DateTime::parse_from_rfc3339("2026-09-26T15:00:00Z")
                .unwrap()
                .to_utc(),
            &NamedZone::Utc,
        )
        .brief
    };
    let kinds = |brief: &attention::Brief| {
        brief
            .needs
            .iter()
            .map(|item| match &item.item {
                attention::Item::PaymentDue { .. } => "payment-due",
                attention::Item::CoverageShort { .. } => "coverage-short",
                attention::Item::CreditExpiring { .. } => "credit-expiring",
                attention::Item::CashLow { .. } => "cash-low",
                attention::Item::UtilizationHigh { .. } => "utilization-high",
                attention::Item::PossibleDuplicate { .. } => "possible-duplicate",
                attention::Item::WashWindow { .. } => "wash-window",
            })
            .collect::<Vec<_>>()
    };

    // Rent on Oct 1 exceeds cash: the urgent shortfall leads, and the later low is the
    // same problem, so it is not listed twice.
    let short = brief(1_200.10, &settings);
    assert_eq!(kinds(&short), vec!["coverage-short", "credit-expiring"]);
    assert!(matches!(
        &short.needs[1].item,
        attention::Item::CreditExpiring { benefit_id, remaining_low, remaining_high, days, .. }
            if benefit_id == "digital-entertainment"
                && *remaining_low == 25.0 && *remaining_high == 25.0 && *days == 4
    ));
    assert_eq!(short.coming.len(), 1, "rent is due within two weeks");

    // Cash covers the next 30 days but not three months of rent.
    let later = brief(2_000.0, &settings);
    assert_eq!(kinds(&later), vec!["cash-low", "credit-expiring"]);
    assert!(matches!(
        later.needs[0].item,
        attention::Item::CashLow { expected, .. } if expected == -2_500.0
    ));

    // A hidden benefit never asks for attention.
    let hidden = FinanceSettings {
        hidden_benefit_ids: vec!["digital-entertainment".into()],
        ..settings.clone()
    };
    assert!(kinds(&brief(9_000.0, &hidden)).is_empty());
}

#[test]
fn each_assigned_card_is_tracked_under_its_own_product_and_account() {
    let card_txn =
        |id: &str, account: &str, date: &str, merchant: &str, amount: f64| -> Transaction {
            serde_json::from_value(json!({
                "id": id, "merchant": merchant, "category": "Entertainment", "date": date,
                "amount": amount, "account": account, "accountId": account, "pending": false,
            }))
            .unwrap()
        };
    let mut transactions = Vec::new();
    for account in ["plaid:card", "plaid:second", "plaid:unassigned"] {
        transactions.push(card_txn(
            &format!("{account}-buy"),
            account,
            "2026-08-15",
            "Disney Plus",
            -13.99,
        ));
        transactions.push(card_txn(
            &format!("{account}-credit"),
            account,
            "2026-08-18",
            "Digital Entertainment Credit",
            13.99,
        ));
    }
    let account = |id: &str| -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": "credit", "value": -1.0,
            "currency": "USD", "balanceFetchedAt": "2026-09-26T12:00:00Z"
        }))
        .unwrap()
    };
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:card".into()),
        benefit_cards: [("plaid:second".to_owned(), "amex-platinum".to_owned())].into(),
        ..Default::default()
    };
    let insights = compute(
        &transactions,
        &[
            account("plaid:card"),
            account("plaid:second"),
            account("plaid:unassigned"),
        ],
        &settings,
        "2026-09-26T12:00:00Z",
        DateTime::parse_from_rfc3339("2026-09-26T15:00:00Z")
            .unwrap()
            .to_utc(),
        &NamedZone::Utc,
    );
    // The spending card needs no assignment; a card without one is never tracked.
    assert_eq!(insights.benefits.as_ref().unwrap().account_id, "plaid:card");
    assert_eq!(
        insights.benefits.as_ref().unwrap().product_name,
        "American Express Platinum"
    );
    assert_eq!(
        insights
            .other_card_benefits
            .iter()
            .map(|card| card.account_id.as_str())
            .collect::<Vec<_>>(),
        vec!["plaid:second"]
    );
    // Each card's own credit counts only toward that card, and each expiring credit is
    // its own need.
    assert_eq!(
        insights.other_card_benefits[0]
            .fee
            .as_ref()
            .unwrap()
            .recovered,
        13.99
    );
    let expiring = insights
        .brief
        .needs
        .iter()
        .filter_map(|item| match &item.item {
            attention::Item::CreditExpiring { account_id, .. } => {
                Some((item.id.as_str(), account_id.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        expiring,
        vec![
            (
                "credit-expiring:plaid:card:digital-entertainment:2026-09-30",
                "plaid:card"
            ),
            (
                "credit-expiring:plaid:second:digital-entertainment:2026-09-30",
                "plaid:second"
            ),
        ]
    );
    assert_eq!(benefits::products().len(), 1);
}

#[test]
fn card_payments_due_within_a_week_need_attention_until_paid() {
    let card = |last_payment: &str| -> Account {
        serde_json::from_value(json!({
            "id": "plaid:card", "name": "Card", "institution": "Bank", "type": "credit",
            "value": -500.0, "currency": "USD",
            "liability": {
                "lastStatementDate": "2026-08-25", "lastStatementBalance": 500.0,
                "minimumPayment": 35.0, "nextPaymentDueDate": "2026-09-19",
                "lastPaymentDate": last_payment, "lastPaymentAmount": 500.0,
                "purchaseApr": 20.0, "isOverdue": false
            }
        }))
        .unwrap()
    };
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:card".into()),
        ..Default::default()
    };
    let due = |account: Account| {
        compute(
            &[],
            &[account],
            &settings,
            "2026-09-16T12:00:00Z",
            now(),
            &NamedZone::Utc,
        )
        .attention
        .into_iter()
        .find_map(|item| match item.item {
            attention::Item::PaymentDue { days, .. } => Some((days, item.severity)),
            _ => None,
        })
    };
    assert_eq!(
        due(card("2026-08-10")),
        Some((3, attention::Severity::Urgent))
    );
    assert_eq!(
        due(card("2026-09-01")),
        None,
        "a covering payment clears it"
    );
}

#[test]
fn unknown_spending_account_produces_no_card_insights() {
    let settings = FinanceSettings {
        spending_account_id: Some("plaid:checking".into()),
        ..Default::default()
    };
    let insights = compute(
        &history(),
        &[account("plaid:checking", "cash", 10.0)],
        &settings,
        "2026-09-16T12:00:00Z",
        now(),
        &NamedZone::Utc,
    );
    assert!(
        insights.benefits.is_none(),
        "only a credit account can be the spending card"
    );
    assert!(insights.statement.is_none());
}
