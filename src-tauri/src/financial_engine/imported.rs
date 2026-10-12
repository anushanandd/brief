//! Statement rows projected beside provider records. Rows for a Plaid cash or card account
//! take the shape of Plaid records, so Plaid's categories, the transaction policy, and the
//! reconstructed balance history treat them like any other transaction.
use super::*;
use crate::{
    providers::{PlaidData, SnapTradeData},
    statements::{self, Activity, ProviderActivity, StatementFile, StatementRow},
};

pub(super) struct Imported {
    pub(super) plaid_transactions: Vec<PlaidTransaction>,
    /// The earliest imported day per Plaid account, where its balance history now begins.
    pub(super) history_starts: BTreeMap<String, NaiveDate>,
    pub(super) transactions: Vec<Transaction>,
    pub(super) trades: Vec<Trade>,
}

/// Activity each provider already reports, by projected account id.
fn activity_from(
    plaid_transactions: &[PlaidTransaction],
    snap_activities: &BTreeMap<String, Vec<SnapActivity>>,
) -> BTreeMap<String, Vec<ProviderActivity>> {
    let mut activity = BTreeMap::<String, Vec<ProviderActivity>>::new();
    for transaction in plaid_transactions
        .iter()
        .filter(|transaction| !transaction.pending)
    {
        let Some(date) = plaid_posted_date(transaction)
            .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
        else {
            continue;
        };
        activity
            .entry(format!("plaid:{}", transaction.account_id))
            .or_default()
            .push(ProviderActivity {
                date,
                amount: -transaction.amount,
            });
    }
    for (account, activities) in snap_activities {
        for item in activities {
            let Some(date) = item
                .date()
                .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
            else {
                continue;
            };
            activity
                .entry(format!("snaptrade:{account}"))
                .or_default()
                .push(ProviderActivity {
                    date,
                    amount: item.amount,
                });
        }
    }
    activity
}

/// Activity each provider already reports in saved provider data, by projected account id.
pub(crate) fn provider_activity(
    plaid: &PlaidData,
    snaptrade: &SnapTradeData,
) -> BTreeMap<String, Vec<ProviderActivity>> {
    let mut ignored = Vec::new();
    let transactions =
        parse_records::<PlaidTransaction>(&plaid.transactions, "Plaid transaction", &mut ignored);
    let activities = snaptrade
        .activities
        .iter()
        .map(|(id, records)| {
            (
                id.clone(),
                parse_records::<SnapActivity>(records, "SnapTrade activity", &mut ignored),
            )
        })
        .collect();
    activity_from(&transactions, &activities)
}

/// Plaid's category for an activity type that names its meaning outright. Other rows have
/// none, and the transaction policy reads their description.
fn plaid_category(row: &StatementRow) -> Option<PlaidCategory> {
    let inflow = row.amount.is_sign_positive();
    let (primary, detailed) = match row.activity {
        Activity::Interest if inflow => ("INCOME", "INCOME_INTEREST_EARNED"),
        Activity::Interest => ("BANK_FEES", "BANK_FEES_INTEREST_CHARGE"),
        Activity::Dividend if inflow => ("INCOME", "INCOME_DIVIDENDS"),
        Activity::Wages if inflow => ("INCOME", "INCOME_WAGES"),
        Activity::Transfer if inflow => ("TRANSFER_IN", "TRANSFER_IN_ACCOUNT_TRANSFER"),
        Activity::Transfer => ("TRANSFER_OUT", "TRANSFER_OUT_ACCOUNT_TRANSFER"),
        Activity::PaymentApp if inflow => ("TRANSFER_IN", "TRANSFER_IN_TRANSFER_IN_FROM_APPS"),
        Activity::PaymentApp => ("TRANSFER_OUT", "TRANSFER_OUT_TRANSFER_OUT_FROM_APPS"),
        Activity::CashWithdrawal if !inflow => ("TRANSFER_OUT", "TRANSFER_OUT_WITHDRAWAL"),
        Activity::Fee => ("BANK_FEES", "BANK_FEES_OTHER_BANK_FEES"),
        Activity::Contribution if inflow => {
            ("TRANSFER_IN", "TRANSFER_IN_INVESTMENT_AND_RETIREMENT_FUNDS")
        }
        _ => return None,
    };
    Some(PlaidCategory {
        primary: primary.into(),
        detailed: Some(detailed.into()),
        confidence_level: None,
    })
}

fn description(row: &StatementRow) -> String {
    text(
        Some(row.description.as_str()).filter(|value| !value.trim().is_empty()),
        text(Some(row.activity_label.as_str()), "Statement activity").as_str(),
    )
}

fn plaid_record(id: String, account_id: &str, row: &StatementRow) -> PlaidTransaction {
    PlaidTransaction {
        transaction_id: id,
        account_id: account_id.to_owned(),
        amount: -row.amount,
        name: Some(description(row)),
        merchant_name: None,
        logo_url: None,
        website: None,
        pending: false,
        date: Some(row.posted_on.to_string()),
        datetime: None,
        authorized_date: row.occurred_on.map(|date| date.to_string()),
        authorized_datetime: None,
        location: None,
        payment_channel: None,
        category: None,
        personal_finance_category: plaid_category(row),
        counterparties: None,
        transaction_code: None,
        pending_transaction_id: None,
        merchant_entity_id: None,
        original_description: None,
        check_number: None,
        account_owner: None,
    }
}

