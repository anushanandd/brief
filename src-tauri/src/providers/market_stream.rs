//! App-owned transports. Consumers select data; they never replace subscriptions.
use super::alpaca::quote_midpoint;
use super::*;
use chrono::FixedOffset;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

#[derive(Default)]
pub struct MarketStreams {
    inner: Mutex<Registry>,
    pub epoch: Arc<AtomicU64>,
}
#[derive(Default)]
struct Registry {
    enabled: bool,
    symbols: Vec<String>,
    feeds: BTreeMap<String, Arc<Feed>>,
    leases: BTreeMap<String, Arc<tokio::sync::Mutex<()>>>,
}
#[derive(Clone, Default)]
pub struct FeedState {
    pub subscribed: BTreeSet<String>,
    pub quotes: BTreeMap<String, Value>,
    pub bars: BTreeMap<String, Vec<Value>>,
    pub error: Option<String>,
    pub finished: bool,
}

impl FeedState {
    /// Keeps a price observation unless the saved one is newer, so prices never move back in
    /// time. Returns whether it was kept.
    fn observe(&mut self, symbol: &str, item: &Value, time: DateTime<FixedOffset>) -> bool {
        let newer = self
            .quotes
            .get(symbol)
            .and_then(stamp)
            .is_none_or(|saved| time >= saved);
        if newer {
            self.quotes.insert(symbol.into(), item.clone());
        }
        newer
    }

    /// A corrected or cancelled trade: the symbol's live observations are no longer trusted.
    fn forget(&mut self, symbol: &str) {
        self.quotes.remove(symbol);
        self.bars.remove(symbol);
    }

    /// Keeps the two latest minute bars, a bar replacing one with the same start. Returns
    /// whether an updated bar is older than every bar kept, so history needs repair.
    fn record_bar(&mut self, symbol: &str, item: &Value, time: DateTime<FixedOffset>) -> bool {
        let bars = self.bars.entry(symbol.into()).or_default();
        bars.retain(|bar| bar["t"] != item["t"]);
        let repair = item["T"] == "u"
            && bars
                .iter()
                .filter_map(stamp)
                .min()
                .is_some_and(|oldest| time < oldest);
        bars.push(item.clone());
        bars.sort_by_key(stamp);
        if bars.len() > 2 {
            bars.remove(0);
        }
        repair
    }
}

/// When a stream message happened.
fn stamp(item: &Value) -> Option<DateTime<FixedOffset>> {
    item["t"]
        .as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
}

/// The message's symbol, if it is one this connection asked for.
fn listed<'a>(item: &'a Value, requested: &[String]) -> Option<&'a str> {
    item["S"]
        .as_str()
        .filter(|symbol| requested.iter().any(|value| value == symbol))
}

/// A trade's price, or an overnight quote's midpoint unless the quote is crossed.
fn stream_price(item: &Value, feed: &str) -> Option<f64> {
    if feed != "overnight" {
        item["p"].as_f64()
    } else if item["bp"].as_f64() > item["ap"].as_f64() {
        None
    } else {
        quote_midpoint(item)
    }
}

