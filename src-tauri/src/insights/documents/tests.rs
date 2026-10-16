use serde_json::json;

use super::*;
use crate::documents::{Field, PayDeposit};
use crate::transaction_policy::{Classification, Mark};

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn row(
    id: &str,
    account: &str,
    date: &str,
    merchant: &str,
    amount: f64,
    kind: TransactionKind,
) -> Transaction {
    let mut value = json!({
        "id": id, "merchant": merchant, "category": "Income", "date": date,
        "amount": amount, "account": "Example", "accountId": account, "pending": false,
    });
    value["classification"] = serde_json::to_value(Classification {
        mark: Mark::Initial,
        spending: false,
        kind,
        credit: false,
        zelle: false,
        others_spending: false,
    })
    .unwrap();
    serde_json::from_value(value).unwrap()
}

fn account(id: &str, kind: &str, institution: &str, mask: Option<&str>) -> Account {
    Account {
        id: id.into(),
        name: id.into(),
        institution: institution.into(),
        r#type: kind.into(),
        mask: mask.map(str::to_owned),
        tax_treatment: Some(
            if kind == "retirement" {
                "taxFree"
            } else {
                "taxable"
            }
            .into(),
        ),
        ..Account::default()
    }
}

fn field(key: &str, amount: Option<f64>, ytd: Option<f64>) -> Field {
    Field {
        key: key.into(),
        label: key.into(),
        amount,
        ytd,
        ..Field::default()
    }
}

fn document(id: &str, kind: Kind, form: &str, issuer: &str, year: i32, date: &str) -> Document {
    Document {
        id: format!("{id:0>64}"),
        file_name: format!("{id}.pdf"),
        media_type: "application/pdf".into(),
        size: 1,
        imported_at: "2026-10-15T00:00:00Z".into(),
        kind,
        form: Some(form.into()),
        corrected: false,
        issuer: Some(issuer.into()),
        tax_year: Some(year),
        date: Some(date.into()),
        period_start: None,
        period_end: None,
        account_id: None,
        mask: None,
        fields: Vec::new(),
        deposits: Vec::new(),
        checks: Vec::new(),
        statement_rows: None,
        statement_csv: None,
        statement_file_id: None,
        reviewed: false,
        person: None,
        unreadable: None,
        reader: 0,
        summary: None,
    }
}

fn payslip(id: &str, date: &str, net: f64, mask: &str, ytd: [f64; 7]) -> Document {
    let [gross, pretax, espp, federal, social, medicare, state] = ytd;
    Document {
        fields: vec![
            field("gross", None, Some(gross)),
            field("net", Some(net), None),
            field("pretax401k", Some(240.38), Some(pretax)),
            field("espp", Some(240.38), Some(espp)),
            field("federalWithholding", None, Some(federal)),
            field("socialSecurityTax", None, Some(social)),
            field("medicareTax", None, Some(medicare)),
            field("stateWithholding", None, Some(state)),
        ],
        deposits: vec![PayDeposit {
            mask: Some(mask.into()),
            amount: net,
        }],
        ..document(
            id,
            Kind::Payslip,
            "Pay statement",
            "Example Services Group Inc",
            2026,
            date,
        )
    }
}

fn run(
    documents: &[Document],
    transactions: &[Transaction],
    accounts: &[Account],
    today: &str,
) -> Documents {
    super::documents(
        Inputs {
            documents,
            transactions,
            accounts,
            filing_status: Some("single"),
        },
        day(today),
    )
}

fn accounts() -> Vec<Account> {
    vec![
        account("cash", "cash", "Example Bank", Some("8549")),
        account("broker", "brokerage", "Example Brokerage", Some("5784")),
        account("roth", "retirement", "Example Brokerage", Some("5791")),
    ]
}

