//! Imported statement files.
use super::*;

pub(super) fn storage_with_plaid_cash(directory: &Path) -> Storage {
    let mut storage = Storage::new(directory.to_path_buf()).unwrap();
    storage.data.snapshot.updated_at = "2026-09-10T07:30:00Z".into();
    storage.data.snapshot.calculation_version = Some(1);
    storage.data.provider_data.plaid = Some(crate::providers::PlaidData {
        accounts: vec![serde_json::json!({
            "account_id": "cash", "name": "Checking", "institution_name": "Bank",
            "type": "depository", "mask": "0042",
            "balances": { "current": 100, "iso_currency_code": "USD" }
        })],
        transactions: vec![serde_json::json!({
            "transaction_id": "plaid-coffee", "account_id": "cash", "amount": 4.5,
            "name": "Coffee", "pending": false, "date": "2026-09-08"
        })],
        ..Default::default()
    });
    assert!(storage.upgrade_projection().unwrap());
    storage
}

const CHECKING_STATEMENT: &str = "Account Activity for Checking -0042\n\
    Date,Description,Amount\n\
    09/08/2026,COFFEE,-4.50\n\
    08/20/2026,PAYROLL EXAMPLE,250.00\n\
    08/03/2026,GROCER,-31.25\n";

fn statement_ids(storage: &Storage) -> Vec<String> {
    storage.snapshot().unwrap()["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["id"].as_str())
        .filter(|id| id.starts_with("statement:"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn imported_statements_fill_history_persist_and_can_be_removed() {
    let directory = directory();
    let mut storage = storage_with_plaid_cash(&directory);
    let (preview, draft) = storage
        .preview_statement(
            "checking.csv".into(),
            CHECKING_STATEMENT.as_bytes().to_vec(),
        )
        .unwrap();
    assert_eq!(preview.suggested_account_id.as_deref(), Some("plaid:cash"));
    assert_eq!(preview.rows, 3);
    assert_eq!(preview.money_in, 250.0);
    assert_eq!(preview.money_out, 35.75);
    let fit = &preview.accounts[0];
    assert_eq!(fit.provider_start.as_deref(), Some("2026-09-08"));
    assert_eq!((fit.counts.added, fit.counts.covered), (2, 1));

    let revision = storage.data.revision;
    storage
        .import_statement(draft.clone(), "plaid:cash")
        .unwrap();
    assert_eq!(storage.data.revision, revision + 1);
    assert_eq!(statement_ids(&storage).len(), 2);
    assert_eq!(
        storage.import_statement(draft, "plaid:cash").unwrap_err(),
        "This statement is already imported"
    );
    let (again, _) = storage
        .preview_statement(
            "checking.csv".into(),
            CHECKING_STATEMENT.as_bytes().to_vec(),
        )
        .unwrap();
    assert_eq!(again.imported_account_id.as_deref(), Some("plaid:cash"));

    drop(storage);
    let mut storage = Storage::new(directory.clone()).unwrap();
    assert_eq!(statement_ids(&storage).len(), 2);
    let summaries = storage.statement_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].counts.added, 2);
    assert_eq!(summaries[0].first_date.as_deref(), Some("2026-08-03"));

    let id = summaries[0].id.clone();
    storage.remove_statement(&id).unwrap();
    assert!(statement_ids(&storage).is_empty());
    assert!(storage.data.statements.is_empty());
    assert!(storage.remove_statement(&id).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn statements_that_fail_their_total_or_hold_trades_for_a_cash_account_are_refused() {
    let directory = directory();
    let mut storage = storage_with_plaid_cash(&directory);
    let failing = "Total:,10.00\nDate,Description,Amount\n08/03/2026,GROCER,-31.25\n";
    let (preview, draft) = storage
        .preview_statement("bad.csv".into(), failing.as_bytes().to_vec())
        .unwrap();
    assert_eq!(preview.check.status, crate::statements::Status::Failed);
    assert!(storage.import_statement(draft, "plaid:cash").is_err());

    let trades = "Date,Activity Type,Description,Symbol,Quantity,Price,Amount\n\
        08/01/2026,Bought,SAMPLE FUND,SMPL,3,41.20,-123.60\n";
    let (preview, draft) = storage
        .preview_statement("trades.csv".into(), trades.as_bytes().to_vec())
        .unwrap();
    assert!(preview.accounts.is_empty());
    assert_eq!(
        storage.import_statement(draft, "plaid:cash").unwrap_err(),
        "Statements with trades belong to an investment account"
    );
    assert!(storage.data.statements.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_plan_export_values_a_manual_account_entered_without_a_balance() {
    let directory = directory();
    let mut storage = storage_with_plaid_cash(&directory);
    let mut settings = storage.data.settings.clone();
    settings
        .manual_accounts
        .push(crate::settings::ManualAccount {
            id: "manual:plan".into(),
            name: "Workplace plan".into(),
            kind: "retirement".into(),
            entries: Vec::new(),
            tracks_sp500: false,
        });
    storage.save_settings(settings, false).unwrap();
    let plan = "\"POSTING DATE\",\"ACTIVITY TYPE\",\"PLAN\",\"FUND\",\"AMOUNT\",\"FUND NAV/PRICE\",\"FUND UNITS\"\n\
        \"08-01-2026\",\"Before-Tax\",\"Sample Plan\",\"Sample Index Fund\",\"100.00\",\"50.00\",\"2.00\"\n\
        \"08-15-2026\",\"Before-Tax\",\"Sample Plan\",\"Sample Index Fund\",\"60.00\",\"60.00\",\"1.00\"\n";
    let (preview, draft) = storage
        .preview_statement("plan.csv".into(), plan.as_bytes().to_vec())
        .unwrap();
    let fit = preview
        .accounts
        .iter()
        .find(|fit| fit.account_id == "manual:plan")
        .expect("a manual account can take a statement");
    assert_eq!(fit.counts.added, 2);
    storage.import_statement(draft, "manual:plan").unwrap();

    let snapshot = storage.snapshot().unwrap();
    let account = snapshot["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "manual:plan")
        .unwrap();
    // Three units at the latest price, 60.
    assert_eq!(account["value"], 180.0);
    assert_eq!(account["name"], "Workplace plan");
    // A fund without a market price is listed on its account, not among holdings.
    assert!(snapshot["holdings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|holding| holding["accountId"] != "manual:plan"));
    assert_eq!(
        account["statementPositions"][0]["name"],
        "Sample Index Fund"
    );
    assert_eq!(account["statementPositions"][0]["units"], 3.0);
    // The plan's own label names the source: the employee's deferral.
    assert!(snapshot["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["accountId"] == "manual:plan")
        .all(|row| row["account"] == "Workplace plan" && row["flow"] == "deferral"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn erasure_removes_imported_statements() {
    let directory = directory();
    let mut storage = storage_with_plaid_cash(&directory);
    let (_, draft) = storage
        .preview_statement(
            "checking.csv".into(),
            CHECKING_STATEMENT.as_bytes().to_vec(),
        )
        .unwrap();
    storage.import_statement(draft, "plaid:cash").unwrap();
    storage.erase().unwrap();
    assert!(storage.data.statements.is_empty());
    drop(storage);
    assert!(Storage::new(directory.clone())
        .unwrap()
        .data
        .statements
        .is_empty());
    fs::remove_dir_all(directory).unwrap();
}