pub struct Feed {
    pub state: Arc<Mutex<FeedState>>,
    pub events: broadcast::Sender<Vec<Value>>,
    cancel: watch::Sender<bool>,
}
impl MarketStreams {
    pub fn start(&self, symbols: Vec<String>) {
        let mut registry = self.inner.lock().unwrap();
        for feed in registry.feeds.values() {
            let _ = feed.cancel.send(true);
        }
        registry.feeds.clear();
        registry.symbols = symbols;
        registry.enabled = true;
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
    pub fn stop(&self) {
        let mut registry = self.inner.lock().unwrap();
        registry.enabled = false;
        for feed in registry.feeds.values() {
            let _ = feed.cancel.send(true);
        }
        registry.feeds.clear();
    }
    pub fn symbols(&self) -> Vec<String> {
        self.inner.lock().unwrap().symbols.clone()
    }
    pub(super) fn acquire(
        &self,
        feed: &str,
        credentials: AlpacaCredentials,
    ) -> Result<Arc<Feed>, String> {
        let mut registry = self.inner.lock().map_err(|_| "Market state unavailable")?;
        if !registry.enabled {
            return Err("Market data is paused".into());
        }
        if let Some(existing) = registry.feeds.get(feed) {
            if !existing.state.lock().unwrap().finished {
                return Ok(existing.clone());
            }
        }
        let symbols = registry.symbols.clone();
        let lease = registry.leases.entry(feed.into()).or_default().clone();
        let (cancel, mut cancellation) = watch::channel(false);
        let (events, _) = broadcast::channel(256);
        let state = Arc::new(Mutex::new(FeedState::default()));
        let handle = Arc::new(Feed {
            state: state.clone(),
            events: events.clone(),
            cancel,
        });
        registry.feeds.insert(feed.into(), handle.clone());
        let feed = feed.to_owned();
        let epoch = self.epoch.clone();
        tokio::spawn(async move {
            let lease_cancellation = cancellation.clone();
            let work = async {
                // The previous transport must be dropped before its replacement authenticates.
                let _lease = socket_lease(&lease, lease_cancellation)
                    .await
                    .ok_or("Market data paused")?;
                let version = if matches!(feed.as_str(), "boats" | "overnight") {
                    "v1beta1"
                } else {
                    "v2"
                };
                let url = format!("wss://stream.data.alpaca.markets/{version}/{feed}");
                let (socket, _) =
                    tokio::time::timeout(StdDuration::from_secs(15), connect_async(url))
                        .await
                        .map_err(|_| "Market connection timed out")?
                        .map_err(|_| "Market connection failed")?;
                consume(
                    socket,
                    credentials,
                    &feed,
                    &symbols,
                    &state,
                    &events,
                    &epoch,
                )
                .await
            };
            let result = tokio::select! {
                _ = cancellation.changed() => Ok(()),
                result = work => result,
            };
            epoch.fetch_add(1, Ordering::SeqCst);
            let mut current = state.lock().unwrap();
            current.finished = true;
            current.subscribed.clear();
            current.quotes.clear();
            current.bars.clear();
            current.error = result.err();
            let message = current
                .error
                .clone()
                .unwrap_or_else(|| "Market data paused".into());
            drop(current);
            let _ = events.send(vec![json!({"T":"error", "msg":message})]);
        });
        Ok(handle)
    }
}

fn subscription(symbols: &[String], feed: &str) -> Value {
    if feed == "overnight" {
        json!({"action":"subscribe", "quotes":symbols})
    } else {
        json!({"action":"subscribe", "trades":symbols, "bars":symbols, "updatedBars":symbols})
    }
}

async fn consume<IO>(
    mut socket: tokio_tungstenite::WebSocketStream<IO>,
    credentials: AlpacaCredentials,
    feed: &str,
    symbols: &[String],
    state: &Mutex<FeedState>,
    events: &broadcast::Sender<Vec<Value>>,
    epoch: &AtomicU64,
) -> Result<(), String>
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(
            json!({"action": "auth", "key": credentials.key_id, "secret": credentials.secret_key})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| "Authentication failed")?;
    drop(credentials);
    let mut ready = false;
    let mut subscribed = false;
    let deadline = tokio::time::sleep(StdDuration::from_secs(15));
    tokio::pin!(deadline);
    let mut flush = tokio::time::interval(StdDuration::from_millis(250));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat = tokio::time::interval(StdDuration::from_secs(30));
    let mut last_message = Instant::now();
    let mut pending = BTreeMap::<String, Value>::new();
    let mut requested = symbols.to_vec();
    loop {
        let message = tokio::select! {
            _ = &mut deadline, if !subscribed => return Err("Market subscription timed out".into()),
            _ = flush.tick() => {
                if !pending.is_empty() { let _ = events.send(std::mem::take(&mut pending).into_values().collect()); }
                continue;
            }
            _ = heartbeat.tick() => {
                if last_message.elapsed() > StdDuration::from_secs(90) { return Err("Market heartbeat timed out".into()); }
                socket.send(Message::Ping(Vec::new().into())).await.map_err(|_| "Market disconnected")?;
                continue;
            }
            message = socket.next() => message.ok_or("Market disconnected")?.map_err(|_| "Market disconnected")?,
        };
        last_message = Instant::now();
        match message {
            Message::Text(text) => {
                let mut durable = Vec::new();
                for item in parse_stream_payload(&text)? {
                    match item["T"].as_str() {
                        Some("success") if item["msg"] == "authenticated" => {
                            ready = true;
                            socket
                                .send(Message::Text(
                                    subscription(&requested, feed).to_string().into(),
                                ))
                                .await
                                .map_err(|_| "Subscription failed")?;
                        }
                        Some("error") if item["code"] == 405 && requested.len() > 30 => {
                            // Basic's documented ceiling. Overflow remains on REST; never rotate symbols.
                            requested.truncate(30);
                            socket
                                .send(Message::Text(
                                    subscription(&requested, feed).to_string().into(),
                                ))
                                .await
                                .map_err(|_| "Subscription failed")?;
                        }
                        Some("error") => return Err(stream_error(&item)),
                        Some("subscription") if ready => {
                            let channel = if feed == "overnight" {
                                "quotes"
                            } else {
                                "trades"
                            };
                            let accepted: BTreeSet<String> = item[channel]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .filter(|s| symbols.iter().any(|v| v == s))
                                .map(str::to_owned)
                                .collect();
                            subscribed = !accepted.is_empty();
                            state.lock().unwrap().subscribed = accepted;
                            durable.push(item);
                        }
                        Some("q" | "t") => {
                            let Some(symbol) = listed(&item, &requested) else {
                                continue;
                            };
                            let Some(time) = stamp(&item) else {
                                continue;
                            };
                            if !stream_price(&item, feed).is_some_and(|p| p.is_finite() && p > 0.0)
                                || time > Utc::now() + Duration::seconds(5)
                            {
                                continue;
                            }
                            if state.lock().unwrap().observe(symbol, &item, time) {
                                pending.insert(symbol.into(), item);
                            }
                        }
                        Some("c" | "x") if listed(&item, &requested).is_some() => {
                            let symbol = item["S"].as_str().unwrap_or_default();
                            epoch.fetch_add(1, Ordering::SeqCst);
                            pending.remove(symbol);
                            state.lock().unwrap().forget(symbol);
                            durable.push(item);
                        }
                        Some("b" | "u") if listed(&item, &requested).is_some() => {
                            if let (Some(symbol), Some(time)) = (item["S"].as_str(), stamp(&item)) {
                                if state.lock().unwrap().record_bar(symbol, &item, time) {
                                    epoch.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                            durable.push(item);
                        }
                        _ => {}
                    }
                }
                if !durable.is_empty() {
                    let _ = events.send(durable);
                }
            }
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .await
                .map_err(|_| "Market disconnected")?,
            Message::Close(_) => return Err("Market disconnected".into()),
            _ => {}
        }
    }
}

#[cfg(test)]
impl Feed {
    pub(super) fn test() -> Self {
        let (events, _) = broadcast::channel(256);
        let (cancel, _) = watch::channel(false);
        Self {
            state: Arc::new(Mutex::new(FeedState::default())),
            events,
            cancel,
        }
    }
}

// Dropping the previous socket future releases this lease before its replacement starts.
pub(crate) async fn socket_lease<'a>(
    socket: &'a tokio::sync::Mutex<()>,
    mut cancellation: tokio::sync::watch::Receiver<bool>,
) -> Option<tokio::sync::MutexGuard<'a, ()>> {
    if *cancellation.borrow() {
        return None;
    }
    let lease = tokio::select! {
        _ = cancellation.changed() => return None,
        lease = socket.lock() => lease,
    };
    if *cancellation.borrow() {
        None
    } else {
        Some(lease)
    }
}

