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
pub mod holding_market;
pub mod http;
pub mod market_stream;
pub mod news_cache;
pub mod plaid;
pub mod snaptrade;
pub mod vault;

use http::{ProviderDiagnostic, ProviderError};

const KEYCHAIN_SERVICE: &str = "com.brief.finance.providers";
const CREDENTIAL_VAULT_KEY: &str = "credential-vault-v2";
const PLAID_CREDENTIALS_KEY: &str = "plaid-credentials";
const PLAID_ITEMS_KEY: &str = "plaid-items";
const PLAID_USER_KEY: &str = "plaid-user";
const SNAPTRADE_CREDENTIALS_KEY: &str = "snaptrade-credentials";
const ALPACA_CREDENTIALS_KEY: &str = "alpaca-credentials";
const ALPHA_VANTAGE_CREDENTIALS_KEY: &str = "alpha-vantage-credentials";
const PLAID_ITEM_CONCURRENCY: usize = 4;

fn report_plaid_progress(
    progress: &(dyn Fn(PlaidRefreshProgress) + Sync),
    item_index: usize,
    item_count: usize,
    task: &'static str,
    status: String,
    state: &'static str,
) {
    let connection = if item_count == 1 {
        "Plaid".into()
    } else {
        format!("Plaid {}", item_index + 1)
    };
    progress(PlaidRefreshProgress {
        id: format!("plaid-{item_index}-{}", task.to_lowercase()),
        label: format!("{connection} {task}"),
        status,
        state,
    });
}

fn measured_status(started: Instant, detail: impl AsRef<str>) -> String {
    format!(
        "{} · {:.1}s",
        detail.as_ref(),
        started.elapsed().as_secs_f64()
    )
}

