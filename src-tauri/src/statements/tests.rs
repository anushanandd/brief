use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::*;

fn decimal(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

/// The layout of a brokerage activity export: a preamble with the account and a total, the
/// table, then quoted disclosures.
const BROKERAGE: &str = "All Transactions Activity Types\r
\r
Account Activity for Individual Brokerage -1234 from Current Year\r
\r
Total:,-62.36\r
\r
Activity/Trade Date,Transaction Date,Settlement Date,Activity Type,Description,Symbol,Cusip,Quantity #,Price $,Amount $,Commission,Category,Note\r
03/14/2026,03/14/2026,03/14/2026,Qualified Dividend,EXAMPLE CORP REC 03/01/26 PAY 03/14/26,EXMP,000000001,,,2.10,0.0,--,--\r
03/02/2026,03/02/2026,03/03/2026,Bought,SAMPLE INDEX FUND,SMPL,000000002,2,40.25,-80.50,0.0,--,--\r
02/27/2026,02/27/2026,02/27/2026,Interest Income,SWEEP DEPOSIT (Period 02/01-02/26),SWEEP,000000003,,,0.04,0.0,--,--\r
02/20/2026,02/20/2026,02/23/2026,Sold,DEMO HOLDINGS INC,DEMO,000000004,-1,16.01,16.00,0.01,--,--\r
\r
\"Disclosures, which mention amounts like 1,000.00, follow the table.\"\r
";

#[test]
fn reads_a_brokerage_export_with_preamble_total_and_footer() {
    let parsed = parse(BROKERAGE.as_bytes()).unwrap();
    assert_eq!(
        parsed.hint,
        AccountHint {
            label: Some("Individual Brokerage -1234".into()),
            last_four: Some("1234".into()),
        }
    );
    assert_eq!(parsed.rows.len(), 4);
    assert_eq!(parsed.check.status, Status::Verified);
    assert_eq!(parsed.check.total_matches, Some(true));
    assert!(!parsed.dates_assumed);
    let dividend = &parsed.rows[0];
    assert_eq!(dividend.activity, Activity::Dividend);
    assert_eq!(dividend.symbol.as_deref(), Some("EXMP"));
    assert_eq!(dividend.amount, decimal("2.10"));
    let buy = &parsed.rows[1];
    assert_eq!(buy.activity, Activity::Buy);
    assert_eq!(buy.quantity, Some(decimal("2")));
    assert_eq!(buy.price, Some(decimal("40.25")));
    assert_eq!(buy.settled_on, Some(day("2026-03-03")));
    let sell = &parsed.rows[3];
    assert_eq!(sell.activity, Activity::Sell);
    assert_eq!(sell.fee, Some(decimal("0.01")));
    assert!(parsed.has_trades());
}

#[test]
fn a_total_that_disagrees_with_the_rows_fails() {
    let changed = BROKERAGE.replace("Total:,-62.36", "Total:,-62.37");
    let parsed = parse(changed.as_bytes()).unwrap();
    assert_eq!(parsed.check.status, Status::Failed);
    assert_eq!(parsed.check.total_matches, Some(false));
}

#[test]
fn running_balances_verify_even_when_a_day_is_listed_out_of_order() {
    // Newest first, and the two 03/05 rows are listed in the opposite order to the balances.
    let csv = "Date,Description,Type,Amount,Balance\n\
               03/09/2026,COFFEE SHOP,Debit Card,-4.75,1191.25\n\
               03/05/2026,MONTHLY FEE,Account Fee,-12.00,1196.00\n\
               03/05/2026,PAYROLL EXAMPLE,Direct Deposit,1000.00,1208.00\n\
               03/01/2026,INTEREST,Interest,0.25,208.00\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.check.status, Status::Verified);
    assert_eq!(parsed.check.balance_explained, 4);
    assert_eq!(parsed.rows[2].activity, Activity::Wages);
    assert_eq!(parsed.rows[1].activity, Activity::Fee);
}

#[test]
fn balances_that_do_not_follow_are_partly_verified() {
    let csv = "Date,Description,Amount,Balance\n\
               03/09/2026,COFFEE SHOP,-4.75,500.00\n\
               03/05/2026,PAYROLL,1000.00,1208.00\n\
               03/01/2026,INTEREST,0.25,208.00\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.check.status, Status::Partial);
    assert_eq!(parsed.check.balance_rows, 3);
    assert_eq!(parsed.check.balance_explained, 2);
}

#[test]
fn a_statement_without_totals_or_balances_is_unverified_and_month_first_dates_are_flagged() {
    let csv = "Posted,Payee,Amount\n03/04/2026,GROCER,-23.40\n03/02/2026,CAFE,-3.10\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.check.status, Status::Unverified);
    assert!(parsed.dates_assumed);
    assert_eq!(parsed.rows[0].posted_on, day("2026-03-04"));
}

#[test]
fn split_debit_and_credit_columns_with_day_first_dates() {
    let csv = "Ref,Value date,Narrative,Paid out,Paid in,Balance\n\
               A1,14/08/2026,GROCER LTD,23.40,,977.60\n\
               A2,13/08/2026,SALARY EXAMPLE,,2100.00,1001.00\n\
               A3,02/08/2026,RENT,1100.00,,-1099.00\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert!(!parsed.dates_assumed);
    assert_eq!(parsed.rows[0].posted_on, day("2026-08-14"));
    assert_eq!(parsed.rows[2].posted_on, day("2026-08-02"));
    assert_eq!(parsed.rows[0].amount, decimal("-23.40"));
    assert_eq!(parsed.rows[1].amount, decimal("2100.00"));
    assert_eq!(parsed.rows[0].description, "GROCER LTD");
    assert_eq!(parsed.check.status, Status::Verified);
}

#[test]
fn signs_follow_the_running_balance_when_charges_are_printed_positive() {
    let csv = "Date,Description,Amount,Balance\n\
               03/09/2026,BOOKSTORE,20.00,120.00\n\
               03/05/2026,PAYMENT THANK YOU,-50.00,140.00\n\
               03/01/2026,HARDWARE,40.00,90.00\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.check.status, Status::Verified);
    assert_eq!(parsed.rows[0].amount, decimal("-20.00"));
    assert_eq!(parsed.rows[1].amount, decimal("50.00"));
}

#[test]
fn printed_amount_formats_parse_exactly() {
    for (text, expected) in [
        ("$1,234.56", "1234.56"),
        ("(1,234.56)", "-1234.56"),
        ("12.00-", "-12.00"),
        ("15.00 CR", "15.00"),
        ("15.00 DR", "-15.00"),
        ("-0.07", "-0.07"),
    ] {
        assert_eq!(table::money(text), Some(Ok(decimal(expected))), "{text}");
    }
    assert_eq!(table::money("--"), None);
    assert_eq!(table::money(""), None);
    assert_eq!(table::money("twelve"), Some(Err(())));
}

#[test]
fn unreadable_rows_and_missing_tables_are_errors() {
    let csv = "Date,Description,Amount\n03/04/2026,GROCER,-23.40\n03/02/2026,CAFE,abc\n";
    assert_eq!(
        parse(csv.as_bytes()).unwrap_err(),
        "Row 3 has an unreadable amount"
    );
    assert!(parse(b"Just some notes\nwith no table\n").is_err());
}

#[test]
fn pending_rows_and_rows_without_money_are_kept_out_of_history() {
    let csv = "Date,Description,Amount,Pending\n\
               03/04/2026,GROCER,-23.40,P\n\
               03/03/2026,NOTICE,0.00,\n\
               03/02/2026,CAFE,-3.10,\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 2);
    assert!(parsed.rows[0].pending);
    let plan = plan("plaid:cash", &[parsed.rows.as_slice()], &[], None);
    assert_eq!(plan.rows.len(), 1);
    assert_eq!(plan.counts[0].pending, 1);
}

#[test]
fn reads_a_spreadsheet_export_with_number_cells_and_multiline_descriptions() {
    let parsed = parse(include_bytes!("fixtures/synthetic-statement.xlsx")).unwrap();
    assert_eq!(parsed.hint.last_four.as_deref(), Some("4321"));
    assert_eq!(parsed.rows.len(), 3);
    assert_eq!(parsed.rows[0].description, "FUNDS RECEIVED From XXXXXX0000");
    assert_eq!(parsed.rows[1].amount, decimal("-137.4"));
    assert_eq!(parsed.rows[1].occurred_on, Some(day("2026-03-01")));
    assert_eq!(parsed.check.status, Status::Verified);
}

fn bank_rows(lines: &[&str]) -> Vec<StatementRow> {
    let csv = format!("Date,Description,Amount\n{}\n", lines.join("\n"));
    parse(csv.as_bytes()).unwrap().rows
}

fn provider(date: &str, amount: &str) -> ProviderActivity {
    described(date, amount, date, "")
}

fn described(date: &str, amount: &str, id: &str, description: &str) -> ProviderActivity {
    ProviderActivity {
        id: id.into(),
        date: day(date),
        amount: decimal(amount),
        description: description.into(),
    }
}

#[test]
fn statements_fill_only_the_days_before_the_provider_reports_activity() {
    let rows = bank_rows(&[
        "03/20/2026,CAFE,-3.10",
        "03/14/2026,TRANSFER IN,500.00",
        "03/13/2026,GROCER,-23.40",
        "02/01/2026,INTEREST,0.25",
    ]);
    // Plaid starts on 03/14 and reports the 03/13 purchase a day later.
    let activity = [
        provider("2026-03-14", "-23.40"),
        provider("2026-03-20", "-3.10"),
    ];
    let plan = plan("plaid:cash", &[rows.as_slice()], &activity, None);
    assert_eq!(plan.provider_start, Some(day("2026-03-14")));
    assert_eq!(plan.counts[0].covered, 3);
    assert_eq!(plan.counts[0].added, 1);
    assert_eq!(plan.rows[0].row.posted_on, day("2026-02-01"));
}

#[test]
fn overlapping_statements_count_shared_rows_once_but_keep_identical_rows_within_one() {
    let year = bank_rows(&[
        "03/02/2026,VENDING,-1.50",
        "03/02/2026,VENDING,-1.50",
        "02/01/2026,INTEREST,0.25",
    ]);
    let month = bank_rows(&["03/02/2026,VENDING,-1.50", "03/01/2026,CAFE,-3.10"]);
    let plan = plan(
        "plaid:cash",
        &[year.as_slice(), month.as_slice()],
        &[],
        None,
    );
    assert_eq!(plan.rows.len(), 4);
    assert_eq!(plan.counts[0].added, 3);
    assert_eq!(plan.counts[1].added, 1);
    assert_eq!(plan.counts[1].repeated, 1);
}

#[test]
fn row_ids_are_stable_across_statements_and_accounts() {
    let year = bank_rows(&["03/02/2026,VENDING,-1.50", "02/01/2026,INTEREST,0.25"]);
    let month = bank_rows(&["03/02/2026,VENDING,-1.50"]);
    let ids = |statements: &[&[StatementRow]], account: &str| {
        let mut ids = plan(account, statements, &[], None)
            .rows
            .into_iter()
            .map(|planned| planned.id)
            .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    let both = ids(&[month.as_slice(), year.as_slice()], "plaid:cash");
    assert_eq!(both, ids(&[year.as_slice()], "plaid:cash"));
    assert!(ids(&[month.as_slice()], "plaid:cash")
        .iter()
        .all(|id| both.contains(id)));
    assert_ne!(
        ids(&[month.as_slice()], "plaid:other"),
        ids(&[month.as_slice()], "plaid:cash")
    );
    assert!(both
        .iter()
        .all(|id| id.starts_with("statement:") && id.len() < 60));
}

/// A card statement for October and what Plaid reported over the same days.
fn october() -> (Vec<StatementRow>, Vec<ProviderActivity>) {
    let rows = bank_rows(&[
        "10/31/2026,INTEREST CHARGE,-0.10",
        "10/10/2026,HARDWARE STORE,-12.00",
        "10/05/2026,RESTAURANT XYZ,-48.00",
        "10/02/2026,BLUE BOTTLE COFFEE,-6.50",
        "10/01/2026,PAYMENT THANK YOU,500.00",
    ]);
    let plaid = vec![
        described("2026-10-01", "500.00", "payment", "PAYMENT THANK YOU"),
        described("2026-10-03", "-6.50", "coffee", "BLUE BOTTLE COFFEE"),
        // The tip is on the statement only.
        described("2026-10-05", "-40.00", "dinner", "RESTAURANT XYZ"),
        described("2026-10-12", "-30.00", "parking", "CITY PARKING"),
        // Near the statement's end, so it may belong to November's.
        described("2026-10-30", "-5.00", "late", "TRANSIT"),
        described("2026-10-31", "-0.10", "interest", "INTEREST CHARGE"),
    ];
    (rows, plaid)
}

#[test]
fn a_statement_is_reconciled_against_plaid_over_the_days_plaid_covers() {
    let (rows, plaid) = october();
    let plan = plan(
        "plaid:card",
        &[rows.as_slice()],
        &plaid,
        Some(&BTreeSet::new()),
    );
    let checked = plan.reconciliation.unwrap();
    assert_eq!(
        (checked.from, checked.through),
        (day("2026-10-01"), day("2026-10-31"))
    );
    assert_eq!(checked.matched, ["coffee", "interest", "payment"]);
    assert_eq!(
        checked.amount_differs,
        [("dinner".into(), decimal("-48.00"))]
    );
    assert_eq!(checked.extra, ["parking"]);
    let [missing] = checked.missing.as_slice() else {
        panic!("one row Plaid never reported")
    };
    assert_eq!(missing.planned.row.description, "HARDWARE STORE");
    assert!(!missing.confirmed);
    // Nothing joins history until the user confirms it.
    assert!(plan.rows.is_empty());
    assert_eq!(
        plan.counts[0],
        Counts {
            covered: 4,
            missing: 1,
            ..Counts::default()
        }
    );
}

#[test]
fn a_confirmed_row_fills_history_only_until_plaid_reports_it() {
    let (rows, mut plaid) = october();
    let unconfirmed = plan(
        "plaid:card",
        &[rows.as_slice()],
        &plaid,
        Some(&BTreeSet::new()),
    );
    let id = unconfirmed.reconciliation.unwrap().missing[0]
        .planned
        .id
        .clone();
    let confirmed = BTreeSet::from([id.clone()]);

    let filled = plan("plaid:card", &[rows.as_slice()], &plaid, Some(&confirmed));
    assert_eq!(filled.rows.len(), 1);
    assert_eq!(filled.rows[0].id, id);
    assert!(filled.reconciliation.unwrap().missing[0].confirmed);

    plaid.push(described(
        "2026-10-11",
        "-12.00",
        "hardware",
        "HARDWARE STORE",
    ));
    let reported = plan("plaid:card", &[rows.as_slice()], &plaid, Some(&confirmed));
    assert!(reported.rows.is_empty());
    let checked = reported.reconciliation.unwrap();
    assert!(checked.missing.is_empty());
    assert!(checked.matched.contains(&"hardware".to_string()));
}

#[test]
fn identical_charges_pair_one_to_one_and_unreconciled_accounts_trust_the_provider() {
    let rows = bank_rows(&[
        "10/20/2026,VENDING,-1.50",
        "10/03/2026,VENDING,-1.50",
        "10/03/2026,VENDING,-1.50",
        "10/01/2026,CAFE,-3.10",
    ]);
    let plaid = [
        described("2026-10-01", "-3.10", "cafe", "CAFE"),
        described("2026-10-03", "-1.50", "first", "VENDING"),
        described("2026-10-04", "-1.50", "second", "VENDING"),
        described("2026-10-20", "-1.50", "third", "VENDING"),
    ];
    let checked = plan(
        "plaid:card",
        &[rows.as_slice()],
        &plaid,
        Some(&BTreeSet::new()),
    )
    .reconciliation
    .unwrap();
    assert_eq!(checked.matched.len(), 4);
    assert!(checked.missing.is_empty() && checked.extra.is_empty());

    // Without reconciliation, rows in the provider's days are taken as reported.
    let trusted = plan("snaptrade:brokerage", &[rows.as_slice()], &plaid[..1], None);
    assert!(trusted.reconciliation.is_none() && trusted.rows.is_empty());
    assert_eq!(trusted.counts[0].covered, 4);
}

#[test]
fn a_card_export_listing_charges_as_positive_takes_its_sign_from_plaid() {
    // Charges positive, as a card issuer's export lists them, with no total to settle it.
    let overlapping = bank_rows(&[
        "10/12/2026,HARDWARE STORE,12.00",
        "10/03/2026,CAFE,6.50",
        "10/02/2026,GROCER,40.00",
    ]);
    let earlier = bank_rows(&["09/01/2026,BOOKSHOP,20.00"]);
    let plaid = [
        described("2026-10-02", "-40.00", "grocer", "GROCER"),
        described("2026-10-03", "-6.50", "cafe", "CAFE"),
    ];
    let plan = plan(
        "plaid:card",
        &[earlier.as_slice(), overlapping.as_slice()],
        &plaid,
        Some(&BTreeSet::new()),
    );
    // The earlier statement never meets Plaid, so it follows the account's other statement.
    assert_eq!(plan.rows.len(), 1);
    assert_eq!(plan.rows[0].amount, decimal("-20.00"));
    let checked = plan.reconciliation.unwrap();
    assert_eq!(checked.missing[0].planned.amount, decimal("-12.00"));
}

#[test]
fn reads_a_retirement_plan_export_of_contributions_buying_fund_units() {
    let csv = "\"VALUATION DATE\",\"POSTING DATE\",\"ACTIVITY TYPE\",\"PLAN\",\"ACCOUNT\",\"FUND\",\"AMOUNT\",\"FUND NAV/PRICE\",\"FUND UNITS\",\n\
               \"07-17-2026\",\"07-17-2026\",\"Before-Tax\",\"401(k) Plan\",\"Before-Tax\",\"Sample Index Fund\",\"100.00\",\"50.000000\",\"2.000000\",\n\
               \"07-31-2026\",\"07-31-2026\",\"Employer Match\",\"401(k) Plan\",\"Match\",\"Sample Index Fund\",\"40.00\",\"40.000000\",\"1.000000\",\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(parsed.rows.len(), 2);
    let row = &parsed.rows[0];
    assert_eq!(row.activity, Activity::Contribution);
    assert_eq!(row.posted_on, day("2026-07-17"));
    assert_eq!(row.symbol.as_deref(), Some("Sample Index Fund"));
    assert_eq!(row.quantity, Some(decimal("2")));
    assert_eq!(row.price, Some(decimal("50")));
    assert_eq!(parsed.rows[1].activity, Activity::Contribution);
}

#[test]
fn saved_files_must_match_their_content_id() {
    let contents = b"Date,Description,Amount\n03/02/2026,CAFE,-3.10\n".to_vec();
    let mut file = StatementFile {
        id: content_id(&contents),
        account_id: "plaid:cash".into(),
        file_name: "statement.csv".into(),
        imported_at: "2026-03-05T12:00:00Z".into(),
        contents,
        confirmed_rows: Default::default(),
    };
    assert!(file.validate().is_ok());
    file.contents.push(b'\n');
    assert!(file.validate().is_err());
}

#[test]
fn reads_what_a_card_issuers_export_says_about_each_merchant() {
    let csv = "Date,Description,Amount,Extended Details,Appears On Your Statement As,Address,City/State,Zip Code,Country,Reference,Category\n\
               05/04/2026,AplPay TST* EXAMPLE TAQSAN JOSE,12.40,\"99999996131 4085550142\nAplPay TST* EXAMPLE TAQUERIA - 000067640\nSAN JOSE\nCA\",AplPay TST* EXAMPLE TAQSAN JOSE,368 EXAMPLE ROW,\"SAN JOSE\nCA\",95128,UNITED STATES,'320261240000000000',Restaurant-Bar & Café\n\
               05/06/2026,EXAMPLE AIRLINES HOUSTON,8.00,\"16281873    WWW.EXAMPLE.COM\nWIFI\nPassenger Name : SAMPLE/PERSON\",EXAMPLE AIRLINES HOUSTON,\"HQ - 12TH\nFLOOR 609 MAIN STREET\",\"HOUSTON\nTX\",77002,UNITED STATES,'320261260000000000',Travel-Airline\n\
               05/07/2026,EXAMPLE CREDIT,-5.00,EXAMPLE CREDIT,EXAMPLE CREDIT,,,,,'320261270000000000',Fees & Adjustments-Fees & Adjustments\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    let issuer = parsed.rows[0].issuer.as_ref().unwrap();
    assert_eq!(
        issuer,
        &IssuerDetail {
            name: Some("AplPay TST* EXAMPLE TAQUERIA".into()),
            address: Some("368 EXAMPLE ROW".into()),
            city: Some("SAN JOSE".into()),
            region: Some("CA".into()),
            postal_code: Some("95128".into()),
            country: Some("US".into()),
            category: Some("Restaurant-Bar & Café".into()),
            phone: Some("408-555-0142".into()),
        }
    );
    // Only the name and phone are read from the details, never a passenger record.
    let airline = parsed.rows[1].issuer.as_ref().unwrap();
    assert_eq!(airline.name, None);
    assert_eq!(
        airline.address.as_deref(),
        Some("HQ - 12TH FLOOR 609 MAIN STREET")
    );
    assert_eq!(airline.category.as_deref(), Some("Travel-Airline"));
    assert_eq!(parsed.rows[2].issuer.as_ref().unwrap().address, None);
}

#[test]
fn keeps_a_brokerages_tax_year_note_and_reads_a_payroll_deposit_as_wages() {
    let csv = "Account Activity for Roth IRA -0000 from Current Year\n\n\
               Activity/Trade Date,Transaction Date,Activity Type,Description,Amount $\n\
               05/19/2026,05/19/2026,Contributions,TY 2026 $ 7500 INDIVID CONTRIB - CURR YR; FUNDS RECEIVED,0.0\n\
               05/19/2026,05/19/2026,Contributions,TRANSFER FROM XXXXXX0000,7500.0\n\
               06/18/2026,06/18/2026,DEPOSIT,EXAMPLE PAYROLL DEPOSIT,2937.35\n\
               06/19/2026,06/19/2026,DEPOSIT,FUNDS RECEIVED,20.0\n";
    let parsed = parse(csv.as_bytes()).unwrap();
    assert_eq!(
        parsed.memos,
        vec![ContributionMemo {
            posted_on: day("2026-05-19"),
            tax_year: 2026,
            amount: decimal("7500"),
        }]
    );
    assert_eq!(parsed.rows.len(), 3);
    assert_eq!(parsed.rows[1].activity, Activity::Wages);
    assert_eq!(parsed.rows[2].activity, Activity::Other);
}