/// A chart's prices over time: each symbol's previous close, then the bars that arrived, by
/// minute. Kept apart from the latest trades and quotes, which only reprice the portfolio.
pub(super) struct ChartFrames {
    seed: BTreeMap<String, MarketSnapshot>,
    frames: BTreeMap<String, BTreeMap<String, MarketSnapshot>>,
}

impl ChartFrames {
    pub(super) fn new(seed: BTreeMap<String, MarketSnapshot>) -> Self {
        Self {
            seed,
            frames: BTreeMap::new(),
        }
    }

    /// Every symbol's latest price at `as_of`.
    fn at(&self, as_of: &str) -> BTreeMap<String, MarketSnapshot> {
        chart_snapshots_at(as_of, &self.seed, &self.frames)
    }
}

/// How long a stream runs, and how often it publishes what changed.
pub(super) struct StreamPacing {
    /// The next market transition, when the feed changes.
    pub(super) until: DateTime<Utc>,
    pub(super) every: StdDuration,
}

pub(super) async fn consume_market_stream<F>(
    feed: &market_stream::Feed,
    symbols: &[String],
    pacing: &StreamPacing,
    market: &mut MarketSnapshots,
    chart: &mut ChartFrames,
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
    let wait = (pacing.until - Utc::now())
        .to_std()
        .unwrap_or_else(|_| StdDuration::from_secs(0));
    let transition_sleep = tokio::time::sleep(wait);
    tokio::pin!(transition_sleep);
    let mut flush = tokio::time::interval(pacing.every.max(StdDuration::from_secs(1)));
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
                    chart,
                    &mut market_changed,
                    &mut chart_times,
                    publish,
                )?;
                return Ok(());
            }
            _ = flush.tick() => {
                publish_pending_market_updates(
                    market,
                    chart,
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
                                            .or_else(|| chart.seed.get(symbol));
                                        if let Some(snapshot) = stream_snapshot(base, symbol, price, as_of) {
                                            chart
                                                .frames
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

pub(super) fn publish_market_backfill<F>(
    market: &MarketSnapshots,
    backfill: Vec<MarketFrame>,
    chart: &mut ChartFrames,
    publish: &mut F,
) -> Result<(), String>
where
    F: FnMut(MarketStreamTick) -> Result<(), String>,
{
    let mut changed = Vec::new();
    for frame in backfill {
        if chart.frames.get(&frame.as_of) != Some(&frame.snapshots) {
            changed.push(frame.as_of.clone());
        }
        chart.frames.insert(frame.as_of, frame.snapshots);
    }
    let last = changed.len().saturating_sub(1);
    for (index, as_of) in changed.into_iter().enumerate() {
        publish(MarketStreamTick {
            market: market.clone(),
            chart_snapshots: Some(chart.at(&as_of)),
            chart_as_of: Some(as_of),
            latest: index == last,
        })?;
    }
    Ok(())
}

pub(super) fn publish_pending_market_updates<F>(
    market: &MarketSnapshots,
    chart: &ChartFrames,
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
            latest: true,
        })?;
    } else {
        let times = std::mem::take(chart_times);
        let last = times.len().saturating_sub(1);
        for (index, as_of) in times.into_iter().enumerate() {
            publish(MarketStreamTick {
                market: market.clone(),
                chart_snapshots: Some(chart.at(&as_of)),
                chart_as_of: Some(as_of),
                latest: index == last,
            })?;
        }
    }
    *market_changed = false;
    Ok(())
}