fn completion_status(retries: usize) -> String {
    match retries {
        0 => "Complete".into(),
        1 => "Complete · 1 retry".into(),
        retries => format!("Complete · {retries} retries"),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub plaid: bool,
    pub snaptrade: bool,
    pub alpaca: bool,
    pub alpha_vantage: bool,
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
    pub relevance_score: Option<f64>,
    pub sentiment_score: Option<f64>,
    pub sentiment_label: Option<String>,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidRecurringAmount {
    amount: f64,
    currency: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidRecurringStream {
    stream_id: String,
    account_id: String,
    direction: &'static str,
    description: String,
    merchant_name: Option<String>,
    category: Option<String>,
    frequency: String,
    status: String,
    is_active: bool,
    first_date: String,
    last_date: String,
    predicted_next_date: Option<String>,
    average_amount: PlaidRecurringAmount,
    last_amount: PlaidRecurringAmount,
    transaction_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidRecurringConnection {
    item_id: String,
    name: String,
    streams: Vec<PlaidRecurringStream>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaidRecurringReport {
    fetched_at: String,
    connections: Vec<PlaidRecurringConnection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnection {
    item_id: String,
    name: String,
    provider: String,
    error: Option<String>,
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
    pub previous_snapshot: Value,
    pub plaid: PlaidData,
    pub snaptrade: SnapTradeData,
    pub benchmark_history: Value,
    pub security_history: BTreeMap<String, Vec<Value>>,
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
    #[serde(skip)]
    pub diagnostics: Vec<ProviderDiagnostic>,
}

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
    cursor: Option<String>,
    transactions: Vec<Value>,
    #[serde(default)]
    transaction_history_start: Option<String>,
    #[serde(default)]
    data: Option<PlaidData>,
    #[serde(default)]
    error: Option<String>,
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
    market_access: Mutex<Option<(u64, Instant, bool, bool)>>,
    weekly_closes: Mutex<WeeklyCloseCache>,
    news: tokio::sync::Mutex<()>,
    news_cache_path: Option<std::path::PathBuf>,
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
            news: tokio::sync::Mutex::new(()),
            news_cache_path: None,
        })
    }

    pub fn with_news_cache(mut self, path: &std::path::Path) -> Result<Self, String> {
        self.news_cache_path = Some(path.to_owned());
        Ok(self)
    }
}

async fn consume_market_stream<F>(
    feed: &market_stream::Feed,
    symbols: &[String],
    transition: DateTime<Utc>,
    market: &mut MarketSnapshots,
    chart_seed: &BTreeMap<String, MarketSnapshot>,
    bar_frames: &mut BTreeMap<String, BTreeMap<String, MarketSnapshot>>,
    update_interval: StdDuration,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
    publish: &mut F,
) -> Result<(), String>
where
    F: FnMut(MarketStreamTick) -> Result<(), String> + Send,
{
    let mut messages = feed.events.subscribe();
    let initial = feed
        .state
        .lock()
        .map_err(|_| "Market state unavailable")?
        .clone();
    if initial.finished {
        return Err(initial
            .error
            .unwrap_or_else(|| "Market disconnected".into()));
    }
    for (symbol, quote) in initial.quotes {
        let price = if market.feed == "overnight" {
            quote_midpoint(&quote)
        } else {
            quote["p"].as_f64()
        };
        if let (Some(price), Some(time)) = (price, quote["t"].as_str()) {
            update_stream_price(&mut market.snapshots, &symbol, price, time);
        }
    }
    let wait = (transition - Utc::now())
        .to_std()
        .unwrap_or_else(|_| StdDuration::from_secs(0));
    let transition_sleep = tokio::time::sleep(wait);
    tokio::pin!(transition_sleep);
    let mut flush = tokio::time::interval(update_interval.max(StdDuration::from_secs(1)));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    flush.tick().await;
    let mut market_changed = true;
    let mut chart_times = BTreeSet::new();

    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return Ok(());
                }
            }
            _ = &mut transition_sleep => {
                publish_pending_market_updates(
                    market,
                    chart_seed,
                    bar_frames,
                    &mut market_changed,
                    &mut chart_times,
                    publish,
                )?;
                return Ok(());
            }
            _ = flush.tick() => {
                publish_pending_market_updates(
                    market,
                    chart_seed,
                    bar_frames,
                    &mut market_changed,
                    &mut chart_times,
                    publish,
                )?;
            }
            message = messages.recv() => {
                let items = message.map_err(|_| "Market consumer needs reconciliation")?;
                        for item in items {
                            if item.get("T").and_then(Value::as_str) == Some("error") {
                                return Err(stream_error(&item));
                            }
                            let Some(symbol) = item
                                .get("S")
                                .and_then(Value::as_str)
                                .filter(|symbol| symbols.iter().any(|candidate| candidate == symbol))
                            else {
                                continue;
                            };
                            if matches!(item["T"].as_str(), Some("c" | "x")) {
                                return Err("Market correction requires reconciliation".into());
                            }
                            let Some(as_of) = item.get("t").and_then(Value::as_str) else {
                                continue;
                            };
                            match item.get("T").and_then(Value::as_str) {
                                Some("q") if market.feed == "overnight" => {
                                    if let Some(price) = quote_midpoint(&item) {
                                        market_changed |= update_stream_price(
                                            &mut market.snapshots,
                                            symbol,
                                            price,
                                            as_of,
                                        );
                                    }
                                }
                                Some("t") if market.feed != "overnight" => {
                                    if let Some(price) = item.get("p").and_then(Value::as_f64) {
                                        market_changed |= update_stream_price(
                                            &mut market.snapshots,
                                            symbol,
                                            price,
                                            as_of,
                                        );
                                    }
                                }
                                Some("b" | "u") => {
                                    if let Some(price) = item.get("c").and_then(Value::as_f64) {
                                        let base = market
                                            .snapshots
                                            .get(symbol)
                                            .or_else(|| chart_seed.get(symbol));
                                        if let Some(snapshot) = stream_snapshot(base, symbol, price, as_of) {
                                            bar_frames
                                                .entry(as_of.to_string())
                                                .or_default()
                                                .insert(symbol.to_string(), snapshot);
                                            chart_times.insert(as_of.to_string());
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        market.as_of = newest_market_time(&market.snapshots);
            }
        }
    }
}

fn publish_market_backfill<F>(
    market: &MarketSnapshots,
    backfill: Vec<MarketFrame>,
    chart_seed: &BTreeMap<String, MarketSnapshot>,
    bar_frames: &mut BTreeMap<String, BTreeMap<String, MarketSnapshot>>,
    publish: &mut F,
) -> Result<(), String>
where
    F: FnMut(MarketStreamTick) -> Result<(), String>,
{
    for frame in backfill {
        let changed = bar_frames.get(&frame.as_of) != Some(&frame.snapshots);
        bar_frames.insert(frame.as_of.clone(), frame.snapshots);
        if changed {
            publish(MarketStreamTick {
                market: market.clone(),
                chart_snapshots: Some(chart_snapshots_at(&frame.as_of, chart_seed, bar_frames)),
                chart_as_of: Some(frame.as_of),
            })?;
        }
    }
    Ok(())
}

fn publish_pending_market_updates<F>(
    market: &MarketSnapshots,
    chart_seed: &BTreeMap<String, MarketSnapshot>,
    bar_frames: &BTreeMap<String, BTreeMap<String, MarketSnapshot>>,
    market_changed: &mut bool,
    chart_times: &mut BTreeSet<String>,
    publish: &mut F,
) -> Result<(), String>
where
    F: FnMut(MarketStreamTick) -> Result<(), String>,
{
    if !*market_changed && chart_times.is_empty() {
        return Ok(());
    }
    if chart_times.is_empty() {
        publish(MarketStreamTick {
            market: market.clone(),
            chart_as_of: None,
            chart_snapshots: None,
        })?;
    } else {
        for as_of in std::mem::take(chart_times) {
            publish(MarketStreamTick {
                market: market.clone(),
                chart_snapshots: Some(chart_snapshots_at(&as_of, chart_seed, bar_frames)),
                chart_as_of: Some(as_of),
            })?;
        }
    }
    *market_changed = false;
    Ok(())
}

fn parse_stream_payload(text: &str) -> Result<Vec<Value>, String> {
    let payload: Value = serde_json::from_str(text)
        .map_err(|error| format!("Alpaca WebSocket sent invalid data: {error}"))?;
    match payload {
        Value::Array(items) => Ok(items),
        item @ Value::Object(_) => Ok(vec![item]),
        _ => Err("Alpaca WebSocket sent an unexpected message".into()),
    }
}

fn stream_error(value: &Value) -> String {
    let message = value
        .get("msg")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    let code = value
        .get("code")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    format!("Alpaca WebSocket error {code}: {message}")
}

fn update_stream_price(
    snapshots: &mut BTreeMap<String, MarketSnapshot>,
    symbol: &str,
    price: f64,
    as_of: &str,
) -> bool {
    if !price.is_finite() || price <= 0.0 {
        return false;
    }
    if snapshots.get(symbol).is_some_and(|current| {
        let current_time = DateTime::parse_from_rfc3339(&current.as_of).ok();
        let next_time = DateTime::parse_from_rfc3339(as_of).ok();
        current_time
            .zip(next_time)
            .is_some_and(|(current, next)| next < current)
    }) {
        return false;
    }
    let Some(snapshot) = stream_snapshot(snapshots.get(symbol), symbol, price, as_of) else {
        return false;
    };
    snapshots.insert(symbol.to_string(), snapshot);
    true
}

fn stream_snapshot(
    base: Option<&MarketSnapshot>,
    symbol: &str,
    price: f64,
    as_of: &str,
) -> Option<MarketSnapshot> {
    if !price.is_finite() || price <= 0.0 || DateTime::parse_from_rfc3339(as_of).is_err() {
        return None;
    }
    let previous_close = base
        .map(|snapshot| snapshot.previous_close)
        .unwrap_or(price);
    let weekly_reference_price = base.and_then(|snapshot| snapshot.weekly_reference_price);
    Some(MarketSnapshot {
        symbol: symbol.to_string(),
        price,
        previous_close,
        previous_close_as_of: base.and_then(|snapshot| snapshot.previous_close_as_of.clone()),
        daily_change_pct: if previous_close == 0.0 {
            0.0
        } else {
            (price - previous_close) / previous_close * 100.0
        },
        weekly_change_pct: weekly_reference_price
            .map(|reference| (price - reference) / reference * 100.0),
        weekly_reference_price,
        weekly_reference_date: base.and_then(|snapshot| snapshot.weekly_reference_date.clone()),
        as_of: as_of.to_string(),
    })
}

fn newest_market_time(snapshots: &BTreeMap<String, MarketSnapshot>) -> Option<String> {
    snapshots
        .values()
        .filter_map(|snapshot| {
            DateTime::parse_from_rfc3339(&snapshot.as_of)
                .ok()
                .map(|time| (time, snapshot.as_of.clone()))
        })
        .max_by_key(|(time, _)| *time)
        .map(|(_, value)| value)
}

fn previous_close_snapshots(
    snapshots: &BTreeMap<String, MarketSnapshot>,
    history: &[MarketFrame],
) -> BTreeMap<String, MarketSnapshot> {
    let as_of = history
        .first()
        .map(|frame| frame.as_of.clone())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    snapshots
        .iter()
        .filter_map(|(symbol, snapshot)| {
            stream_snapshot(Some(snapshot), symbol, snapshot.previous_close, &as_of)
                .map(|snapshot| (symbol.clone(), snapshot))
        })
        .collect()
}

fn chart_snapshots_at(
    as_of: &str,
    seed: &BTreeMap<String, MarketSnapshot>,
    frames: &BTreeMap<String, BTreeMap<String, MarketSnapshot>>,
) -> BTreeMap<String, MarketSnapshot> {
    let Some(target) = DateTime::parse_from_rfc3339(as_of).ok() else {
        return seed.clone();
    };
    let mut snapshots = seed.clone();
    for (frame_time, frame) in frames {
        let Some(frame_time) = DateTime::parse_from_rfc3339(frame_time).ok() else {
            continue;
        };
        if frame_time > target {
            continue;
        }
        snapshots.extend(frame.clone());
    }
    snapshots
}

fn validate_plaid_page(
    cursor: &str,
    has_more: bool,
    page: usize,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    if has_more && (page >= 99 || !seen.insert(cursor.into())) {
        return Err(
            "Plaid transaction pagination made no progress or exceeded its safe page limit".into(),
        );
    }
    Ok(())
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
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

fn parse_alpha_vantage_news(
    payload: &Value,
    symbol: &str,
) -> Result<Vec<MarketNewsArticle>, String> {
    if payload.get("Note").is_some() || payload.get("Information").is_some() {
        return Err("Alpha Vantage news is unavailable: check API quota or news access in Settings, then retry later".into());
    }
    if payload.get("Error Message").is_some() {
        return Err("Alpha Vantage rejected the news request".into());
    }
    let feed = payload
        .get("feed")
        .and_then(Value::as_array)
        .ok_or("Alpha Vantage response omitted news")?;
    let mut seen = BTreeSet::new();
    Ok(feed
        .iter()
        .filter_map(|article| {
            let sentiment = article["ticker_sentiment"]
                .as_array()?
                .iter()
                .find(|entry| entry["ticker"].as_str() == Some(symbol))?;
            let url = article["url"].as_str()?;
            let parsed = Url::parse(url).ok()?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return None;
            }
            let headline = article["title"].as_str()?.trim();
            let source = article["source"].as_str()?.trim();
            if headline.is_empty() || source.is_empty() {
                return None;
            }
            let date = chrono::NaiveDateTime::parse_from_str(
                article["time_published"].as_str()?,
                "%Y%m%dT%H%M%S",
            )
            .ok()?
            .and_utc();
            let score = |key: &str, min: f64, max: f64| {
                sentiment[key]
                    .as_str()
                    .and_then(|v| v.parse::<f64>().ok())
                    .or_else(|| sentiment[key].as_f64())
                    .filter(|v| v.is_finite() && *v >= min && *v <= max)
            };
            if !seen.insert(url.to_string()) {
                return None;
            }
            let label = sentiment["ticker_sentiment_label"].as_str().filter(|v| {
                matches!(
                    *v,
                    "Bearish" | "Somewhat-Bearish" | "Neutral" | "Somewhat-Bullish" | "Bullish"
                )
            });
            Some(MarketNewsArticle {
                headline: headline.into(),
                summary: article["summary"].as_str().unwrap_or("").into(),
                source: source.into(),
                url: url.into(),
                created_at: date.to_rfc3339(),
                symbols: vec![symbol.into()],
                relevance_score: score("relevance_score", 0.0, 1.0),
                sentiment_score: score("ticker_sentiment_score", -1.0, 1.0),
                sentiment_label: label.map(str::to_owned),
            })
        })
        .collect())
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            '\r' => {}
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
}

fn parse_earnings_csv(body: &str) -> Result<Vec<EarningsEvent>, String> {
    if body.trim_start().starts_with('{') {
        return Err("Alpha Vantage did not return an earnings calendar".into());
    }
    let mut lines = body.lines();
    let headers = csv_fields(
        lines
            .next()
            .ok_or("Alpha Vantage returned an empty calendar")?,
    );
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Alpha Vantage earnings calendar omitted {name}"))
    };
    let symbol = column("symbol")?;
    let name = column("name")?;
    let report_date = column("reportDate")?;
    let fiscal_date_ending = column("fiscalDateEnding").ok();
    let estimate = column("estimate").ok();
    let currency = column("currency").ok();
    let mut events = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let values = csv_fields(line);
        let Some(symbol_value) = values
            .get(symbol)
            .map(|value| value.trim().to_ascii_uppercase())
        else {
            continue;
        };
        let Some(date) = values.get(report_date).map(|value| value.trim()) else {
            continue;
        };
        if symbol_value.is_empty() || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            continue;
        }
        events.push(EarningsEvent {
            symbol: symbol_value,
            name: values
                .get(name)
                .map(|value| value.trim().to_owned())
                .unwrap_or_default(),
            report_date: date.to_owned(),
            fiscal_date_ending: fiscal_date_ending
                .and_then(|index| values.get(index))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            estimate: estimate
                .and_then(|index| values.get(index))
                .and_then(|value| value.trim().parse().ok()),
            currency: currency
                .and_then(|index| values.get(index))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        });
    }
    Ok(events)
}

