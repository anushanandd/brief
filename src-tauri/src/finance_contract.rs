//! Shared native output contract. Provider compatibility JSON is converted at
//! the projection boundary; storage validates these same types.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The part of a committed snapshot live prices revalue, without its ledger and history.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketProjection {
    #[serde(default)]
    pub revision: u64,
    pub updated_at: String,
    pub net_worth: f64,
    #[serde(default)]
    pub net_worth_incomplete: bool,
    #[serde(default)]
    pub net_worth_provisional: bool,
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
    pub brokerage_performance: Vec<CurrentPerformance>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentPerformance {
    pub account_id: String,
    pub current_value: f64,
}

impl From<&Snapshot> for MarketProjection {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            revision: snapshot.revision.unwrap_or_default(),
            updated_at: snapshot.updated_at.clone(),
            net_worth: snapshot.net_worth,
            net_worth_incomplete: snapshot.net_worth_incomplete,
            net_worth_provisional: snapshot.net_worth_provisional,
            accounts: snapshot.accounts.clone(),
            holdings: snapshot.holdings.clone(),
            brokerage_performance: snapshot
                .brokerage_performance
                .iter()
                .map(|performance| CurrentPerformance {
                    account_id: performance.account_id.clone(),
                    current_value: performance.current_value,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calculation_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_status: Option<BTreeMap<String, ProviderSyncStatus>>,
    #[serde(default)]
    pub account_links: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_date: Option<String>,
    pub net_worth: f64,
    #[serde(default)]
    pub net_worth_incomplete: bool,
    #[serde(default)]
    pub net_worth_provisional: bool,
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
    #[serde(default)]
    pub trades: Vec<Trade>,
    pub transactions: Vec<Transaction>,
    #[serde(default)]
    pub account_movements: Vec<AccountMovement>,
    pub spending: Spending,
    pub net_worth_history: Vec<Point>,
    #[serde(default)]
    pub net_worth_history_estimated: bool,
    #[serde(default)]
    pub benchmark_history: Vec<Point>,
    #[serde(default)]
    pub brokerage_performance: Vec<Performance>,
    #[serde(default)]
    pub account_balance_history: Vec<Performance>,
    #[serde(default)]
    pub observed_net_worth_history: Vec<Point>,
    #[serde(default)]
    pub possible_duplicate_accounts: Vec<PossibleDuplicate>,
    /// Plaid's recurring streams, when the user enabled Recurring Transactions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recurring_streams: Vec<RecurringStream>,
    /// Cash dividends per share for held symbols, from a year back through those announced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dividend_history: Vec<DividendRecord>,
    /// Reconstructed net worth against what was observed at each refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_check: Option<HistoryCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<Change>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecurringStream {
    pub stream_id: String,
    pub account_id: String,
    /// `in` or `out`.
    pub direction: String,
    pub description: String,
    pub merchant_name: Option<String>,
    /// Plaid's cadence, such as `MONTHLY` or `BIWEEKLY`.
    pub frequency: String,
    /// `MATURE`, `EARLY_DETECTION`, `TOMBSTONED`, or `UNKNOWN`.
    pub status: String,
    pub is_active: bool,
    pub last_date: Option<String>,
    pub predicted_next_date: Option<String>,
    /// Plaid's sign: positive leaves the account.
    pub average_amount: Option<f64>,
    pub transaction_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DividendRecord {
    pub symbol: String,
    pub ex_date: String,
    pub payable_date: Option<String>,
    /// Per share, in USD.
    pub rate: Option<f64>,
    pub special: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCheck {
    pub compared_days: u32,
    pub divergent_days: u32,
    /// Estimate minus observation on the day they differ most.
    pub largest_gap: Option<f64>,
    pub largest_gap_date: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub institution: String,
    pub r#type: String,
    pub value: Option<f64>,
    #[serde(default)]
    pub cash_value: Option<f64>,
    #[serde(default)]
    pub invested_value: Option<f64>,
    #[serde(default)]
    pub known_cost_basis: Option<f64>,
    #[serde(default)]
    pub known_unrealized_gain: Option<f64>,
    #[serde(default)]
    pub known_unrealized_gain_pct: Option<f64>,
    #[serde(default)]
    pub investment_income_ytd: Option<f64>,
    #[serde(default)]
    pub sale_proceeds_ytd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sales_ytd: Option<u64>,
    #[serde(default)]
    pub estimated_realized_gain_ytd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_gain_coverage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_basis_coverage: Option<String>,
    #[serde(default)]
    pub balance_as_of: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub balance_fetched_at: Option<String>,
    #[serde(default)]
    pub balance_source: Option<String>,
    #[serde(default)]
    pub reported_balance: Option<f64>,
    #[serde(default)]
    pub positions_as_of: Option<String>,
    #[serde(default)]
    pub activity_as_of: Option<String>,
    /// `live` for a real-time Plaid Balance request, `cached` for Plaid's last bank sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liability: Option<CreditLiability>,
    /// Spendable cash after pending activity (deposits) or remaining credit (cards), as the
    /// institution reports it. Absent when the institution doesn't calculate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_balance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_limit: Option<f64>,
    /// The institution's full product name, such as "Platinum Card®".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_name: Option<String>,
    /// The provider's account subtype, such as `checking`, `401k`, or `Roth IRA`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// `personal` or `business`, when the institution says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder_category: Option<String>,
    /// How an investment account's income and gains are taxed: `taxable`, `taxDeferred`, or
    /// `taxFree`. Derived from the subtype; a saved choice overrides it when insights are read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_treatment: Option<String>,
    /// The institution's registered domain from its provider-reported website, for its logo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}
impl Account {
    /// Whether income and gains here are taxed as they happen. Snapshots saved before tax
    /// treatment existed fall back to the account type.
    pub fn taxable(&self) -> bool {
        match self.tax_treatment.as_deref() {
            Some(treatment) => treatment == "taxable",
            None => self.r#type != "retirement",
        }
    }
}

/// Issuer-reported card statement details from Plaid Liabilities.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditLiability {
    pub last_statement_date: Option<String>,
    pub last_statement_balance: Option<f64>,
    pub minimum_payment: Option<f64>,
    pub next_payment_due_date: Option<String>,
    pub last_payment_date: Option<String>,
    pub last_payment_amount: Option<f64>,
    pub purchase_apr: Option<f64>,
    pub is_overdue: Option<bool>,
    /// Every rate on the card with the balance it applies to and the interest it charged on
    /// the last statement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aprs: Vec<CreditApr>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditApr {
    /// Plaid's `apr_type`: `purchase_apr`, `balance_transfer_apr`, `cash_apr`, or `special`.
    pub r#type: String,
    pub percentage: Option<f64>,
    pub balance_subject_to_apr: Option<f64>,
    pub interest_charged: Option<f64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub ticker: String,
    pub name: String,
    pub account_id: String,
    pub shares: Option<f64>,
    pub price: Option<f64>,
    pub value: Option<f64>,
    pub cost_basis: Option<f64>,
    #[serde(default)]
    pub unrealized_gain: Option<f64>,
    pub daily_change_pct: Option<f64>,
    #[serde(default)]
    pub weekly_change_pct: Option<f64>,
    #[serde(default)]
    pub weekly_reference_price: Option<f64>,
    #[serde(default)]
    pub weekly_reference_date: Option<String>,
    pub total_change_pct: Option<f64>,
    #[serde(default)]
    pub market_as_of: Option<String>,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_kind: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_eligible: Option<bool>,
    #[serde(default)]
    pub valuation_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_subtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub industry: Option<String>,
    /// Shares already vested, for equity compensation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vested_shares: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vested_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<SecurityIdentifiers>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<OptionContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_income: Option<FixedIncome>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityIdentifiers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cusip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figi: Option<String>,
    /// The ISO 10383 market identifier code of the listing exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptionContract {
    /// `call` or `put`.
    pub contract_type: Option<String>,
    pub expiration_date: Option<String>,
    pub strike_price: Option<f64>,
    pub underlying_ticker: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FixedIncome {
    pub yield_percent: Option<f64>,
    /// Plaid's yield kind, such as `coupon` or `yield_to_maturity`.
    pub yield_type: Option<String>,
    pub maturity_date: Option<String>,
    pub issue_date: Option<String>,
    pub face_value: Option<f64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    // Absent only in legacy storage; every native view is classified before IPC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<crate::transaction_policy::Classification>,
    pub id: String,
    pub merchant: String,
    pub category: String,
    pub date: String,
    #[serde(default)]
    pub occurred_on: Option<String>,
    #[serde(default)]
    pub posted_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<TransactionLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_channel: Option<String>,
    pub amount: f64,
    pub account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benefit_confirmed: Option<bool>,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_detail: Option<String>,
    /// A concise label from Plaid's detailed category, such as Coffee within Dining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcategory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_code: Option<String>,
    /// The pending transaction this posted one replaced, so edits made while pending carry over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_transaction_id: Option<String>,
    /// View-time fields assigned by `transaction_policy::apply`; committed rows omit them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_source: Option<crate::transaction_policy::CategorySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Parts that divide the amount between categories; they sum to `amount`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<crate::storage::Split>,
    /// The recurring series this transaction belongs to, assigned when insights are read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring_id: Option<String>,
    /// The purchase this refund repays. Its category follows that purchase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refund_of: Option<String>,
    /// Someone else's money: a purchase made for them, or what they paid you back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other: bool,
    /// Plaid's stable merchant identity, shared by every location and descriptor variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_entity_id: Option<String>,
    /// The institution's raw descriptor, before Plaid cleaned it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_number: Option<String>,
    /// The cardholder, on an account with several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_owner: Option<String>,
    /// Who was on the other side: the merchant, a payment app, a marketplace, or a bank.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counterparties: Vec<Counterparty>,
    /// `qualified` or `nonQualified`, for a dividend whose provider says which.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dividend_type: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counterparty {
    pub name: String,
    /// Plaid's counterparty type, such as `merchant`, `payment_app`, or `marketplace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// How sure Plaid is of the match, from `VERY_HIGH` to `LOW`, or `UNKNOWN`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
}

impl Transaction {
    /// The posting date when known, otherwise the transaction date.
    pub fn posted_date(&self) -> &str {
        self.posted_on.as_deref().unwrap_or(&self.date)
    }

    pub fn kind(&self) -> Option<crate::transaction_policy::Kind> {
        self.classification.as_ref().map(|value| value.kind.clone())
    }

    /// Every name the transaction carries: the merchant, Plaid's cleaned description, and the
    /// bank's original descriptor, which keeps issuer wording Plaid's cleanup can drop.
    pub fn descriptors(&self) -> String {
        [
            Some(self.merchant.as_str()),
            self.description.as_deref(),
            self.original_description.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
    }

    /// The category and amount of each part: the saved splits, or the whole transaction.
    pub fn parts(&self) -> Vec<(&str, f64)> {
        if self.splits.is_empty() {
            vec![(self.category.as_str(), self.amount)]
        } else {
            self.splits
                .iter()
                .map(|split| (split.category.as_str(), split.amount))
                .collect()
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionLocation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_number: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: String,
    pub r#type: String,
    pub date: String,
    pub amount: f64,
    pub account: String,
    pub account_id: String,
    pub ticker: Option<String>,
    pub description: Option<String>,
    pub units: Option<f64>,
    pub price: Option<f64>,
    #[serde(default)]
    pub realized_cost_basis: Option<f64>,
    #[serde(default)]
    pub estimated_realized_gain: Option<f64>,
    #[serde(default)]
    pub estimated_realized_gain_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_gain_method: Option<String>,
    /// The parts of the estimated gain from shares held a year or less, and longer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_short_term_gain: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_long_term_gain: Option<f64>,
    /// Units and price were restated for later splits by this ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_ratio: Option<f64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMovement {
    pub id: String,
    pub observed_at: String,
    pub account_id: String,
    pub name: String,
    pub change: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Point {
    pub date: String,
    pub value: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Allocation {
    pub name: String,
    pub value: f64,
    pub percent: f64,
    pub color: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spending {
    pub month_total: f64,
    pub categories: Vec<Allocation>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    pub account_id: String,
    pub name: String,
    pub institution: String,
    pub current_value: f64,
    #[serde(default)]
    pub history_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_source: Option<String>,
    pub points: Vec<PerformancePoint>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformancePoint {
    pub date: String,
    pub value: f64,
    pub net_deposits: Option<f64>,
    pub sp500: Option<f64>,
    #[serde(default)]
    pub market_change: Option<f64>,
    #[serde(default)]
    pub market_change_pct: Option<f64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub observed_at: String,
    pub previous_updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_complete: Option<bool>,
    pub previous_net_worth: f64,
    pub net_worth_change: f64,
    pub account_changes: Vec<AccountChange>,
    pub new_transaction_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountChange {
    pub account_id: String,
    pub name: String,
    pub change: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PossibleDuplicate {
    pub plaid_account_id: String,
    pub snaptrade_account_id: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderSyncStatus {
    pub updated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub provider: String,
    pub adjustment: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub calculation: String,
    pub benchmark: Source,
    pub stock_plan_history: Source,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub message: String,
    pub can_restore: bool,
}

/// Edits a snapshot through its JSON form, as a test sets up saved data.
#[cfg(test)]
pub fn edit_json(snapshot: &mut Snapshot, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut value = serde_json::to_value(&*snapshot).unwrap();
    edit(&mut value);
    *snapshot = serde_json::from_value(value).unwrap();
}

/// A contract type from JSON, as a test writes it.
#[cfg(test)]
pub fn decode<T: serde::de::DeserializeOwned>(
    value: impl Into<serde_json::Value>,
) -> Result<T, String> {
    serde_json::from_value(value.into())
        .map_err(|error| format!("Invalid financial projection: {error}"))
}
