//! Shared native output contract. Provider compatibility JSON is converted at
//! the projection boundary; storage validates these same types.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Narrow, typed live view. Deserializing from a committed snapshot skips its ledger/history.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<Change>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_code: Option<String>,
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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
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

// Transitional conversion for existing decimal projection helpers. All new
// output fields belong in the typed contract, not in an unvalidated JSON map.
pub fn decode<T: serde::de::DeserializeOwned>(
    value: impl Into<serde_json::Value>,
) -> Result<T, String> {
    serde_json::from_value(value.into())
        .map_err(|error| format!("Invalid financial projection: {error}"))
}