pub fn normalize_stock_symbols(symbols: Vec<String>) -> Vec<String> {
    symbols
        .into_iter()
        .filter_map(|symbol| stock_symbol(&symbol).ok())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn stock_symbol(value: &str) -> Result<String, String> {
    let symbol = value.trim().to_ascii_uppercase();
    if symbol.is_empty()
        || symbol.len() > 24
        || !symbol
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
    {
        return Err("Invalid stock symbol".into());
    }
    Ok(symbol)
}

fn parse_alpaca_market_schedule(
    payload: &Value,
    now: DateTime<Utc>,
) -> Result<AlpacaMarketSchedule, String> {
    let clocks = payload
        .get("clocks")
        .and_then(Value::as_array)
        .ok_or("Alpaca market clock did not include any markets")?;
    let market = |clock: &Value, names: &[&str]| {
        clock
            .pointer("/market/acronym")
            .or_else(|| clock.pointer("/market/mic"))
            .and_then(Value::as_str)
            .is_some_and(|market| names.contains(&market))
    };
    let nasdaq = clocks
        .iter()
        .find(|clock| market(clock, &["NASDAQ", "XNAS"]));
    let boats = clocks
        .iter()
        .find(|clock| market(clock, &["BOATS", "OCEA"]));
    let phase = |clock: Option<&Value>| {
        clock
            .and_then(|clock| clock.get("phase"))
            .and_then(Value::as_str)
            .unwrap_or("closed")
            .to_owned()
    };

    let (session, active_clock) = match phase(nasdaq).as_str() {
        "pre" => (AlpacaMarketSession::PreMarket, nasdaq),
        "core" | "lunch" => (AlpacaMarketSession::Core, nasdaq),
        "post" => (AlpacaMarketSession::AfterHours, nasdaq),
        _ if phase(boats) != "closed" => (AlpacaMarketSession::Overnight, boats),
        _ => (AlpacaMarketSession::Closed, None),
    };
    let parse_transition = |clock: &Value| {
        clock
            .get("phase_until")
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .filter(|value| *value > now)
    };
    let next_transition_at = active_clock
        .and_then(parse_transition)
        .or_else(|| clocks.iter().filter_map(parse_transition).min())
        .ok_or("Alpaca market clock did not include its next transition")?;

    Ok(AlpacaMarketSchedule {
        session,
        next_transition_at,
    })
}

fn quote_midpoint(value: &Value) -> Option<f64> {
    let bid = value.get("bp").and_then(Value::as_f64)?;
    let ask = value.get("ap").and_then(Value::as_f64)?;
    (bid.is_finite() && ask.is_finite() && bid > 0.0 && ask > 0.0).then_some((bid + ask) / 2.0)
}

fn weekly_reference_window(now: DateTime<Utc>) -> (String, String, String) {
    let target = now.date_naive() - Duration::days(7);
    (
        target.to_string(),
        (target - Duration::days(7)).to_string(),
        (target + Duration::days(1)).to_string(),
    )
}

fn parse_weekly_reference_closes(
    payload: &Value,
    target_date: &str,
) -> BTreeMap<String, WeeklyReference> {
    payload
        .get("bars")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(symbol, bars)| {
            bars.as_array()?
                .iter()
                .filter_map(|bar| {
                    let timestamp = bar.get("t")?.as_str()?;
                    let date = DateTime::parse_from_rfc3339(timestamp)
                        .ok()?
                        .date_naive()
                        .to_string();
                    let price = bar.get("c")?.as_f64()?;
                    (date.as_str() <= target_date && price.is_finite() && price > 0.0)
                        .then_some(WeeklyReference { price, date })
                })
                .max_by(|left, right| left.date.cmp(&right.date))
                .map(|reference| (symbol.clone(), reference))
        })
        .collect()
}