pub(super) fn parse_stream_payload(text: &str) -> Result<Vec<Value>, String> {
    let payload: Value = serde_json::from_str(text)
        .map_err(|error| format!("Alpaca WebSocket sent invalid data: {error}"))?;
    match payload {
        Value::Array(items) => Ok(items),
        item @ Value::Object(_) => Ok(vec![item]),
        _ => Err("Alpaca WebSocket sent an unexpected message".into()),
    }
}

pub(super) fn stream_error(value: &Value) -> String {
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

pub(super) fn update_stream_price(
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

pub(super) fn stream_snapshot(
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

pub(super) fn newest_market_time(snapshots: &BTreeMap<String, MarketSnapshot>) -> Option<String> {
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

pub(super) fn previous_close_snapshots(
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

pub(super) fn chart_snapshots_at(
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn one_subscription_serves_multiple_consumers_and_preserves_corrections() {
        let (client, server) = tokio::io::duplex(16384);
        let client = tokio_tungstenite::WebSocketStream::from_raw_socket(
            client,
            tokio_tungstenite::tungstenite::protocol::Role::Client,
            None,
        )
        .await;
        let mut server = tokio_tungstenite::WebSocketStream::from_raw_socket(
            server,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;
        let feed = Feed::test();
        let mut chart = feed.events.subscribe();
        let mut portfolio = feed.events.subscribe();
        let epoch = AtomicU64::new(0);
        let symbols = vec!["TEST".into(), "OTHER".into()];
        let consumer = consume(
            client,
            AlpacaCredentials {
                key_id: "test".into(),
                secret_key: "test".into(),
            },
            "sip",
            &symbols,
            &feed.state,
            &feed.events,
            &epoch,
        );
        let driver = async {
            loop {
                if let Message::Text(text) = server.next().await.unwrap().unwrap() {
                    assert_eq!(
                        serde_json::from_str::<Value>(&text).unwrap()["action"],
                        "auth"
                    );
                    break;
                }
            }
            server
                .send(Message::Text(
                    json!([{"T":"success", "msg":"authenticated"}])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            loop {
                if let Message::Text(text) = server.next().await.unwrap().unwrap() {
                    let request: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(request["trades"], json!(["TEST", "OTHER"]));
                    assert!(request.get("quotes").is_none());
                    break;
                }
            }
            server
                .send(Message::Text(
                    json!([
                        {"T":"subscription", "trades":["TEST","OTHER"]},
                        {"T":"b", "S":"TEST", "c":100, "t":"2026-09-17T14:00:00Z"},
                        {"T":"u", "S":"TEST", "c":101, "t":"2026-09-17T13:59:00Z"},
                        {"T":"x", "S":"TEST"},
                        {"T":"t", "S":"TEST", "p":104, "t":"2026-09-17T14:00:02Z"},
                        {"T":"t", "S":"TEST", "p":103, "t":"2026-09-17T14:00:01Z"},
                        {"T":"t", "S":"OUTSIDE", "p":999, "t":"2026-09-17T14:00:03Z"}
                    ])
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let revisions = chart.recv().await.unwrap();
            assert_eq!(revisions, portfolio.recv().await.unwrap());
            assert_eq!(
                revisions
                    .iter()
                    .map(|v| v["T"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["subscription", "b", "u", "x"]
            );
            let quotes = chart.recv().await.unwrap();
            assert_eq!(quotes, portfolio.recv().await.unwrap());
            assert_eq!(quotes.len(), 1);
            assert_eq!(quotes[0]["p"], 104);
            assert_eq!(epoch.load(Ordering::SeqCst), 2);
            assert_eq!(feed.state.lock().unwrap().quotes.len(), 1);
            server.close(None).await.unwrap();
        };
        tokio::time::timeout(StdDuration::from_secs(5), async {
            let (_, result) = tokio::join!(driver, consumer);
            assert!(result.is_err());
        })
        .await
        .unwrap();
    }
    #[test]
    fn registry_reuses_feed_and_stopped_app_rejects_chart_reconnection() {
        let streams = MarketStreams::default();
        streams.start(vec!["TEST".into()]);
        let feed = Arc::new(Feed::test());
        let mut cancel = feed.cancel.subscribe();
        streams
            .inner
            .lock()
            .unwrap()
            .feeds
            .insert("sip".into(), feed.clone());
        let credentials = || AlpacaCredentials {
            key_id: "test".into(),
            secret_key: "test".into(),
        };
        let chart = streams.acquire("sip", credentials()).unwrap();
        let portfolio = streams.acquire("sip", credentials()).unwrap();
        assert!(Arc::ptr_eq(&chart, &portfolio));
        drop(chart);
        assert!(!*cancel.borrow_and_update());
        streams.stop();
        assert!(*cancel.borrow());
        assert!(streams.acquire("sip", credentials()).is_err());
    }

    #[tokio::test]
    async fn symbol_limit_keeps_stable_subset_and_leaves_overflow_unsubscribed() {
        let (client, server) = tokio::io::duplex(16384);
        let client = tokio_tungstenite::WebSocketStream::from_raw_socket(
            client,
            tokio_tungstenite::tungstenite::protocol::Role::Client,
            None,
        )
        .await;
        let mut server = tokio_tungstenite::WebSocketStream::from_raw_socket(
            server,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;
        let feed = Feed::test();
        let mut events = feed.events.subscribe();
        let symbols: Vec<String> = (0..32).map(|i| format!("TEST{i:02}")).collect();
        let epoch = AtomicU64::new(0);
        let consumer = consume(
            client,
            AlpacaCredentials {
                key_id: "test".into(),
                secret_key: "test".into(),
            },
            "sip",
            &symbols,
            &feed.state,
            &feed.events,
            &epoch,
        );
        let driver = async {
            loop {
                if let Message::Text(_) = server.next().await.unwrap().unwrap() {
                    break;
                }
            }
            server
                .send(Message::Text(
                    json!([{"T":"success", "msg":"authenticated"}])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            loop {
                if let Message::Text(text) = server.next().await.unwrap().unwrap() {
                    assert_eq!(
                        serde_json::from_str::<Value>(&text).unwrap()["trades"]
                            .as_array()
                            .unwrap()
                            .len(),
                        32
                    );
                    break;
                }
            }
            server
                .send(Message::Text(
                    json!([{"T":"error", "code":405}]).to_string().into(),
                ))
                .await
                .unwrap();
            loop {
                if let Message::Text(text) = server.next().await.unwrap().unwrap() {
                    assert_eq!(
                        serde_json::from_str::<Value>(&text).unwrap()["trades"],
                        json!(&symbols[..30])
                    );
                    break;
                }
            }
            server
                .send(Message::Text(
                    json!([{"T":"subscription", "trades":&symbols[..30]}])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            events.recv().await.unwrap();
            assert_eq!(feed.state.lock().unwrap().subscribed.len(), 30);
            assert!(!feed.state.lock().unwrap().subscribed.contains("TEST31"));
            server.close(None).await.unwrap();
        };
        tokio::time::timeout(StdDuration::from_secs(5), async {
            let (_, result) = tokio::join!(driver, consumer);
            assert!(result.is_err());
        })
        .await
        .unwrap();
    }

    fn at(time: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(time).unwrap()
    }

    #[test]
    fn feed_state_keeps_prices_monotonic_and_flags_bars_that_need_repair() {
        let mut state = FeedState::default();
        let newer = json!({"T": "t", "S": "TEST", "p": 10.0, "t": "2026-09-18T14:00:02Z"});
        let older = json!({"T": "t", "S": "TEST", "p": 9.0, "t": "2026-09-18T14:00:01Z"});
        assert!(state.observe("TEST", &newer, at("2026-09-18T14:00:02Z")));
        assert!(!state.observe("TEST", &older, at("2026-09-18T14:00:01Z")));
        assert_eq!(state.quotes["TEST"]["p"], 10.0);

        let bar = |kind: &str, time: &str| json!({"T": kind, "S": "TEST", "t": time});
        for time in [
            "2026-09-18T14:01:00Z",
            "2026-09-18T14:02:00Z",
            "2026-09-18T14:03:00Z",
        ] {
            assert!(!state.record_bar("TEST", &bar("b", time), at(time)));
        }
        let kept: Vec<_> = state.bars["TEST"]
            .iter()
            .map(|bar| bar["t"].clone())
            .collect();
        assert_eq!(kept, ["2026-09-18T14:02:00Z", "2026-09-18T14:03:00Z"]);
        // An update to a bar older than any kept means the saved history is out of date.
        let late = "2026-09-18T14:00:00Z";
        assert!(state.record_bar("TEST", &bar("u", late), at(late)));
        // An update to a kept bar replaces it in place.
        let kept_time = "2026-09-18T14:03:00Z";
        assert!(!state.record_bar("TEST", &bar("u", kept_time), at(kept_time)));

        state.forget("TEST");
        assert!(state.quotes.is_empty() && state.bars.is_empty());
    }

    #[test]
    fn overnight_quotes_use_the_midpoint_unless_crossed() {
        let trade = json!({"p": 10.0, "bp": 11.0, "ap": 9.0});
        assert_eq!(stream_price(&trade, "sip"), Some(10.0));
        assert_eq!(stream_price(&trade, "overnight"), None);
        let quote = json!({"bp": 9.0, "ap": 11.0});
        assert_eq!(stream_price(&quote, "overnight"), quote_midpoint(&quote));
    }
}
