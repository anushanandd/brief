//! App-owned transports. Consumers select data; they never replace subscriptions.
use super::*;
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
            json!({"action":"auth", "key":credentials.key_id,
        "secret":credentials.secret_key})
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
                            let Some(symbol) = item["S"]
                                .as_str()
                                .filter(|s| requested.iter().any(|v| v == s))
                            else {
                                continue;
                            };
                            let Some(time) = item["t"]
                                .as_str()
                                .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                            else {
                                continue;
                            };
                            let price = if feed == "overnight" {
                                if item["bp"].as_f64() > item["ap"].as_f64() {
                                    None
                                } else {
                                    quote_midpoint(&item)
                                }
                            } else {
                                item["p"].as_f64()
                            };
                            if !price.is_some_and(|p| p.is_finite() && p > 0.0)
                                || time > Utc::now() + Duration::seconds(5)
                            {
                                continue;
                            }
                            let mut current = state.lock().unwrap();
                            let previous = current
                                .quotes
                                .get(symbol)
                                .and_then(|v| v["t"].as_str())
                                .and_then(|t| DateTime::parse_from_rfc3339(t).ok());
                            if previous.is_none_or(|old| time >= old) {
                                current.quotes.insert(symbol.into(), item.clone());
                                pending.insert(symbol.into(), item);
                            }
                        }
                        Some("b" | "u" | "c" | "x")
                            if item["S"]
                                .as_str()
                                .is_some_and(|s| requested.iter().any(|v| v == s)) =>
                        {
                            if matches!(item["T"].as_str(), Some("c" | "x")) {
                                let symbol = item["S"].as_str().unwrap();
                                epoch.fetch_add(1, Ordering::SeqCst);
                                pending.remove(symbol);
                                let mut current = state.lock().unwrap();
                                current.quotes.remove(symbol);
                                current.bars.remove(symbol);
                            }
                            if matches!(item["T"].as_str(), Some("b" | "u")) {
                                if let (Some(symbol), Some(time)) = (
                                    item["S"].as_str(),
                                    item["t"]
                                        .as_str()
                                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok()),
                                ) {
                                    let mut current = state.lock().unwrap();
                                    let bars = current.bars.entry(symbol.into()).or_default();
                                    bars.retain(|bar| bar["t"] != item["t"]);
                                    if item["T"] == "u"
                                        && bars
                                            .iter()
                                            .filter_map(|bar| {
                                                bar["t"].as_str().and_then(|t| {
                                                    DateTime::parse_from_rfc3339(t).ok()
                                                })
                                            })
                                            .min()
                                            .is_some_and(|oldest| time < oldest)
                                    {
                                        epoch.fetch_add(1, Ordering::SeqCst);
                                    }
                                    bars.push(item.clone());
                                    bars.sort_by_key(|bar| {
                                        bar["t"]
                                            .as_str()
                                            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                                    });
                                    if bars.len() > 2 {
                                        bars.remove(0);
                                    }
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