fn parse_market_snapshots(payload: &Value, prefer_quote: bool) -> BTreeMap<String, MarketSnapshot> {
    let snapshots = payload
        .get("snapshots")
        .unwrap_or(payload)
        .as_object()
        .into_iter()
        .flatten();
    snapshots
        .filter_map(|(symbol, snapshot)| {
            let quote_price = snapshot.pointer("/latestQuote").and_then(quote_midpoint);
            let trade_price = snapshot.pointer("/latestTrade/p").and_then(Value::as_f64);
            let price = if prefer_quote {
                quote_price.or(trade_price)
            } else {
                trade_price.or(quote_price)
            }
            .or_else(|| snapshot.pointer("/minuteBar/c").and_then(Value::as_f64))
            .or_else(|| snapshot.pointer("/dailyBar/c").and_then(Value::as_f64))?;
            let previous_close = snapshot
                .pointer("/prevDailyBar/c")
                .and_then(Value::as_f64)
                .unwrap_or(price);
            let previous_close_as_of = snapshot
                .pointer("/prevDailyBar/t")
                .and_then(Value::as_str)
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| (value.with_timezone(&Utc) + Duration::hours(16)).to_rfc3339());
            let as_of = (if prefer_quote {
                snapshot.pointer("/latestQuote/t")
            } else {
                snapshot.pointer("/latestTrade/t")
            })
            .or_else(|| snapshot.pointer("/latestTrade/t"))
            .or_else(|| snapshot.pointer("/latestQuote/t"))
            .or_else(|| snapshot.pointer("/minuteBar/t"))
            .or_else(|| snapshot.pointer("/dailyBar/t"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
            Some((
                symbol.clone(),
                MarketSnapshot {
                    symbol: symbol.clone(),
                    price,
                    previous_close,
                    previous_close_as_of,
                    daily_change_pct: if previous_close == 0.0 {
                        0.0
                    } else {
                        (price - previous_close) / previous_close * 100.0
                    },
                    weekly_change_pct: None,
                    weekly_reference_price: None,
                    weekly_reference_date: None,
                    as_of,
                },
            ))
        })
        .collect()
}

fn parse_market_bar_frames(
    payload: &Value,
    seed: &BTreeMap<String, MarketSnapshot>,
) -> Vec<MarketFrame> {
    let mut frames = BTreeMap::<String, BTreeMap<String, MarketSnapshot>>::new();
    for (symbol, bars) in payload
        .get("bars")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        for bar in bars.as_array().into_iter().flatten() {
            let Some(as_of) = bar.get("t").and_then(Value::as_str) else {
                continue;
            };
            let Some(price) = bar.get("c").and_then(Value::as_f64) else {
                continue;
            };
            let Some(snapshot) = stream_snapshot(seed.get(symbol), symbol, price, as_of) else {
                continue;
            };
            frames
                .entry(as_of.to_string())
                .or_default()
                .insert(symbol.clone(), snapshot);
        }
    }
    frames
        .into_iter()
        .map(|(as_of, snapshots)| MarketFrame { as_of, snapshots })
        .collect()
}

fn replace_plaid_credentials(
    vault: &mut CredentialVault,
    credentials: &PlaidCredentials,
) -> Result<(), String> {
    let client_changed = vault
        .entries
        .get(PLAID_CREDENTIALS_KEY)
        .cloned()
        .map(serde_json::from_value::<PlaidCredentials>)
        .transpose()
        .map_err(|error| error.to_string())?
        .is_some_and(|previous| previous.client_id != credentials.client_id);

    if client_changed {
        // Plaid Item access tokens and Multi-Item Link users are scoped to the client. Keeping
        // them after a client switch makes every subsequent sync fail with the new secret.
        vault.entries.remove(PLAID_ITEMS_KEY);
        vault.entries.remove(PLAID_USER_KEY);
    }
    vault.entries.insert(
        PLAID_CREDENTIALS_KEY.into(),
        serde_json::to_value(credentials).map_err(|error| error.to_string())?,
    );
    Ok(())
}

fn parse_alpaca_history(payload: &Value) -> Vec<Value> {
    payload
        .get("bars")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|bar| {
            let date = bar.get("t")?.as_str()?.get(..10)?;
            let value = bar.get("c")?;
            let numeric = value.as_f64()?;
            (numeric.is_finite() && numeric > 0.0).then(|| json!({ "date": date, "value": value }))
        })
        .collect()
}

async fn provider_response(
    provider: &str,
    endpoint: &str,
    response: reqwest::Response,
) -> Result<Value, String> {
    ProviderError::from_response(provider, endpoint, response, 1)
        .await
        .map_err(|error| error.to_string())
}

fn keychain_entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|error| error.to_string())
}

fn read_keychain<T: DeserializeOwned>(user: &str) -> Result<Option<T>, String> {
    match keychain_entry(user)?.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read secure provider settings: {error}")),
    }
}

fn write_keychain<T: Serialize>(user: &str, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    keychain_entry(user)?
        .set_password(&encoded)
        .map_err(|error| format!("Could not save provider settings securely: {error}"))
}

fn migrate_legacy_credentials() -> Result<CredentialVault, String> {
    let mut vault = CredentialVault::default();
    for key in [
        PLAID_CREDENTIALS_KEY,
        PLAID_ITEMS_KEY,
        PLAID_USER_KEY,
        SNAPTRADE_CREDENTIALS_KEY,
        ALPACA_CREDENTIALS_KEY,
    ] {
        if let Some(value) = read_keychain::<Value>(key)? {
            vault.entries.insert(key.into(), value);
        }
    }
    if !vault.entries.is_empty() {
        write_protected_vault(&vault)?;
    }
    Ok(vault)
}

fn read_protected_vault() -> Result<Option<CredentialVault>, String> {
    read_keychain(CREDENTIAL_VAULT_KEY)
}