#[test]
fn a_pay_statements_deposit_is_found_in_the_account_it_names_or_raised_when_missing() {
    let documents = vec![
        payslip(
            "a",
            "2026-06-18",
            2_937.35,
            "8549",
            [
                19_230.76, 721.14, 721.14, 2_795.49, 1_192.31, 278.85, 1_201.03,
            ],
        ),
        payslip(
            "b",
            "2026-07-02",
            2_937.35,
            "8549",
            [
                24_038.45, 961.52, 961.52, 3_460.68, 1_490.38, 348.56, 1_495.14,
            ],
        ),
        // Paid to an account Brief doesn't track: noted, never raised.
        payslip(
            "c",
            "2026-07-17",
            100.0,
            "0804",
            [
                24_038.45, 961.52, 961.52, 3_460.68, 1_490.38, 348.56, 1_495.14,
            ],
        ),
    ];
    let transactions = vec![
        // Arrived a day early; the same amount in another account doesn't count.
        row(
            "pay",
            "cash",
            "2026-06-17",
            "EXAMPLE PAYROLL DEPOSIT",
            2_937.35,
            TransactionKind::Income,
        ),
        row(
            "elsewhere",
            "broker",
            "2026-07-02",
            "Transfer",
            2_937.35,
            TransactionKind::Transfer,
        ),
    ];
    let result = run(&documents, &transactions, &accounts(), "2026-07-20");
    let link = |id: &str| {
        result
            .pay
            .iter()
            .find(|link| link.document_id.ends_with(id))
            .unwrap()
    };
    assert_eq!(link("a").transaction_id.as_deref(), Some("pay"));
    assert_eq!(link("b").transaction_id, None);
    assert_eq!(link("c").account_id, None);
    let needs = super::needs(&result, &documents, day("2026-07-20"));
    assert_eq!(needs.len(), 1);
    assert!(
        matches!(&needs[0].item, super::super::attention::Item::PayMissing { date, .. } if *date == day("2026-07-02"))
    );
}

#[test]
fn the_latest_pay_statement_predicts_the_w2_and_a_differing_w2_is_raised() {
    let documents = vec![
        payslip(
            "a",
            "2026-07-02",
            1.0,
            "8549",
            [
                24_038.45, 961.52, 961.52, 3_460.68, 1_490.38, 348.56, 1_495.14,
            ],
        ),
        payslip(
            "b",
            "2026-09-11",
            1.0,
            "8549",
            [
                30_769.31, 1_298.06, 1_201.90, 4_342.05, 1_907.70, 446.15, 1_884.87,
            ],
        ),
    ];
    let year = &run(&documents, &[], &accounts(), "2026-10-15").years[0];
    let prediction = &year.predictions[0];
    assert!(prediction.from_document.ends_with('b'));
    let predicted = |key: &str| {
        prediction
            .boxes
            .iter()
            .find(|item| item.key == key)
            .unwrap()
            .predicted
    };
    // Box 1 leaves out the pre-tax 401(k); Social Security and Medicare wages don't.
    assert_eq!(predicted("box1"), 29_471.25);
    assert_eq!(predicted("box3"), 30_769.31);
    assert_eq!(predicted("box12:D"), 1_298.06);

    let mut w2 = document(
        "w2",
        Kind::TaxForm,
        "W-2",
        "Example Services Group",
        2026,
        "2027-01-31",
    );
    w2.fields = vec![
        field("box1", Some(29_471.25), None),
        field("box2", Some(4_300.0), None),
    ];
    let mut all = documents;
    all.push(w2);
    let result = run(&all, &[], &accounts(), "2027-02-01");
    let year = result.years.iter().find(|year| year.year == 2026).unwrap();
    assert!(year.predictions[0]
        .w2
        .as_deref()
        .is_some_and(|id| id.ends_with("w2")));
    // Every employer's W-2 is in, so wages come from it.
    assert_eq!(year.income.as_ref().unwrap().wages_from, "w2");
    let needs = super::needs(&result, &all, day("2027-02-01"));
    assert!(needs.iter().any(|need| matches!(
        &need.item,
        super::super::attention::Item::W2Differs { boxes: 1, .. }
    )));
}

