//! Account statements imported from files. Rust reads the table, checks it against the
//! statement's own totals and running balances, and decides which rows fill history a
//! provider never reported. Rows are never inferred by a model or invented.
//!
//! A saved file's bytes are authoritative. Rows are parsed again on every projection, so a
//! parser fix applies to earlier imports, and which rows count depends on the provider
//! history at that time.
use std::{collections::BTreeSet, sync::LazyLock};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

mod decode;
mod issuer;
mod plan;
mod table;
mod verify;

#[cfg(test)]
mod tests;

pub use issuer::IssuerDetail;
pub use plan::{plan, row_ids, Counts, ProviderActivity};
pub use verify::{Check, Status};

/// Statement exports are small; this bounds memory and the database.
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_FILES: usize = 500;

/// A saved statement file and the account its rows belong to. `id` is the SHA-256 of the
/// contents, so the same file cannot be imported twice.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatementFile {
    pub id: String,
    pub account_id: String,
    pub file_name: String,
    pub imported_at: String,
    pub contents: Vec<u8>,
    /// Rows the provider never reported that the user confirmed, by row id; they fill history
    /// until the provider reports them.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub confirmed_rows: BTreeSet<String>,
}

impl StatementFile {
    pub fn validate(&self) -> Result<(), String> {
        let hex = self.id.len() == 64 && self.id.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !hex
            || self.id != content_id(&self.contents)
            || self.account_id.is_empty()
            || self.account_id.len() > 220
            || self.file_name.is_empty()
            || self.file_name.len() > 255
            || self.contents.is_empty()
            || self.contents.len() > MAX_FILE_BYTES
            || chrono::DateTime::parse_from_rfc3339(&self.imported_at).is_err()
            || self.confirmed_rows.len() > 10_000
            || self
                .confirmed_rows
                .iter()
                .any(|id| !id.starts_with("statement:") || id.len() > 60)
        {
            return Err("Invalid saved statement".into());
        }
        Ok(())
    }
}

pub fn content_id(contents: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(contents)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// What a row means, from the institution's activity type. Unknown types stay `Other`, and
/// the transaction policy reads their description like any uncategorized transaction.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    Interest,
    Dividend,
    Wages,
    Transfer,
    PaymentApp,
    CashWithdrawal,
    Fee,
    Contribution,
    Buy,
    Sell,
    Other,
}

impl Activity {
    pub fn is_trade(self) -> bool {
        matches!(self, Self::Buy | Self::Sell)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatementRow {
    /// The date the institution recorded the activity.
    pub posted_on: NaiveDate,
    /// When it happened, if the statement reports a different date.
    pub occurred_on: Option<NaiveDate>,
    pub settled_on: Option<NaiveDate>,
    pub activity: Activity,
    pub activity_label: String,
    pub description: String,
    /// Signed: positive moves money into the account.
    pub amount: Decimal,
    pub balance: Option<Decimal>,
    pub symbol: Option<String>,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub fee: Option<Decimal>,
    pub pending: bool,
    /// What a card issuer's export adds about the merchant: address, category, fuller name.
    pub issuer: Option<IssuerDetail>,
}

/// A row that carries no money but says which tax year an IRA contribution counts toward.
#[derive(Clone, Debug, PartialEq)]
pub struct ContributionMemo {
    pub posted_on: NaiveDate,
    pub tax_year: i32,
    pub amount: Decimal,
}

/// What the statement says about the account it came from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountHint {
    pub label: Option<String>,
    pub last_four: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub rows: Vec<StatementRow>,
    pub hint: AccountHint,
    pub check: Check,
    /// The file never showed a day above 12, so dates were read month first.
    pub dates_assumed: bool,
    pub memos: Vec<ContributionMemo>,
}

impl Parsed {
    pub fn has_trades(&self) -> bool {
        self.rows.iter().any(|row| row.activity.is_trade())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vocabulary {
    columns: table::Columns,
    pending_values: Vec<String>,
    activities: std::collections::BTreeMap<Activity, Vec<String>>,
    account_labels: Vec<String>,
    total_labels: Vec<String>,
}

static VOCABULARY: LazyLock<Vocabulary> = LazyLock::new(|| {
    serde_json::from_str(include_str!("vocabulary.json")).expect("statement vocabulary is valid")
});

fn activity_for(label: &str) -> Activity {
    let label = table::normalized(label);
    VOCABULARY
        .activities
        .iter()
        .find(|(_, names)| names.contains(&label))
        .map_or(Activity::Other, |(activity, _)| *activity)
}

/// A generic deposit whose description says it is pay, such as "DEPOSIT" for "MS PAYROLL
/// DEPOSIT", is wages.
fn deposit_activity(label: &str, description: &str, inflow: bool) -> Activity {
    match activity_for(label) {
        Activity::Other if inflow => {
            let description = format!(" {} ", table::normalized(description));
            let pay = VOCABULARY.activities.get(&Activity::Wages);
            if pay.is_some_and(|names| {
                names
                    .iter()
                    .any(|name| description.contains(&format!(" {name} ")))
            }) {
                Activity::Wages
            } else {
                Activity::Other
            }
        }
        activity => activity,
    }
}

/// Reads a CSV or XLSX statement export. Fails when the table can't be found or a row in it
/// can't be read; a checked total that disagrees is reported in `check`, not as an error.
pub fn parse(contents: &[u8]) -> Result<Parsed, String> {
    let grid = decode::grid(contents)?;
    let table = table::read(&grid)?;
    let check = verify::check(&table.rows, table.total);
    let mut rows = table.rows;
    if check.flip_signs {
        for row in &mut rows {
            row.amount = -row.amount;
        }
    }
    Ok(Parsed {
        rows,
        hint: table.hint,
        check: check.check,
        dates_assumed: table.dates_assumed,
        memos: table.memos,
    })
}