fn write_protected_vault(vault: &CredentialVault) -> Result<(), String> {
    write_keychain(CREDENTIAL_VAULT_KEY, vault)
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

fn plaid_user_identifier(user: &PlaidUser) -> Result<(&'static str, &str), String> {
    if let Some(user_token) = user.user_token.as_deref() {
        Ok(("user_token", user_token))
    } else if let Some(user_id) = user.user_id.as_deref() {
        Ok(("user_id", user_id))
    } else {
        Err("Plaid did not return a user ID for Multi-Item Link".into())
    }
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

fn plaid_recurring_amount(value: &Value, field: &str) -> Result<PlaidRecurringAmount, String> {
    let amount = value
        .pointer(&format!("/{field}/amount"))
        .and_then(Value::as_f64)
        .filter(|amount| amount.is_finite())
        .ok_or_else(|| format!("Plaid recurring stream omitted {field}.amount"))?;
    let currency = value
        .pointer(&format!("/{field}/iso_currency_code"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .pointer(&format!("/{field}/unofficial_currency_code"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned);
    Ok(PlaidRecurringAmount { amount, currency })
}

fn parse_plaid_recurring_stream(
    value: &Value,
    direction: &'static str,
) -> Result<PlaidRecurringStream, String> {
    let category = value
        .pointer("/personal_finance_category/primary")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("category")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" · ")
                })
        })
        .filter(|category| !category.is_empty());
    Ok(PlaidRecurringStream {
        stream_id: string_field(value, "stream_id")?,
        account_id: format!("plaid:{}", string_field(value, "account_id")?),
        direction,
        description: string_field(value, "description")?,
        merchant_name: value
            .get("merchant_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        category,
        frequency: string_field(value, "frequency")?,
        status: string_field(value, "status")?,
        is_active: bool_field(value, "is_active")?,
        first_date: string_field(value, "first_date")?,
        last_date: string_field(value, "last_date")?,
        predicted_next_date: value
            .get("predicted_next_date")
            .and_then(Value::as_str)
            .map(str::to_owned),
        average_amount: plaid_recurring_amount(value, "average_amount")?,
        last_amount: plaid_recurring_amount(value, "last_amount")?,
        transaction_count: required_array(value, "transaction_ids")?.len(),
    })
}

fn parse_plaid_recurring_streams(value: Value) -> Result<Vec<PlaidRecurringStream>, String> {
    let mut streams = Vec::new();
    for (field, direction) in [("inflow_streams", "inflow"), ("outflow_streams", "outflow")] {
        for stream in required_array(&value, field)? {
            streams.push(parse_plaid_recurring_stream(&stream, direction)?);
        }
    }
    streams.sort_by(|left, right| {
        left.predicted_next_date
            .cmp(&right.predicted_next_date)
            .then_with(|| left.description.cmp(&right.description))
    });
    Ok(streams)
}

fn plaid_public_tokens(value: &Value) -> Vec<String> {
    let mut tokens = Vec::new();
    for session in array(value, "link_sessions") {
        if let Some(token) = session
            .pointer("/on_success/public_token")
            .and_then(Value::as_str)
        {
            tokens.push(token.into());
        }
        if let Some(results) = session
            .pointer("/results/item_add_results")
            .and_then(Value::as_array)
        {
            tokens.extend(
                results
                    .iter()
                    .filter_map(|item| item.get("public_token").and_then(Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn plaid_link_completed(value: &Value) -> bool {
    array(value, "link_sessions").iter().any(|session| {
        session.get("finished_at").and_then(Value::as_str).is_some()
            && (session.get("on_success").is_some_and(Value::is_object)
                || session
                    .pointer("/results/item_add_results")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
                || array(session, "events").iter().any(|event| {
                    event.get("event_name").and_then(Value::as_str) == Some("HANDOFF")
                }))
    })
}

// Only enabled connections are in the baseline, so repairing one is observable
// even when SnapTrade preserves its connection and account IDs.
fn snaptrade_active_connection_ids(value: &Value) -> Result<BTreeSet<String>, String> {
    let connections = value
        .as_array()
        .ok_or("SnapTrade response did not include a connection list")?;
    let mut active = BTreeSet::new();
    for connection in connections {
        let id = string_field(connection, "id")?;
        let disabled = connection
            .get("disabled")
            .and_then(Value::as_bool)
            .ok_or("SnapTrade connection did not include its disabled status")?;
        if !disabled {
            active.insert(id);
        }
    }
    Ok(active)
}

fn snaptrade_link_status(
    response: &Value,
    baseline: &BTreeSet<String>,
    browser_completed: bool,
) -> Result<LinkStatus, String> {
    let active = snaptrade_active_connection_ids(response)?;
    if browser_completed && active.is_empty() {
        return Err(
            "No active SnapTrade connection found. Finish connecting in your browser and try again"
                .into(),
        );
    }
    Ok(if browser_completed || !active.is_subset(baseline) {
        LinkStatus {
            status: "connected".into(),
        }
    } else {
        pending_status()
    })
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

fn persist_vault_change(
    vault: &mut CredentialVault,
    change: impl FnOnce(&mut CredentialVault) -> Result<(), String>,
    persist: impl FnOnce(&CredentialVault) -> Result<(), String>,
) -> Result<(), String> {
    let mut next = vault.clone();
    change(&mut next)?;
    persist(&next)?;
    *vault = next;
    Ok(())
}

fn update_plaid_items(
    vault: &mut CredentialVault,
    change: impl FnOnce(&mut Vec<PlaidItem>),
) -> Result<(), String> {
    let mut items: Vec<PlaidItem> = serde_json::from_value(
        vault
            .entries
            .get(PLAID_ITEMS_KEY)
            .cloned()
            .unwrap_or(json!([])),
    )
    .map_err(|_| "Invalid saved Plaid connections")?;
    change(&mut items);
    vault.entries.insert(
        PLAID_ITEMS_KEY.into(),
        serde_json::to_value(items).map_err(|error| error.to_string())?,
    );
    Ok(())
}

fn upsert_plaid_item(vault: &mut CredentialVault, item: PlaidItem) -> Result<(), String> {
    update_plaid_items(vault, |items| {
        if let Some(existing) = items
            .iter_mut()
            .find(|existing| existing.item_id == item.item_id)
        {
            existing.access_token = item.access_token;
        } else {
            items.push(item);
        }
    })
}

fn merge_plaid_data(target: &mut PlaidData, mut fresh: PlaidData) {
    target.diagnostics.append(&mut fresh.diagnostics);
    let ids = fresh
        .accounts
        .iter()
        .chain(&fresh.investment_accounts)
        .filter_map(|account| account.get("account_id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let retained = |value: &Value| {
        !value
            .get("account_id")
            .and_then(Value::as_str)
            .is_some_and(|id| ids.contains(id))
    };
    target.accounts.retain(retained);
    target.investment_accounts.retain(retained);
    target.transactions.retain(retained);
    target.holdings.retain(retained);
    target
        .transaction_history_start
        .retain(|id, _| !ids.contains(id));
    target
        .transaction_history_start
        .extend(fresh.transaction_history_start);
    target.accounts.extend(fresh.accounts);
    target.investment_accounts.extend(fresh.investment_accounts);
    target.transactions.extend(fresh.transactions);
    target.holdings.extend(fresh.holdings);
    let mut securities: BTreeMap<String, Value> = target
        .securities
        .drain(..)
        .chain(fresh.securities)
        .filter_map(|security| Some((security.get("security_id")?.as_str()?.to_owned(), security)))
        .collect();
    target.securities = std::mem::take(&mut securities).into_values().collect();
}

fn merge_recent_activities(cached: &[Value], fresh: Vec<Value>, since: &str) -> Vec<Value> {
    let fresh_ids: BTreeSet<&str> = fresh
        .iter()
        .filter_map(|activity| activity.get("id").and_then(Value::as_str))
        .collect();
    let mut result: Vec<Value> = cached
        .iter()
        .filter(|activity| {
            let date = activity
                .get("trade_date")
                .and_then(Value::as_str)
                .or_else(|| activity.get("settlement_date").and_then(Value::as_str))
                .unwrap_or("");
            !date.is_empty()
                && date < since
                && !activity
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| fresh_ids.contains(id))
        })
        .cloned()
        .collect();
    result.extend(fresh);
    result
}

fn pending_status() -> LinkStatus {
    LinkStatus {
        status: "pending".into(),
    }
}

fn allowed_plaid_account(account: &Value) -> bool {
    matches!(
        (
            account.get("type").and_then(Value::as_str),
            account.get("subtype").and_then(Value::as_str),
        ),
        (Some("depository"), Some("checking" | "savings")) | (Some("credit"), Some("credit card"))
    )
}

fn supported_snaptrade_account(account: &Value) -> bool {
    account
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaid_recurring_report_preserves_provider_evidence_and_direction() {
        let streams = parse_plaid_recurring_streams(json!({
            "inflow_streams": [],
            "outflow_streams": [{
                "stream_id": "stream-1",
                "account_id": "account-1",
                "description": "Example membership",
                "merchant_name": "Example",
                "personal_finance_category": { "primary": "ENTERTAINMENT" },
                "frequency": "MONTHLY",
                "status": "MATURE",
                "is_active": true,
                "first_date": "2026-06-01",
                "last_date": "2026-09-01",
                "predicted_next_date": "2026-10-01",
                "average_amount": { "amount": 12.5, "iso_currency_code": "USD" },
                "last_amount": { "amount": 13.0, "iso_currency_code": "USD" },
                "transaction_ids": ["one", "two", "three"]
            }]
        }))
        .expect("valid recurring response");

        assert_eq!(streams.len(), 1);
        let stream = &streams[0];
        assert_eq!(stream.account_id, "plaid:account-1");
        assert_eq!(stream.direction, "outflow");
        assert_eq!(stream.status, "MATURE");
        assert_eq!(stream.transaction_count, 3);
        assert_eq!(stream.average_amount.amount, 12.5);
        assert_eq!(stream.predicted_next_date.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn plaid_rejects_cursor_cycles_and_unbounded_pages_but_accepts_noop_completion() {
        let mut seen = BTreeSet::from(["saved".into()]);
        assert!(validate_plaid_page("next", true, 0, &mut seen).is_ok());
        assert!(validate_plaid_page("saved", true, 1, &mut seen).is_err());
        assert!(validate_plaid_page("last", true, 99, &mut seen).is_err());
        assert!(validate_plaid_page("saved", false, 0, &mut seen).is_ok());
        assert!(validate_plaid_page("last", false, 99, &mut seen).is_ok());
    }

    #[test]
    fn failed_vault_persistence_does_not_publish_the_mutation() {
        let original = CredentialVault {
            entries: BTreeMap::from([("first".into(), json!("synthetic"))]),
        };
        let mut vault = original.clone();
        assert!(persist_vault_change(
            &mut vault,
            |vault| {
                vault.entries.clear();
                Ok(())
            },
            |_| Err("Persistence failed".into()),
        )
        .is_err());
        assert_eq!(vault.entries, original.entries);
    }

    #[test]
    fn snaptrade_link_detects_repairs_and_verifies_explicit_browser_completion() {
        let before = json!([
            {"id": "existing", "disabled": false},
            {"id": "repair", "disabled": true}
        ]);
        let baseline = snaptrade_active_connection_ids(&before).unwrap();
        assert_eq!(
            snaptrade_link_status(&before, &baseline, false)
                .unwrap()
                .status,
            "pending"
        );
        let repaired = json!([
            {"id": "existing", "disabled": false},
            {"id": "repair", "disabled": false}
        ]);
        assert_eq!(
            snaptrade_link_status(&repaired, &baseline, false)
                .unwrap()
                .status,
            "connected"
        );
        let added = json!([{"id": "new", "disabled": false}]);
        assert_eq!(
            snaptrade_link_status(&added, &baseline, false)
                .unwrap()
                .status,
            "connected"
        );
        // Reopening a healthy connection may leave every provider field unchanged.
        assert_eq!(
            snaptrade_link_status(&before, &baseline, true)
                .unwrap()
                .status,
            "connected"
        );
        for unavailable in [json!([]), json!([{"id": "repair", "disabled": true}])] {
            assert_eq!(
                snaptrade_link_status(&unavailable, &baseline, false)
                    .unwrap()
                    .status,
                "pending"
            );
            assert!(snaptrade_link_status(&unavailable, &baseline, true).is_err());
        }
        for malformed in [
            json!({}),
            json!([{"id": "existing"}]),
            json!([{"disabled": false}]),
        ] {
            assert!(snaptrade_link_status(&malformed, &baseline, true).is_err());
        }
    }

    #[test]
    fn link_results_require_current_credentials_and_an_active_session() {
        let providers = Providers::new().unwrap();
        let (cancel, _) = tokio::sync::watch::channel(false);
        let mut session = PendingSession {
            link: PendingLink::SnapTrade {
                existing_connection_ids: BTreeSet::from(["existing".into()]),
            },
            generation: 0,
            expires: Instant::now() + StdDuration::from_secs(300),
            polling: false,
            cancel,
        };
        assert!(validate_link_session(&session, 0).is_ok());
        assert!(validate_link_session(&session, 1).is_err());
        session.expires = Instant::now() - StdDuration::from_secs(1);
        assert!(validate_link_session(&session, 0).is_err());
        assert!(providers
            .mutate_link_vault("cancelled", |_| panic!("Cancelled session must not mutate"))
            .is_err());
        assert!(!plaid_link_completed(
            &json!({"link_sessions": [{"finished_at": null, "on_success": {"public_token": "synthetic"}}]})
        ));
        assert!(plaid_link_completed(
            &json!({"link_sessions": [{"finished_at": "2026-09-01T12:00:00Z", "events": [{"event_name": "HANDOFF"}]}]})
        ));
    }

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

    #[test]
    fn recent_activity_refresh_replaces_recent_records_but_retains_backfill() {
        let cached = vec![
            json!({"id": "old", "trade_date": "2026-01-01"}),
            json!({"id": "deleted", "trade_date": "2026-09-01"}),
            json!({"id": "moved", "trade_date": "2026-01-01"}),
        ];
        let result = merge_recent_activities(
            &cached,
            vec![
                json!({"id": "new", "trade_date": "2026-09-02"}),
                json!({"id": "moved", "trade_date": "2026-09-03"}),
            ],
            "2026-08-01",
        );
        assert_eq!(
            result
                .iter()
                .map(|value| value["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["old", "new", "moved"]
        );
    }

    #[test]
    fn partial_plaid_merge_retains_unrelated_cached_accounts() {
        let mut old = PlaidData {
            accounts: vec![
                json!({"account_id": "bank", "value": 100}),
                json!({"account_id": "broken", "value": 50}),
            ],
            transactions: vec![
                json!({"account_id": "bank", "transaction_id": "removed"}),
                json!({"account_id": "broken", "transaction_id": "saved"}),
            ],
            transaction_history_start: BTreeMap::from([
                ("bank".into(), "2026-01-01".into()),
                ("broken".into(), "2026-02-01".into()),
            ]),
            ..Default::default()
        };
        merge_plaid_data(
            &mut old,
            PlaidData {
                accounts: vec![json!({"account_id": "bank", "value": 110})],
                transactions: vec![json!({"account_id": "bank", "transaction_id": "new"})],
                transaction_history_start: BTreeMap::from([("bank".into(), "2026-03-01".into())]),
                ..Default::default()
            },
        );
        assert_eq!(old.accounts.len(), 2);
        assert_eq!(old.accounts[0]["value"], 50);
        assert_eq!(old.accounts[1]["value"], 110);
        assert_eq!(
            old.transactions
                .iter()
                .map(|value| value["transaction_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["saved", "new"]
        );
        assert_eq!(
            old.transaction_history_start,
            BTreeMap::from([
                ("bank".into(), "2026-03-01".into()),
                ("broken".into(), "2026-02-01".into()),
            ])
        );
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
    async fn empty_market_requests_match_frontend_shape_without_credentials() {
        let providers = Providers::new().unwrap();
        let response = providers
            .market_snapshots(vec!["bad,symbol".into()])
            .await
            .unwrap();
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["snapshots"], json!({}));
        assert_eq!(value["session"], "Market closed");
        assert_eq!(value["pollIntervalMs"], Value::Null);
        assert_eq!(
            normalize_stock_symbols(vec![
                " aapl ".into(),
                "AAPL".into(),
                "BRK.B".into(),
                "bad,symbol".into()
            ]),
            vec!["AAPL", "BRK.B"],
        );
    }

    #[tokio::test]
    async fn saved_news_reads_need_no_credentials_and_spend_no_request() {
        let path = std::env::temp_dir().join(format!(
            "brief-provider-news-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let now = Utc::now().timestamp();
        let mut cache = news_cache::NewsCache::new(Some(&path)).unwrap();
        cache.reserve(Some("TEST"), now).unwrap();
        cache
            .save(
                "TEST",
                &json!({"feed": [{
                    "title": "Synthetic news", "summary": "Synthetic evidence", "source": "Example",
                    "url": "https://example.com/story", "time_published": "20260921T120000",
                    "ticker_sentiment": [{"ticker":"TEST", "relevance_score":"0.91", "ticker_sentiment_score":"-0.3", "ticker_sentiment_label":"Somewhat-Bearish"}]
                }]}),
                now,
            )
            .unwrap();
        drop(cache);

        let providers = Providers::new().unwrap().with_news_cache(&path).unwrap();
        let result = providers
            .market_news(vec!["TEST".into()], false)
            .await
            .unwrap();
        assert_eq!(result.articles.len(), 1);
        assert_eq!(result.requests_remaining, 19);
        drop(providers);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn alpha_news_preserves_ticker_scores_and_rejects_errors_and_unsafe_articles() {
        let article = json!({"title":"Synthetic news", "source":"Example", "url":"https://example.com/story",
            "summary":"Synthetic evidence", "time_published":"20260921T120000", "overall_sentiment_score": -0.8,
            "ticker_sentiment":[{"ticker":"TEST", "relevance_score":"0.91", "ticker_sentiment_score":"0.3", "ticker_sentiment_label":"Somewhat-Bullish"}]});
        let mut unsafe_article = article.clone();
        unsafe_article["url"] = "javascript:alert(1)".into();
        let articles = parse_alpha_vantage_news(
            &json!({"feed":[article.clone(), unsafe_article, article.clone()]}),
            "TEST",
        )
        .unwrap();
        assert_eq!(articles.len(), 1);
        let value = serde_json::to_value(&articles[0]).unwrap();
        assert_eq!(value["relevanceScore"], 0.91);
        assert_eq!(value["sentimentScore"], 0.3);
        assert_eq!(value["createdAt"], "2026-09-21T12:00:00+00:00");
        assert!(
            parse_alpha_vantage_news(&json!({"feed":[article.clone()]}), "OTHER")
                .unwrap()
                .is_empty()
        );
        for payload in [
            json!({"Note":"quota"}),
            json!({"Information":"access"}),
            json!({"Error Message":"invalid"}),
            json!({}),
        ] {
            assert!(parse_alpha_vantage_news(&payload, "TEST").is_err());
        }
        let mut invalid = article;
        invalid["ticker_sentiment"][0]["relevance_score"] = "NaN".into();
        assert!(
            parse_alpha_vantage_news(&json!({"feed":[invalid]}), "TEST").unwrap()[0]
                .relevance_score
                .is_none()
        );
    }

    #[test]
    fn plaid_imports_supported_cash_and_card_accounts() {
        assert!(allowed_plaid_account(
            &json!({ "type": "depository", "subtype": "checking" })
        ));
        assert!(allowed_plaid_account(
            &json!({ "type": "credit", "subtype": "credit card" })
        ));
        assert!(!allowed_plaid_account(
            &json!({ "type": "investment", "subtype": "brokerage" })
        ));
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

    #[test]
    fn snaptrade_accepts_accounts_without_an_account_category() {
        let account = json!({
            "id": "account-id",
            "name": "E*Trade Roth IRA",
            "raw_type": "ROTHIRA",
            "account_category": null
        });

        assert!(supported_snaptrade_account(&account));
    }

    #[test]
    fn plaid_uses_new_user_ids_and_supports_legacy_user_tokens() {
        let new_user = PlaidUser {
            client_id: "client".into(),
            user_id: Some("usr_123".into()),
            user_token: None,
        };
        assert_eq!(
            plaid_user_identifier(&new_user).unwrap(),
            ("user_id", "usr_123")
        );

        let legacy_user = PlaidUser {
            client_id: "client".into(),
            user_id: Some("legacy-id".into()),
            user_token: Some("user-production-123".into()),
        };
        assert_eq!(
            plaid_user_identifier(&legacy_user).unwrap(),
            ("user_token", "user-production-123")
        );
    }

    #[test]
    fn changing_plaid_clients_discards_client_scoped_tokens() {
        let previous = PlaidCredentials {
            client_id: "old-client".into(),
            secret: "old-secret".into(),
        };
        let replacement = PlaidCredentials {
            client_id: "new-client".into(),
            secret: "new-secret".into(),
        };
        let mut vault = CredentialVault::default();
        vault.entries.insert(
            PLAID_CREDENTIALS_KEY.into(),
            serde_json::to_value(previous).unwrap(),
        );
        vault
            .entries
            .insert(PLAID_ITEMS_KEY.into(), json!([{"accessToken": "token"}]));
        vault
            .entries
            .insert(PLAID_USER_KEY.into(), json!({"userId": "user"}));

        replace_plaid_credentials(&mut vault, &replacement).unwrap();

        assert!(!vault.entries.contains_key(PLAID_ITEMS_KEY));
        assert!(!vault.entries.contains_key(PLAID_USER_KEY));
        assert_eq!(
            vault.entries[PLAID_CREDENTIALS_KEY]["clientId"],
            "new-client"
        );
    }

    #[test]
    fn extracts_multi_item_hosted_link_tokens() {
        let tokens = plaid_public_tokens(&json!({
            "link_sessions": [{
                "results": { "item_add_results": [
                    { "public_token": "public-b" },
                    { "public_token": "public-a" }
                ]}
            }]
        }));
        assert_eq!(tokens, vec!["public-a", "public-b"]);
    }

    #[test]
    fn parses_available_alpaca_daily_closes() {
        assert_eq!(
            parse_alpaca_history(&json!({
                "bars": [
                    { "t": "2026-08-28T04:00:00Z", "c": 211.82 },
                    { "t": "2026-08-29T04:00:00Z", "c": null }
                ]
            })),
            vec![json!({ "date": "2026-08-28", "value": 211.82 })]
        );
    }

    #[test]
    fn weekly_reference_is_seven_calendar_days_earlier() {
        let now = DateTime::parse_from_rfc3339("2026-09-15T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            weekly_reference_window(now),
            (
                "2026-09-08".into(),
                "2026-09-01".into(),
                "2026-09-09".into()
            )
        );
    }

    #[test]
    fn weekly_references_use_the_latest_bar_on_or_before_the_target() {
        assert_eq!(
            parse_weekly_reference_closes(
                &json!({
                    "bars": {
                        "AAPL": [
                            { "t": "2026-09-04T04:00:00Z", "c": 230.0 },
                            { "t": "2026-09-02T04:00:00Z", "c": 220.0 },
                            { "t": "2026-09-03T04:00:00Z", "c": 225.0 }
                        ],
                        "NBIS": [{ "t": "2026-09-01T04:00:00Z", "c": 25.0 }]
                    }
                }),
                "2026-09-03"
            ),
            BTreeMap::from([
                (
                    "AAPL".into(),
                    WeeklyReference {
                        price: 225.0,
                        date: "2026-09-03".into()
                    }
                ),
                (
                    "NBIS".into(),
                    WeeklyReference {
                        price: 25.0,
                        date: "2026-09-01".into()
                    }
                )
            ])
        );
    }

    #[test]
    fn parses_alpaca_market_data_and_rejects_unsafe_symbols() {
        let snapshots = parse_market_snapshots(
            &json!({
                "snapshots": { "AAPL": {
                    "latestTrade": { "p": 230.0, "t": "2026-09-01T17:00:00Z" },
                    "prevDailyBar": { "c": 225.0, "t": "2026-08-31T04:00:00Z" }
                }}
            }),
            false,
        );
        let snapshot = snapshots.get("AAPL").unwrap();
        assert_eq!(snapshot.price, 230.0);
        assert_eq!(
            snapshot.previous_close_as_of.as_deref(),
            Some("2026-08-31T20:00:00+00:00")
        );
        assert!((snapshot.daily_change_pct - 2.222222).abs() < 0.000001);

        assert!(stock_symbol("AAPL,MSFT").is_err());
    }

    #[test]
    fn selects_alpaca_feeds_from_the_authoritative_market_clock_and_parses_quotes() {
        let schedule = |nasdaq_phase: &str, boats_phase: &str| {
            parse_alpaca_market_schedule(
                &json!({ "clocks": [
                    {
                        "market": { "acronym": "NASDAQ" },
                        "phase": nasdaq_phase,
                        "phase_until": "2026-09-08T20:00:00Z"
                    },
                    {
                        "market": { "acronym": "BOATS" },
                        "phase": boats_phase,
                        "phase_until": "2026-09-08T08:00:00Z"
                    }
                ]}),
                "2026-09-08T01:00:00Z".parse().unwrap(),
            )
            .unwrap()
            .session
        };
        assert_eq!(schedule("pre", "closed"), AlpacaMarketSession::PreMarket);
        assert_eq!(schedule("core", "closed"), AlpacaMarketSession::Core);
        assert_eq!(schedule("post", "closed"), AlpacaMarketSession::AfterHours);
        assert_eq!(schedule("closed", "core"), AlpacaMarketSession::Overnight);
        assert_eq!(schedule("closed", "closed"), AlpacaMarketSession::Closed);

        let holiday = parse_alpaca_market_schedule(
            &json!({ "clocks": [{
                "market": { "acronym": "NASDAQ" },
                "phase": "closed",
                "phase_until": "2026-09-08T08:00:00-04:00"
            }]}),
            "2026-09-07T14:00:00Z".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(holiday.session, AlpacaMarketSession::Closed);
        assert_eq!(
            holiday.next_transition_at.to_rfc3339(),
            "2026-09-08T12:00:00+00:00"
        );

        assert_eq!(
            quote_midpoint(&json!({ "bp": 229.0, "ap": 231.0 })),
            Some(230.0)
        );

        let snapshots = parse_market_snapshots(
            &json!({ "snapshots": { "AAPL": {
                "latestTrade": { "p": 225.0, "t": "2026-09-02T23:45:00Z" },
                "latestQuote": { "bp": 229.0, "ap": 231.0, "t": "2026-09-03T00:00:00Z" },
                "prevDailyBar": { "c": 224.0 }
            }}}),
            true,
        );
        assert_eq!(snapshots["AAPL"].price, 230.0);
        assert_eq!(snapshots["AAPL"].as_of, "2026-09-03T00:00:00Z");
    }

    #[test]
    fn groups_minute_bars_and_preserves_price_references() {
        let seed = BTreeMap::from([(
            "AAPL".into(),
            MarketSnapshot {
                symbol: "AAPL".into(),
                price: 230.0,
                previous_close: 225.0,
                previous_close_as_of: Some("2026-09-05T20:00:00Z".into()),
                daily_change_pct: 0.0,
                weekly_change_pct: Some(4.5),
                weekly_reference_price: Some(220.0),
                weekly_reference_date: Some("2026-09-01".into()),
                as_of: "2026-09-08T01:02:00Z".into(),
            },
        )]);
        let frames = parse_market_bar_frames(
            &json!({ "bars": { "AAPL": [
                { "t": "2026-09-08T01:00:00Z", "c": 228.0 },
                { "t": "2026-09-08T01:01:00Z", "c": 229.0 }
            ]}}),
            &seed,
        );

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].snapshots["AAPL"].price, 228.0);
        assert_eq!(frames[0].snapshots["AAPL"].previous_close, 225.0);
        assert_eq!(frames[1].as_of, "2026-09-08T01:01:00Z");

        let market = MarketSnapshots {
            snapshots: seed.clone(),
            session: "Market closed".into(),
            feed: "iex".into(),
            delay_minutes: 0,
            as_of: Some("2026-09-08T01:02:00Z".into()),
            next_transition_at: Some("2026-09-08T08:00:00Z".into()),
            poll_interval_ms: None,
            history_feed: "iex".into(),
            history_delay_minutes: 0,
        };
        let mut bar_frames = BTreeMap::new();
        let mut published = Vec::new();
        let mut publish = |tick: MarketStreamTick| {
            published.push(tick.chart_as_of.unwrap());
            Ok(())
        };
        publish_market_backfill(
            &market,
            frames.clone(),
            &seed,
            &mut bar_frames,
            &mut publish,
        )
        .unwrap();
        publish_market_backfill(&market, frames, &seed, &mut bar_frames, &mut publish).unwrap();
        assert_eq!(published, ["2026-09-08T01:00:00Z", "2026-09-08T01:01:00Z"]);
        assert_eq!(AlpacaMarketSession::Overnight.history_feed(), "boats");
        assert_eq!(AlpacaMarketSession::Overnight.history_delay_minutes(), 15);
    }

    #[test]
    fn parses_quoted_earnings_calendar_rows() {
        let events = parse_earnings_csv(
            "symbol,name,reportDate,fiscalDateEnding,estimate,currency\nAAPL,\"Apple, Inc.\",2026-10-29,2026-09-30,1.42,USD\n",
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].symbol, "AAPL");
        assert_eq!(events[0].name, "Apple, Inc.");
        assert_eq!(events[0].report_date, "2026-10-29");
        assert_eq!(events[0].estimate, Some(1.42));
    }
}