#[test]
fn contractor_pay_carries_self_employment_tax_and_a_1099_nec_states_it() {
    let transactions = vec![
        row(
            "h1",
            "cash",
            "2026-04-02",
            "Example AI Sol EXAMPLE",
            1_500.0,
            TransactionKind::Income,
        ),
        row(
            "h2",
            "cash",
            "2026-09-17",
            "DIRECT DEP FUNDS RECVD Example AI Sol EXAMPLE",
            2_666.12,
            TransactionKind::Income,
        ),
        // A transfer from the user's own bank is no payer.
        row(
            "own",
            "cash",
            "2026-08-13",
            "DIRECT DEP FUNDS RECVD JPMorgan Chase Ext Trnsfr",
            5_114.97,
            TransactionKind::Income,
        ),
        row(
            "small",
            "cash",
            "2026-05-01",
            "Tutoring Sample",
            300.0,
            TransactionKind::Income,
        ),
        row(
            "int",
            "broker",
            "2026-06-30",
            "Interest",
            12.0,
            TransactionKind::Interest,
        ),
    ];
    let year = &run(&[], &transactions, &accounts(), "2026-10-15").years[0];
    let income = year.income.as_ref().unwrap();
    // Both payers' deposits, with or without a form: pay under the threshold is still taxable.
    assert_eq!(income.self_employment, 4_466.12);
    // 15.3% of 92.35%.
    assert_eq!(income.self_employment_tax, 631.04);
    // Less half the tax and the $16,100 standard deduction leaves nothing taxable.
    assert_eq!(income.taxable_income, Some(0.0));

    // A 1099-NEC's box 1 replaces what arrived from its payer.
    let mut nec = document(
        "nec",
        Kind::TaxForm,
        "1099-NEC",
        "Example AI Sol",
        2026,
        "2027-01-31",
    );
    nec.fields = vec![field("box1", Some(4_000.0), None)];
    let year = &run(&[nec], &transactions, &accounts(), "2026-10-15").years[0];
    assert_eq!(year.income.as_ref().unwrap().self_employment, 4_300.0);
}

#[test]
fn a_statement_confirms_each_transaction_it_lists_once() {
    let mut statement = document(
        "st",
        Kind::Statement,
        "Statement",
        "Example Bank",
        2026,
        "2026-01-22",
    );
    statement.account_id = Some("cash".into());
    statement.statement_csv = Some(
        "Date,Description,Amount\n12/30/2025,Venmo,-40.00\n01/08/2026,Transfer,19.00\n01/09/2026,Transfer,19.00\n"
            .into(),
    );
    let transactions = vec![
        // Posted two days after the statement's row.
        row(
            "venmo",
            "cash",
            "2026-01-01",
            "Venmo",
            -40.0,
            TransactionKind::Transfer,
        ),
        row(
            "t1",
            "cash",
            "2026-01-08",
            "Transfer",
            19.0,
            TransactionKind::Transfer,
        ),
        row(
            "t2",
            "cash",
            "2026-01-10",
            "Transfer",
            19.0,
            TransactionKind::Transfer,
        ),
        // Another account, and an amount the statement doesn't list.
        row(
            "other",
            "card",
            "2026-01-08",
            "Transfer",
            19.0,
            TransactionKind::Transfer,
        ),
        row(
            "extra",
            "cash",
            "2026-01-08",
            "Coffee",
            -4.5,
            TransactionKind::Expense,
        ),
    ];
    let result = run(&[statement], &transactions, &accounts(), "2026-10-15");
    let mut confirmed = result
        .evidence
        .iter()
        .map(|item| (item.transaction_id.as_str(), item.kind))
        .collect::<Vec<_>>();
    confirmed.sort();
    assert_eq!(
        confirmed,
        [
            ("t1", "statement"),
            ("t2", "statement"),
            ("venmo", "statement")
        ]
    );
}

#[test]
fn documents_say_how_long_to_keep_them() {
    let w2 = document("w2", Kind::TaxForm, "W-2", "Example", 2025, "2026-01-31");
    let roth = document("r", Kind::TaxForm, "5498", "Example", 2025, "2026-05-31");
    let result = run(&[w2, roth], &[], &accounts(), "2026-10-15");
    assert_eq!(result.keep[0].until, Some(day("2029-04-15")));
    assert_eq!(result.keep[1].until, None);
}