/// Investment account rows, labeled like SnapTrade activity in the same accounts.
fn investment_rows(
    id: &str,
    row: &StatementRow,
    account: &str,
    account_id: &str,
    transactions: &mut Vec<Transaction>,
    trades: &mut Vec<Trade>,
) {
    let ticker = row.symbol.clone().unwrap_or_default();
    let date = row.posted_on.to_string();
    let posted = row.settled_on.unwrap_or(row.posted_on).to_string();
    let row_transaction =
        |id: String, merchant: String, category: &str, amount: Decimal| Transaction {
            description: Some(description(row)),
            occurred_on: Some(date.clone()),
            posted_on: Some(posted.clone()),
            ..cash_row(
                id,
                merchant,
                category,
                date.clone(),
                amount,
                account,
                account_id,
            )
        };
    if let Some(fee) = row.fee.filter(|_| row.activity != Activity::Fee) {
        transactions.push(row_transaction(
            format!("{id}:fee"),
            "Investment fee".into(),
            "Fees",
            -fee.abs(),
        ));
    }
    if row.activity.is_trade() {
        let buy = row.activity == Activity::Buy;
        trades.push(Trade {
            id: id.into(),
            r#type: if buy { "BUY" } else { "SELL" }.into(),
            date: date.clone(),
            amount: money(if buy {
                -row.amount.abs()
            } else {
                row.amount.abs()
            }),
            account: account.into(),
            account_id: account_id.into(),
            ticker: row.symbol.clone(),
            description: Some(description(row)),
            units: row.quantity.map(|units| scaled(units.abs(), 8)),
            price: row.price.map(|price| scaled(price.abs(), 8)),
            ..Trade::default()
        });
        return;
    }
    let labeled = |label: &str| {
        if ticker.is_empty() {
            label.to_owned()
        } else {
            format!("{label} · {ticker}")
        }
    };
    let (merchant, category) = match row.activity {
        Activity::Dividend => (labeled("Dividend"), "Dividend"),
        Activity::Interest => ("Interest".to_owned(), "Interest"),
        Activity::Fee => ("Investment fee".to_owned(), "Fees"),
        Activity::Transfer
        | Activity::Contribution
        | Activity::PaymentApp
        | Activity::Wages
        | Activity::CashWithdrawal => (
            if row.amount.is_sign_negative() {
                "Transfer out"
            } else {
                "Transfer in"
            }
            .to_owned(),
            "Transfer",
        ),
        _ => (description(row), "Other"),
    };
    transactions.push(row_transaction(id.into(), merchant, category, row.amount));
}

/// Plans every saved statement against what providers report and shapes the rows that fill
/// history. A statement whose account is gone, or that can no longer be read, is left out
/// with a warning and stays saved.
pub(super) fn project_imported(
    files: &[StatementFile],
    accounts: &[Account],
    plaid_transactions: &[PlaidTransaction],
    snap_activities: &BTreeMap<String, Vec<SnapActivity>>,
    warnings: &mut Vec<String>,
) -> Imported {
    let mut imported = Imported {
        plaid_transactions: Vec::new(),
        history_starts: BTreeMap::new(),
        transactions: Vec::new(),
        trades: Vec::new(),
    };
    if files.is_empty() {
        return imported;
    }
    let activity = activity_from(plaid_transactions, snap_activities);
    let mut by_account = BTreeMap::<&str, Vec<(&StatementFile, statements::Parsed)>>::new();
    for file in files {
        match statements::parse(&file.contents) {
            Ok(parsed) => by_account
                .entry(file.account_id.as_str())
                .or_default()
                .push((file, parsed)),
            Err(_) => warnings.push("Statement file can no longer be read".into()),
        }
    }
    for (account_id, mut saved) in by_account {
        let Some(account) = accounts
            .iter()
            .find(|account| account.id == account_id && account_id != "all")
        else {
            warnings.push("Statement history for an unavailable account is hidden".into());
            continue;
        };
        let name = text(Some(&account.name), "Account");
        let investment = matches!(account.r#type.as_str(), "brokerage" | "retirement");
        let plaid_cash = account_id
            .strip_prefix("plaid:")
            .filter(|_| matches!(account.r#type.as_str(), "cash" | "credit"));
        if !investment && saved.iter().any(|(_, parsed)| parsed.has_trades()) {
            warnings.push(format!(
                "Statement trades for {name} need an investment account"
            ));
            saved.retain(|(_, parsed)| !parsed.has_trades());
        }
        saved.sort_by(|left, right| {
            (left.0.imported_at.as_str(), left.0.id.as_str())
                .cmp(&(right.0.imported_at.as_str(), right.0.id.as_str()))
        });
        let rows = saved
            .iter()
            .map(|(_, parsed)| parsed.rows.as_slice())
            .collect::<Vec<_>>();
        let plan = statements::plan(
            account_id,
            &rows,
            activity
                .get(account_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        for planned in plan.rows {
            match plaid_cash {
                Some(raw) => {
                    let start = imported
                        .history_starts
                        .entry(raw.to_owned())
                        .or_insert(planned.row.posted_on);
                    *start = (*start).min(planned.row.posted_on);
                    imported
                        .plaid_transactions
                        .push(plaid_record(planned.id, raw, planned.row));
                }
                None => investment_rows(
                    &planned.id,
                    planned.row,
                    &name,
                    account_id,
                    &mut imported.transactions,
                    &mut imported.trades,
                ),
            }
        }
    }
    imported
}
