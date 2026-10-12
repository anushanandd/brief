use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration as StdDuration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use futures_util::{stream, SinkExt, StreamExt, TryStreamExt};
use hmac::{Hmac, Mac};
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

pub mod alpaca;
pub mod alpha_vantage;
pub mod closes;
pub mod corporate_actions;
pub mod feeds;
pub mod fundamentals;
pub mod holding_market;
pub mod http;
pub mod insiders;
pub mod market_stream;
pub mod news;
pub mod news_cache;
pub mod plaid;
pub mod portfolio;
pub mod sec;
pub mod snaptrade;
pub mod vault;

pub use alpaca::normalize_stock_symbols;
pub use corporate_actions::CorporateAction;
pub use vault::Integration;

/// One ticker a command may ask about, normalized like every other symbol, or none.
pub fn one_ticker(ticker: Option<String>) -> Result<Option<String>, String> {
    ticker
        .map(|ticker| {
            let mut symbols = normalize_stock_symbols(vec![ticker]);
            if symbols.len() == 1 {
                Ok(symbols.remove(0))
            } else {
                Err("Choose one ticker".to_string())
            }
        })
        .transpose()
}
use http::{ProviderDiagnostic, ProviderError};

const KEYCHAIN_SERVICE: &str = "com.brief.finance.providers";
const CREDENTIAL_VAULT_KEY: &str = "credential-vault-v2";
const PLAID_CREDENTIALS_KEY: &str = "plaid-credentials";
const PLAID_ITEMS_KEY: &str = "plaid-items";
const PLAID_USER_KEY: &str = "plaid-user";
const SNAPTRADE_CREDENTIALS_KEY: &str = "snaptrade-credentials";
const ALPACA_CREDENTIALS_KEY: &str = "alpaca-credentials";
const ALPHA_VANTAGE_CREDENTIALS_KEY: &str = "alpha-vantage-credentials";
const SEC_CONTACT_KEY: &str = "sec-contact";
const PLAID_ITEM_CONCURRENCY: usize = 4;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub plaid: bool,
    pub snaptrade: bool,
    pub alpaca: bool,
    pub alpha_vantage: bool,
    pub sec: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSnapshot {
    pub symbol: String,
    pub price: f64,
    pub previous_close: f64,
    pub previous_close_as_of: Option<String>,
    pub daily_change_pct: f64,
    pub weekly_change_pct: Option<f64>,
    pub weekly_reference_price: Option<f64>,
    pub weekly_reference_date: Option<String>,
    pub as_of: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSnapshots {
    pub snapshots: BTreeMap<String, MarketSnapshot>,
    pub session: String,
    pub feed: String,
    pub delay_minutes: u8,
    pub as_of: Option<String>,
    pub next_transition_at: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub history_feed: String,
    pub history_delay_minutes: u8,
}

#[derive(Clone, Debug)]
pub struct MarketFrame {
    pub as_of: String,
    pub snapshots: BTreeMap<String, MarketSnapshot>,
}

#[derive(Clone, Debug)]
pub struct MarketStreamTick {
    pub market: MarketSnapshots,
    pub chart_as_of: Option<String>,
    pub chart_snapshots: Option<BTreeMap<String, MarketSnapshot>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketNewsArticle {
    pub headline: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub created_at: String,
    pub symbols: Vec<String>,
    /// Computed from the headline and tickers each time stories are read, so rule changes
    /// apply to saved stories too.
    #[serde(default)]
    pub scope: NewsScope,
    #[serde(default)]
    pub tags: Vec<NewsTag>,
    /// How much the story is about the requested company, 0 to 100; computed with `scope`.
    #[serde(default)]
    pub relevance: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsScope {
    /// About the requested company.
    #[default]
    Company,
    /// A roundup or market recap that names many tickers.
    Market,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsTag {
    Earnings,
    Guidance,
    Product,
    Deal,
    Legal,
    Leadership,
    Capital,
    /// Explains the day's move ("Why Is … Stock Soaring?"); not an event itself.
    Explainer,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EarningsEvent {
    pub symbol: String,
    pub name: String,
    pub report_date: String,
    pub fiscal_date_ending: Option<String>,
    pub estimate: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSession {
    pub provider: String,
    pub session_id: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkStatus {
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnection {
    item_id: String,
    name: String,
    provider: String,
    error: Option<String>,
}

impl From<&SnapTradeConnection> for ProviderConnection {
    fn from(connection: &SnapTradeConnection) -> Self {
        Self {
            item_id: connection.id.clone(),
            name: connection.name.clone(),
            provider: "snaptrade".into(),
            error: connection.disabled.then(|| "Needs repair".into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaidRefreshProgress {
    pub id: String,
    pub label: String,
    pub status: String,
    pub state: &'static str,
}

#[derive(Clone, Debug)]
pub struct ProviderSync {
    pub sync_id: String,
    pub balances_fresh: bool,
    pub refreshed_account_ids: Vec<String>,
    pub previous_snapshot: crate::finance_contract::Snapshot,
    pub plaid: PlaidData,
    pub snaptrade: SnapTradeData,
    pub benchmark_history: Value,
    pub security_history: BTreeMap<String, Vec<Value>>,
    /// Saved statement files; their rows fill history before each provider's first activity.
    pub statements: Vec<crate::statements::StatementFile>,
    /// Splits and cash dividends for held and traded symbols.
    pub corporate_actions: Vec<CorporateAction>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PlaidData {
    pub accounts: Vec<Value>,
    pub transactions: Vec<Value>,
    #[serde(default, rename = "transactionHistoryStart")]
    pub transaction_history_start: BTreeMap<String, String>,
    #[serde(rename = "investmentAccounts")]
    pub investment_accounts: Vec<Value>,
    pub holdings: Vec<Value>,
    pub securities: Vec<Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, rename = "refreshedAccountIds")]
    pub refreshed_account_ids: Vec<String>,
    /// Plaid Liabilities credit records, present only when the user enabled Liabilities.
    #[serde(default)]
    pub liabilities: Vec<Value>,
    /// Plaid Investments Transactions records (trades, dividends, fees, contributions),
    /// present only when the user enabled that billed product.
    #[serde(default, rename = "investmentTransactions")]
    pub investment_transactions: Vec<Value>,
    /// Investment accounts whose full available history of investment transactions was read.
    #[serde(default, rename = "investmentHistoryAccounts")]
    pub investment_history_accounts: Vec<String>,
    /// Plaid Recurring Transactions streams, each tagged with its `direction`, present only
    /// when the user enabled that billed product.
    #[serde(default, rename = "recurringStreams")]
    pub recurring_streams: Vec<Value>,
    #[serde(skip)]
    pub diagnostics: Vec<ProviderDiagnostic>,
}

/// Per-refresh Plaid choices. Live balances and Liabilities are billed per call or per
/// Item, so both stay explicit rather than implied by an ordinary refresh.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlaidSyncOptions {
    pub live_balances: bool,
    pub liabilities: bool,
    /// Plaid Investments Transactions: a separate subscription per investment Item.
    pub investment_transactions: bool,
    /// Plaid Recurring Transactions: a separate subscription per transactions Item.
    pub recurring: bool,
}

/// Transactions are requested in Plaid's v2 personal finance category taxonomy.
pub(crate) const PLAID_CATEGORY_TAXONOMY: &str = "v2";
/// The shape of cached Plaid transactions: the category taxonomy plus the request options
/// that add fields. A cache saved in another shape is rebuilt once from a fresh cursor.
pub(crate) const PLAID_TRANSACTION_FORMAT: &str = "v2+original-description";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SnapTradeData {
    pub accounts: Vec<Value>,
    pub positions: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    pub cash_balances: BTreeMap<String, Vec<Value>>,
    #[serde(default, rename = "positionsAsOf")]
    pub positions_as_of: BTreeMap<String, Option<String>>,
    pub activities: BTreeMap<String, Vec<Value>>,
    #[serde(rename = "balanceHistory")]
    pub balance_history: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, rename = "historyComplete")]
    pub history_complete: bool,
    #[serde(default, rename = "activityComplete")]
    pub activity_complete: bool,
    #[serde(default)]
    pub connections: Vec<SnapTradeConnection>,
}

/// A brokerage login. While disabled, SnapTrade returns its last saved data until the user
/// repairs it, which keeps the connection and account IDs.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SnapTradeConnection {
    pub id: String,
    pub name: String,
    pub disabled: bool,
}

pub use crate::finance_contract::ProviderSyncStatus;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderDataCache {
    pub plaid: Option<PlaidData>,
    pub snaptrade: Option<SnapTradeData>,
    #[serde(rename = "securityHistory")]
    pub security_history: BTreeMap<String, Vec<Value>>,
    pub security_history_adjustment: Option<String>,
    pub snaptrade_history_refreshed_at: Option<String>,
    pub snaptrade_activity_backfilled_at: Option<String>,
    pub market_history_refreshed_at: Option<String>,
    pub sync_status: BTreeMap<String, ProviderSyncStatus>,
    /// Splits and cash dividends for held and traded symbols, from Alpaca.
    #[serde(deserialize_with = "corporate_actions::lenient")]
    pub corporate_actions: Vec<CorporateAction>,
    pub corporate_actions_refreshed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidCredentials {
    client_id: String,
    secret: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapTradeCredentials {
    client_id: String,
    consumer_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlpacaCredentials {
    key_id: String,
    secret_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AlphaVantageCredentials {
    api_key: String,
}

/// The contact the SEC requires in every EDGAR request's User-Agent. Not a secret, but kept
/// with the other provider settings so it never reaches renderer storage.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecContact {
    email: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AlpacaMarketSession {
    Core,
    PreMarket,
    AfterHours,
    Overnight,
    Closed,
}

impl AlpacaMarketSession {
    fn feed(self) -> &'static str {
        match self {
            Self::Core | Self::Closed => "iex",
            Self::PreMarket | Self::AfterHours => "delayed_sip",
            Self::Overnight => "overnight",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Core => "Regular market",
            Self::PreMarket => "Pre-market",
            Self::AfterHours => "After hours",
            Self::Overnight => "Overnight",
            Self::Closed => "Market closed",
        }
    }

    fn history_feed(self) -> &'static str {
        match self {
            Self::PreMarket | Self::AfterHours => "sip",
            Self::Overnight => "boats",
            _ => self.feed(),
        }
    }

    fn history_delay_minutes(self) -> u8 {
        match self {
            Self::PreMarket | Self::AfterHours | Self::Overnight => 15,
            _ => 0,
        }
    }

    fn poll_interval_ms(self) -> Option<u64> {
        match self {
            Self::Core | Self::PreMarket | Self::AfterHours | Self::Overnight => Some(300_000),
            Self::Closed => None,
        }
    }
}

#[derive(Clone, Debug)]
struct AlpacaMarketSchedule {
    session: AlpacaMarketSession,
    next_transition_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
struct WeeklyReference {
    price: f64,
    date: String,
}

#[derive(Default)]
struct WeeklyCloseCache {
    reference_date: String,
    references: BTreeMap<String, WeeklyReference>,
    attempted_symbols: BTreeSet<String>,
    attempted_at: Option<Instant>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidItem {
    item_id: String,
    access_token: String,
    institution_id: Option<String>,
    institution_name: Option<String>,
    #[serde(default)]
    investments: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidUser {
    client_id: String,
    user_id: Option<String>,
    user_token: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CredentialVault {
    entries: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidCache {
    items: BTreeMap<String, PlaidItemCache>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaidItemCache {
    /// The institution's website from Plaid, looked up once per connection, for its logo.
    #[serde(default)]
    institution_url: Option<String>,
    #[serde(default)]
    institution_url_checked: bool,
    cursor: Option<String>,
    transactions: Vec<Value>,
    #[serde(default)]
    transaction_history_start: Option<String>,
    /// Plaid reported `HISTORICAL_UPDATE_COMPLETE` on the latest transactions sync.
    #[serde(default)]
    history_complete: bool,
    #[serde(default)]
    data: Option<PlaidData>,
    #[serde(default)]
    error: Option<String>,
    /// `PLAID_TRANSACTION_FORMAT` when `transactions` were fetched; a different value forces
    /// one clean resync. Older builds saved the bare taxonomy here.
    #[serde(default)]
    taxonomy: Option<String>,
}

impl PlaidCache {
    pub(crate) fn clear_legacy_diagnostics(&mut self) {
        for item in self.items.values_mut() {
            item.error = None;
            if let Some(data) = &mut item.data {
                data.warnings.clear();
            }
        }
    }
}

impl ProviderDataCache {
    pub(crate) fn clear_legacy_diagnostics(&mut self) {
        for status in self.sync_status.values_mut() {
            status.error = None;
        }
        if let Some(data) = &mut self.plaid {
            data.warnings.clear();
        }
        if let Some(data) = &mut self.snaptrade {
            data.warnings.clear();
        }
    }
}

#[derive(Clone, Debug)]
enum PendingLink {
    Plaid {
        link_token: String,
        exchanged_public_tokens: Vec<String>,
        investments: bool,
        repairing: bool,
    },
    SnapTrade {
        existing_connection_ids: BTreeSet<String>,
    },
}

#[derive(Clone)]
struct PendingSession {
    link: PendingLink,
    generation: u64,
    expires: Instant,
    polling: bool,
    cancel: tokio::sync::watch::Sender<bool>,
}

pub struct Providers {
    pub market_streams: market_stream::MarketStreams,
    chart_previews: Mutex<BTreeMap<(String, u64), (u64, holding_market::PriceHistory)>>,
    chart_requests: tokio::sync::Semaphore,
    http: reqwest::Client,
    links: Mutex<BTreeMap<String, PendingSession>>,
    vault: Mutex<()>,
    credential_generation: AtomicU64,
    credential_edit_authorized_until: Mutex<Option<Instant>>,
    market_schedule: Mutex<Option<AlpacaMarketSchedule>>,
    market_access: Mutex<Option<holding_market::MarketAccess>>,
    weekly_closes: Mutex<WeeklyCloseCache>,
    /// The disposable news database, opened on first use; see `news_cache`.
    news: tokio::sync::Mutex<Option<news_cache::NewsCache>>,
    news_cache_path: Option<std::path::PathBuf>,
    /// The latest summary runs, for the Logs page and Data health.
    pub summary_runs: crate::summary::SummaryRuns,
}

impl Providers {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent("Brief/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            http,
            market_streams: market_stream::MarketStreams::default(),
            chart_previews: Mutex::new(BTreeMap::new()),
            chart_requests: tokio::sync::Semaphore::new(2),
            links: Mutex::new(BTreeMap::new()),
            vault: Mutex::new(()),
            credential_generation: AtomicU64::new(0),
            credential_edit_authorized_until: Mutex::new(None),
            market_schedule: Mutex::new(None),
            market_access: Mutex::new(None),
            weekly_closes: Mutex::new(WeeklyCloseCache::default()),
            news: tokio::sync::Mutex::new(None),
            news_cache_path: None,
            summary_runs: crate::summary::SummaryRuns::default(),
        })
    }

    /// Keeps stories in `path`; without one, tests keep them in memory.
    pub fn with_news_cache(mut self, path: &std::path::Path) -> Self {
        self.news_cache_path = Some(path.to_owned());
        self
    }

    pub async fn clear_disposable_data(&self, directory: &std::path::Path) -> Result<(), String> {
        if self.news_cache_path.is_some() {
            self.news_cache().await?.clear()?;
        }
        holding_market::clear_cache(&directory.join("market-prices.sqlite3"))?;
        self.chart_previews
            .lock()
            .map_err(|_| "Market preview state is unavailable")?
            .clear();
        *self
            .market_schedule
            .lock()
            .map_err(|_| "Market schedule state is unavailable")? = None;
        *self
            .market_access
            .lock()
            .map_err(|_| "Market access state is unavailable")? = None;
        Ok(())
    }
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Sends a request, then once more after a short pause when the first attempt couldn't connect
/// or the provider asked for a retry. `retry` is the request's clone, absent when it can't be
/// sent again. A failure to connect comes with how many attempts were made.
async fn send_with_retry(
    request: reqwest::RequestBuilder,
    retry: Option<reqwest::RequestBuilder>,
) -> Result<reqwest::Response, (reqwest::Error, usize)> {
    let first = request.send().await;
    let Some(retry) = retry else {
        return first.map_err(|error| (error, 1));
    };
    match &first {
        Ok(response) if retryable_status(response.status()) => retry_pause(response).await,
        Ok(_) => return first.map_err(|error| (error, 1)),
        Err(_) => tokio::time::sleep(StdDuration::from_millis(250)).await,
    }
    retry.send().await.map_err(|error| (error, 2))
}

async fn retry_pause(response: &reqwest::Response) {
    let seconds = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1)
        .min(2);
    tokio::time::sleep(StdDuration::from_secs(seconds)).await;
}

fn history_or_cached(
    result: Result<Vec<Value>, String>,
    cached: Option<&Vec<Value>>,
    context: &str,
    warnings: &mut Vec<String>,
) -> Vec<Value> {
    match result {
        Ok(history) => history,
        Err(error) => {
            warnings.push(format!("{context}: {error}"));
            cached.cloned().unwrap_or_default()
        }
    }
}

async fn provider_response(
    provider: &str,
    endpoint: &str,
    response: reqwest::Response,
) -> Result<Value, String> {
    ProviderError::from_response(provider, endpoint, response, 1)
        .await
        .map_err(|error| {
            http::observe(&error);
            error.to_string()
        })
}

fn required(value: String, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value.into())
    }
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Provider response did not include {field}"))
}

fn bool_field(value: &Value, field: &str) -> Result<bool, String> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("Provider response did not include {field}"))
}

fn array(value: &Value, field: &str) -> Vec<Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn required_array(value: &Value, field: &str) -> Result<Vec<Value>, String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("Provider response did not include {field}"))
}

fn required_arrays(value: &Value, fields: &[&str]) -> Result<Vec<Value>, String> {
    let mut values = Vec::new();
    for field in fields {
        values.extend(required_array(value, field)?);
    }
    Ok(values)
}

fn validate_link_session(session: &PendingSession, generation: u64) -> Result<(), String> {
    if session.expires <= Instant::now() || *session.cancel.borrow() {
        return Err("Connection attempt expired or cancelled".into());
    }
    if session.generation != generation {
        return Err("Credentials changed; start a new connection attempt".into());
    }
    Ok(())
}

async fn cancellable_link_poll(
    mut cancelled: tokio::sync::watch::Receiver<bool>,
    expires: Instant,
    poll: impl std::future::Future<Output = Result<LinkStatus, String>>,
) -> Result<LinkStatus, String> {
    if *cancelled.borrow() {
        return Err("Connection attempt cancelled".into());
    }
    tokio::select! {
        biased;
        _ = cancelled.changed() => Err("Connection attempt cancelled".into()),
        _ = tokio::time::sleep_until(expires.into()) => Err("Connection attempt expired".into()),
        result = poll => result,
    }
}

impl LinkStatus {
    fn of(connected: bool) -> Self {
        Self {
            status: if connected { "connected" } else { "pending" }.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_drops_an_outstanding_exchange_future() {
        struct Dropped(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = dropped.clone();
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let (started, waiting) = tokio::sync::oneshot::channel();
        let poll = async move {
            let _guard = Dropped(flag);
            let _ = started.send(());
            std::future::pending::<Result<LinkStatus, String>>().await
        };
        let cancel_task = async {
            waiting.await.unwrap();
            cancel.send(true).unwrap();
        };
        let (result, ()) = tokio::join!(
            cancellable_link_poll(receiver, Instant::now() + StdDuration::from_secs(300), poll),
            cancel_task
        );
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancellation_before_a_poll_subscribes_is_not_lost() {
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        drop(receiver);
        cancel.send_replace(true);
        let result = cancellable_link_poll(
            cancel.subscribe(),
            Instant::now() + StdDuration::from_secs(300),
            async { panic!("A cancelled exchange must not start") },
        )
        .await;
        assert!(result.unwrap_err().contains("cancelled"));
    }

    #[tokio::test]
    async fn a_request_the_provider_asks_to_retry_is_sent_once_more() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for reply in [
                "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = stream.read(&mut [0; 1024]);
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        let request = reqwest::Client::new().get(format!("http://{address}/page"));
        let retry = request.try_clone();
        let response = send_with_retry(request, retry).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        server.join().unwrap();
    }

    #[test]
    fn provider_cache_reads_legacy_data_and_round_trips_sync_metadata() {
        let mut cache: ProviderDataCache = serde_json::from_value(json!({
            "snaptrade": {
                "accounts": [], "positions": {}, "activities": {}, "balanceHistory": {}
            },
            "securityHistory": {}
        }))
        .unwrap();
        assert!(cache.snaptrade.as_ref().unwrap().warnings.is_empty());
        assert!(cache.sync_status.is_empty());
        assert!(cache.snaptrade_history_refreshed_at.is_none());
        assert!(cache.snaptrade_activity_backfilled_at.is_none());
        assert!(cache.security_history_adjustment.is_none());
        cache.snaptrade_activity_backfilled_at = Some("2026-09-05T11:00:00Z".into());
        cache.snaptrade_history_refreshed_at = Some("2026-09-05T12:00:00Z".into());
        cache.security_history_adjustment = Some("split".into());
        cache.sync_status.insert(
            "snaptrade".into(),
            ProviderSyncStatus {
                updated_at: cache.snaptrade_history_refreshed_at.clone(),
                error: None,
            },
        );
        let value = serde_json::to_value(&cache).unwrap();
        assert_eq!(value["snaptradeHistoryRefreshedAt"], "2026-09-05T12:00:00Z");
        assert_eq!(value["securityHistoryAdjustment"], "split");
        assert_eq!(
            value["snaptradeActivityBackfilledAt"],
            "2026-09-05T11:00:00Z"
        );
        assert_eq!(
            value["syncStatus"]["snaptrade"]["updatedAt"],
            "2026-09-05T12:00:00Z"
        );
        assert!(value["syncStatus"]["snaptrade"]["error"].is_null());
        let restored: ProviderDataCache = serde_json::from_value(value).unwrap();
        assert_eq!(
            restored.snaptrade_history_refreshed_at,
            cache.snaptrade_history_refreshed_at
        );
        assert_eq!(
            restored.security_history_adjustment.as_deref(),
            Some("split")
        );
    }

    #[test]
    fn failed_history_requests_preserve_cache_and_report_warnings() {
        let cached = vec![json!({"date": "2026-09-04", "value": 100})];
        let mut warnings = Vec::new();
        assert_eq!(
            history_or_cached(
                Err("offline".into()),
                Some(&cached),
                "History",
                &mut warnings
            ),
            cached,
        );
        assert_eq!(warnings, vec!["History: offline"]);
        warnings.clear();
        assert!(history_or_cached(Ok(vec![]), Some(&cached), "History", &mut warnings).is_empty());
        assert!(warnings.is_empty());
        assert!(
            history_or_cached(Err("offline".into()), None, "History", &mut warnings).is_empty()
        );
        assert_eq!(warnings.len(), 1);
    }

    #[tokio::test]
    async fn saved_news_reads_need_no_credentials_or_request() {
        let path = std::env::temp_dir().join(format!(
            "brief-provider-news-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let now = Utc::now().timestamp();
        let mut cache = news_cache::NewsCache::new(Some(&path)).unwrap();
        cache.begin_refresh("TEST", now).unwrap();
        cache
            .save(
                "TEST",
                &[MarketNewsArticle {
                    headline: "Synthetic news".into(),
                    summary: "Synthetic evidence".into(),
                    source: "Example".into(),
                    url: "https://example.com/story".into(),
                    created_at: "2026-09-21T12:00:00+00:00".into(),
                    symbols: vec!["TEST".into()],
                    scope: NewsScope::Company,
                    tags: vec![],
                    relevance: 0,
                }],
                now,
            )
            .unwrap();
        drop(cache);

        let providers = Providers::new().unwrap().with_news_cache(&path);
        let result = providers
            .market_news(vec!["TEST".into()], false)
            .await
            .unwrap();
        assert_eq!(result.articles.len(), 1);
        assert!(!result.can_refresh);
        drop(providers);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_plaid_items_default_to_transaction_sync() {
        let item: PlaidItem = serde_json::from_value(json!({
            "itemId": "item",
            "accessToken": "token",
            "institutionId": null,
            "institutionName": "Bank"
        }))
        .unwrap();

        assert!(!item.investments);
    }
}
