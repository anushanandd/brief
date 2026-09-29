//! Security charts are independent of portfolio valuation. REST bars are canonical;
//! live trades/indicative quotes are separate observations, never historical closes.
use super::*;
use chrono::Timelike;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct ChartSelection {
    pub symbol: String,
    pub range: u64,
    pub request_id: String,
    pub warm_symbols: Vec<String>,
}

impl ChartSelection {
    pub fn tag(&self, mut payload: Value) -> Value {
        payload["symbol"] = self.symbol.clone().into();
        payload["requestId"] = self.request_id.clone().into();
        payload
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceBar {
    pub time: i64,
    pub end_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub trades: u64,
    pub feed: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct TradingDay {
    pub date: String,
    pub open: i64,
    pub close: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceHistory {
    pub symbol: String,
    pub range: u64,
    pub resolution: u64,
    pub start: i64,
    pub end: i64,
    pub fetched_at: i64,
    #[serde(default)]
    pub full_verified_at: i64,
    #[serde(default)]
    pub calendar: Option<Vec<TradingDay>>,
    pub bars: Vec<PriceBar>,
    pub feeds: Vec<String>,
    pub delay_minutes: u8,
    pub adjustment: String,
    pub cached: bool,
}

// Only daily all-time history reuses a prefix. A full verification expires at
// the UTC date boundary (or 24 hours), bounding corrections outside the overlap.
fn incremental_start(
    previous: &PriceHistory,
    symbol: &str,
    realtime: bool,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let verified = DateTime::from_timestamp_millis(previous.full_verified_at)?;
    if previous.symbol != symbol
        || previous.range != 0
        || previous.resolution != 86400
        || previous.adjustment != "split"
        || previous.feeds != ["sip"]
        || previous.delay_minutes != if realtime { 0 } else { 15 }
        || previous.full_verified_at > previous.fetched_at
        || verified > now
        || now - verified >= Duration::hours(24)
        || verified.date_naive() != now.date_naive()
        || previous.end > now.timestamp()
        || previous.bars.is_empty()
    {
        return None;
    }
    Some(now - Duration::days(30))
}

fn reconcile_history(previous: &PriceHistory, mut recent: PriceHistory) -> Option<PriceHistory> {
    // Compare the complete overlapping slice, including OHLCV and calendar
    // close times. Splits, corrections, deletions, and new older bars trigger
    // full replacement rather than mixing incompatible adjustment histories.
    if previous.symbol != recent.symbol
        || previous.range != recent.range
        || previous.resolution != recent.resolution
        || previous.feeds != recent.feeds
        || previous.adjustment != recent.adjustment
        || previous.delay_minutes != recent.delay_minutes
        || recent.start < previous.start
        || recent.end < previous.end
    {
        return None;
    }
    let old: Vec<_> = previous
        .bars
        .iter()
        .filter(|bar| bar.time >= recent.start)
        .collect();
    let new: Vec<_> = recent
        .bars
        .iter()
        .filter(|bar| bar.end_time <= previous.end)
        .collect();
    if old.is_empty() || old != new {
        return None;
    }
    if let (Some(old_days), Some(new_days)) = (&previous.calendar, &mut recent.calendar) {
        let cutoff = DateTime::from_timestamp(recent.start, 0)?
            .format("%Y-%m-%d")
            .to_string();
        let end_date = (DateTime::from_timestamp_millis(previous.fetched_at)? + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        if old_days
            .iter()
            .filter(|day| day.date >= cutoff)
            .collect::<Vec<_>>()
            != new_days
                .iter()
                .filter(|day| day.date <= end_date)
                .collect::<Vec<_>>()
        {
            return None;
        }
        let mut days: Vec<_> = old_days
            .iter()
            .filter(|day| day.date < cutoff)
            .cloned()
            .collect();
        days.append(new_days);
        *new_days = days;
    } else if previous.calendar.is_some() != recent.calendar.is_some() {
        return None;
    }
    let mut bars: Vec<_> = previous
        .bars
        .iter()
        .filter(|bar| bar.time < recent.start)
        .cloned()
        .collect();
    bars.append(&mut recent.bars);
    recent.bars = bars;
    recent.start = previous.start;
    recent.full_verified_at = previous.full_verified_at;
    Some(recent)
}

pub fn resolution(range: u64) -> Result<(&'static str, u64), String> {
    match range {
        86400 => Ok(("1Min", 60)),
        604800 => Ok(("5Min", 300)),
        2592000 => Ok(("30Min", 1800)),
        31536000 | 0 => Ok(("1Day", 86400)),
        _ => Err("Unsupported security chart range".into()),
    }
}

fn history_refresh_delay(
    seconds: u64,
    schedule: &AlpacaMarketSchedule,
    now: DateTime<Utc>,
    closed_since: Option<DateTime<Utc>>,
) -> StdDuration {
    let until_transition = (schedule.next_transition_at - now).num_seconds().max(1) as u64 + 1;
    let cadence = if schedule.session == AlpacaMarketSession::Closed {
        // One final reconciliation lets delayed tails and late corrections settle.
        let remaining = closed_since
            .map(|since| (since + Duration::minutes(20) - now).num_seconds())
            .unwrap_or(1200);
        if remaining > 0 {
            remaining as u64
        } else {
            until_transition
        }
    } else {
        seconds.min(3600)
    };
    StdDuration::from_secs(cadence.min(until_transition))
}

fn history_feeds(seconds: u64) -> Vec<&'static str> {
    if seconds < 86400 {
        vec!["sip", "boats"]
    } else {
        vec!["sip"]
    }
}

pub(super) fn valuation_feed(session: AlpacaMarketSession, sip: bool, boats: bool) -> &'static str {
    match session {
        AlpacaMarketSession::Overnight if boats => "boats",
        AlpacaMarketSession::Overnight => "overnight",
        _ if sip => "sip",
        _ => session.feed(),
    }
}

fn stream_feed(session: AlpacaMarketSession, sip: bool, boats: bool) -> &'static str {
    match (session == AlpacaMarketSession::Overnight, sip, boats) {
        (true, _, true) => "boats",
        (true, _, false) => "overnight",
        (false, true, _) => "sip",
        (false, false, _) => "delayed_sip",
    }
}

fn parse_closes(payload: &Value) -> Result<BTreeMap<String, i64>, String> {
    payload
        .as_array()
        .ok_or("Invalid market calendar")?
        .iter()
        .map(|day| {
            let date = day["date"].as_str().ok_or("Missing market date")?;
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| "Invalid market date")?;
            let close = chrono::NaiveTime::parse_from_str(
                day["close"].as_str().ok_or("Missing market close")?,
                "%H:%M",
            )
            .map_err(|_| "Invalid market close")?;
            Ok((
                date.to_string(),
                i64::from(close.num_seconds_from_midnight()),
            ))
        })
        .collect()
}

fn parse_calendar(payload: &Value) -> Result<Vec<TradingDay>, String> {
    let closes = parse_closes(payload)?;
    let mut days = Vec::new();
    for day in payload.as_array().ok_or("Invalid market calendar")? {
        let date = day["date"].as_str().ok_or("Missing market date")?;
        let open = chrono::NaiveTime::parse_from_str(
            day["open"].as_str().ok_or("Missing market open")?,
            "%H:%M",
        )
        .map_err(|_| "Invalid market open")?
        .num_seconds_from_midnight() as i64;
        let close = closes[date];
        if open < 4 * 3600
            || close <= open
            || close > 20 * 3600
            || days
                .last()
                .is_some_and(|previous: &TradingDay| previous.date.as_str() >= date)
        {
            return Err("Invalid market session bounds".into());
        }
        days.push(TradingDay {
            date: date.into(),
            open,
            close,
        });
    }
    Ok(days)
}

fn bar_end(bar: &PriceBar, seconds: u64, closes: &BTreeMap<String, i64>) -> Result<i64, String> {
    if seconds < 86400 {
        return Ok(bar.time + seconds as i64);
    }
    let date = DateTime::from_timestamp(bar.time, 0)
        .ok_or("Invalid daily bar date")?
        .format("%Y-%m-%d")
        .to_string();
    let close = closes
        .get(&date)
        .ok_or("Daily bar has no matching trading calendar date")?;
    Ok(bar.time + close)
}

fn parse_bar(value: &Value, feed: &str) -> Result<PriceBar, String> {
    let number = |key: &str| {
        value[key]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or_else(|| "Invalid security price bar".to_string())
    };
    let bar = PriceBar {
        time: value["t"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .ok_or("Invalid security bar timestamp")?
            .timestamp(),
        end_time: 0,
        open: number("o")?,
        high: number("h")?,
        low: number("l")?,
        close: number("c")?,
        volume: value["v"]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("Invalid security bar volume")?,
        trades: value["n"]
            .as_u64()
            .ok_or("Invalid security bar trade count")?,
        feed: feed.into(),
    };
    if bar.low > bar.open.min(bar.close) || bar.high < bar.open.max(bar.close) {
        return Err("Inconsistent security price bar".into());
    }
    Ok(bar)
}

// A replace-only, bounded cache avoids mixing adjustment epochs or retaining a
// cancelled trade. Failed/incomplete downloads never replace the prior result.
fn cache_connection(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|_| "Price cache unavailable")?;
    crate::database::secure_file(path)?;
    db.busy_timeout(StdDuration::from_secs(2))
        .map_err(|_| "Price cache unavailable")?;
    db.execute_batch(
        "PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS price_history (
            symbol TEXT NOT NULL, range_seconds INTEGER NOT NULL, fetched_at INTEGER NOT NULL,
            payload TEXT NOT NULL CHECK(json_valid(payload)),
            PRIMARY KEY(symbol, range_seconds)
        ) STRICT;",
    )
    .map_err(|_| "Price cache unavailable")?;
    Ok(db)
}

pub fn cached_history(path: &Path, symbol: &str, range: u64) -> Option<PriceHistory> {
    let db = cache_connection(path).ok()?;
    let payload: String = db
        .query_row(
            "SELECT payload FROM price_history WHERE symbol=?1 AND range_seconds=?2",
            params![symbol, range],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    let mut history: PriceHistory = serde_json::from_str(&payload).ok()?;
    if history.symbol != symbol
        || history.range != range
        || history.adjustment != "split"
        || history.resolution != resolution(range).ok()?.1
        || history.feeds != history_feeds(history.resolution)
        || history.start >= history.end
        || history.calendar.as_ref().is_some_and(|days| {
            days.windows(2).any(|pair| pair[0].date >= pair[1].date)
                || days.iter().any(|day| {
                    chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").is_err()
                        || day.open < 14400
                        || day.close <= day.open
                        || day.close > 72000
                })
        })
        || history
            .bars
            .windows(2)
            .any(|w| (w[0].time, &w[0].feed) >= (w[1].time, &w[1].feed))
        || history.bars.iter().any(|b| {
            b.time < history.start
                || b.end_time > history.end
                || b.end_time <= b.time
                || !history.feeds.contains(&b.feed)
                || !b.close.is_finite()
                || b.close <= 0.0
                || !b.open.is_finite()
                || !b.high.is_finite()
                || !b.low.is_finite()
                || b.low <= 0.0
                || b.low > b.open.min(b.close)
                || b.high < b.open.max(b.close)
                || !b.volume.is_finite()
                || b.volume < 0.0
        })
    {
        return None;
    }
    history.cached = true;
    Some(history)
}

fn save_history(path: &Path, history: &PriceHistory) -> Result<(), String> {
    let mut db = cache_connection(path)?;
    let tx = db.transaction().map_err(|_| "Price cache unavailable")?;
    let payload = serde_json::to_string(history).map_err(|_| "Invalid price history")?;
    tx.execute(
        "INSERT OR REPLACE INTO price_history VALUES (?1, ?2, ?3, ?4)",
        params![history.symbol, history.range, history.fetched_at, payload],
    )
    .map_err(|_| "Could not save price history")?;
    tx.execute(
        "DELETE FROM price_history WHERE rowid NOT IN
        (SELECT rowid FROM price_history ORDER BY fetched_at DESC LIMIT 160)",
        [],
    )
    .map_err(|_| "Could not bound price cache")?;
    tx.commit()
        .map_err(|_| "Could not commit price history".into())
}

impl Providers {
    fn fresh_chart(&self, symbol: &str, range: u64) -> Option<PriceHistory> {
        let epoch = self.market_streams.epoch.load(Ordering::SeqCst);
        let previews = self.chart_previews.lock().ok()?;
        let (saved_epoch, history) = previews.get(&(symbol.into(), range))?;
        let ttl = match range {
            86400 => 60_000,
            604800 => 300_000,
            2592000 => 1_800_000,
            _ => 3_600_000,
        };
        let age = Utc::now().timestamp_millis() - history.fetched_at;
        (*saved_epoch == epoch
            && age >= 0
            && age < ttl
            && history.fetched_at / 86_400_000 == Utc::now().timestamp_millis() / 86_400_000)
            .then(|| history.clone())
    }
    fn remember_chart(&self, history: &PriceHistory, epoch: u64) {
        if epoch != self.market_streams.epoch.load(Ordering::SeqCst) {
            return;
        }
        let mut previews = self.chart_previews.lock().unwrap();
        previews.insert(
            (history.symbol.clone(), history.range),
            (epoch, history.clone()),
        );
        while previews.len() > 160 {
            let oldest = previews
                .iter()
                .min_by_key(|(_, (_, history))| history.fetched_at)
                .map(|(key, _)| key.clone())
                .unwrap();
            previews.remove(&oldest);
        }
    }

    pub(super) async fn market_access(&self, symbol: &str) -> Result<(bool, bool), String> {
        let generation = self.credential_generation.load(Ordering::SeqCst);
        if let Some((saved_generation, checked, sip, boats)) = *self
            .market_access
            .lock()
            .map_err(|_| "Market access unavailable")?
        {
            if saved_generation == generation && checked.elapsed() < StdDuration::from_secs(300) {
                return Ok((sip, boats));
            }
        }
        let credentials = self.alpaca_credentials()?;
        let (sip, boats) = tokio::try_join!(
            self.holding_feed_realtime(&credentials, symbol, "sip"),
            self.holding_feed_realtime(&credentials, symbol, "boats"),
        )?;
        *self
            .market_access
            .lock()
            .map_err(|_| "Market access unavailable")? =
            Some((generation, Instant::now(), sip, boats));
        Ok((sip, boats))
    }
    pub(super) async fn canonical_stream_feed(&self, symbol: &str) -> Result<String, String> {
        let (sip, boats) = self.market_access(symbol).await?;
        let schedule = self
            .market_schedule(&self.alpaca_credentials()?, Utc::now())
            .await?;
        Ok(stream_feed(schedule.session, sip, boats).into())
    }

    async fn holding_calendar(
        &self,
        credentials: &AlpacaCredentials,
        start: &str,
        end: &str,
    ) -> Result<Vec<TradingDay>, String> {
        for base in [
            "https://paper-api.alpaca.markets",
            "https://api.alpaca.markets",
        ] {
            let response = self
                .http
                .get(format!("{base}/v2/calendar"))
                .header("APCA-API-KEY-ID", &credentials.key_id)
                .header("APCA-API-SECRET-KEY", &credentials.secret_key)
                .query(&[("start", start), ("end", end)])
                .send()
                .await;
            if let Ok(response) = response {
                if let Ok(payload) =
                    provider_response("Market calendar", "/v2/calendar", response).await
                {
                    return parse_calendar(&payload);
                }
            }
        }
        Err("Market calendar unavailable".into())
    }
    // Probe actual permissions, not the presence of credentials or the user's plan
    // name. Only a forbidden response means delayed access; outages remain errors.
    async fn holding_feed_realtime(
        &self,
        credentials: &AlpacaCredentials,
        symbol: &str,
        feed: &str,
    ) -> Result<bool, String> {
        let end = Utc::now() - Duration::seconds(2);
        let start = (end - Duration::minutes(1)).to_rfc3339();
        let end = end.to_rfc3339();
        let response = self
            .http
            .get(format!(
                "https://data.alpaca.markets/v2/stocks/{symbol}/bars"
            ))
            .header("APCA-API-KEY-ID", &credentials.key_id)
            .header("APCA-API-SECRET-KEY", &credentials.secret_key)
            .query(&[
                ("feed", feed),
                ("timeframe", "1Min"),
                ("start", start.as_str()),
                ("end", end.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(|_| "Could not verify market-data access")?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Ok(false);
        }
        if !response.status().is_success() {
            return Err("Could not verify market-data access".into());
        }
        Ok(true)
    }

    async fn chart_history(
        &self,
        credentials: &AlpacaCredentials,
        symbol: &str,
        range: u64,
        realtime: bool,
        previous: Option<&PriceHistory>,
    ) -> Result<PriceHistory, String> {
        let _permit = self
            .chart_requests
            .acquire()
            .await
            .map_err(|_| "History requests stopped")?;
        let (_, seconds) = resolution(range)?;
        let now = Utc::now();
        let start = if range == 0 {
            DateTime::parse_from_rfc3339("2016-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        } else {
            // Include observed reference bars before weekend/holiday boundaries.
            // Seven days also bounds the renderer's permitted reference age.
            now - Duration::seconds(range as i64 + seconds as i64) - Duration::days(7)
        };
        if range == 0 {
            if let Some((previous, overlap)) = previous.and_then(|previous| {
                incremental_start(previous, symbol, realtime, now).map(|start| (previous, start))
            }) {
                let recent = self
                    .fetch_chart_history(credentials, symbol, range, realtime, now, overlap)
                    .await?;
                if let Some(merged) = reconcile_history(previous, recent) {
                    return Ok(merged);
                }
            }
        }
        self.fetch_chart_history(credentials, symbol, range, realtime, now, start)
            .await
    }

    async fn fetch_chart_history(
        &self,
        credentials: &AlpacaCredentials,
        symbol: &str,
        range: u64,
        realtime: bool,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
    ) -> Result<PriceHistory, String> {
        let (timeframe, seconds) = resolution(range)?;
        let end = now - Duration::seconds(if realtime { 2 } else { 902 });
        let feeds = history_feeds(seconds);
        let calendar = self
            .holding_calendar(
                credentials,
                &start.format("%Y-%m-%d").to_string(),
                // Include the next trading date for evening overnight sessions.
                &(now + Duration::days(1)).format("%Y-%m-%d").to_string(),
            )
            .await?;
        let closes = calendar
            .iter()
            .map(|day| (day.date.clone(), day.close))
            .collect();
        let mut bars = BTreeMap::new();
        for feed in &feeds {
            let mut token: Option<String> = None;
            let mut seen = BTreeSet::new();
            let start_text = start.to_rfc3339();
            let end_text = end.to_rfc3339();
            for page in 0..20 {
                let mut params = vec![
                    ("timeframe", timeframe),
                    ("start", &start_text),
                    ("end", &end_text),
                    ("limit", "10000"),
                    ("feed", *feed),
                    ("adjustment", "split"),
                    ("sort", "asc"),
                ];
                if let Some(token) = token.as_deref() {
                    params.push(("page_token", token));
                }
                let result = self
                    .alpaca_request(credentials, &format!("/v2/stocks/{symbol}/bars"), &params)
                    .await?;
                let values = match result.get("bars") {
                    Some(Value::Array(values)) => values.as_slice(),
                    Some(Value::Null) => &[],
                    _ => return Err("Price history response omitted bars".into()),
                };
                for value in values {
                    let mut bar = parse_bar(value, feed)?;
                    bar.end_time = bar_end(&bar, seconds, &closes)?;
                    // Completed daily bars use the actual calendar close, including
                    // early closes. Their provider bucket start retains the correct
                    // New York midnight UTC offset on each trading date.
                    if bar.time >= start.timestamp() && bar.end_time <= end.timestamp() {
                        bars.insert((bar.time, bar.feed.clone()), bar);
                    }
                }
                token = result["next_page_token"].as_str().map(str::to_owned);
                let Some(next) = token.as_ref() else {
                    break;
                };
                if !seen.insert(next.clone()) || page == 19 {
                    return Err("Price history pagination is incomplete".into());
                }
            }
        }
        Ok(PriceHistory {
            symbol: symbol.into(),
            range,
            resolution: seconds,
            start: start.timestamp(),
            end: end.timestamp(),
            fetched_at: now.timestamp_millis(),
            full_verified_at: now.timestamp_millis(),
            calendar: Some(calendar),
            bars: bars.into_values().collect(),
            feeds: feeds.into_iter().map(str::to_owned).collect(),
            delay_minutes: if realtime { 0 } else { 15 },
            adjustment: "split".into(),
            cached: false,
        })
    }

    pub async fn run_holding_chart<F>(
        &self,
        selection: tokio::sync::watch::Receiver<ChartSelection>,
        cache: &Path,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        publish: F,
    ) -> Result<(), String>
    where
        F: Fn(Value) + Send + Sync,
    {
        let initial = selection.borrow().clone();
        let symbol = stock_symbol(&initial.symbol)?;
        let credentials = self.alpaca_credentials()?;
        let (sip, boats) = self.market_access(&symbol).await?;
        let (repair, mut repairs) = tokio::sync::watch::channel((0_u64, 0_u64));
        let mut history_cancel = cancel.clone();
        let mut history_selection = selection.clone();
        let histories = async {
            loop {
                let selected = history_selection.borrow_and_update().clone();
                let symbol = selected.symbol.clone();
                let range = selected.range;
                let (_, seconds) = resolution(range).expect("Validated chart range");
                let realtime = sip && (range == 0 || range >= 31536000 || boats);
                let publish = |payload| publish(selected.tag(payload));
                let history = async {
                    let mut closed_since = None;
                    let mut previous = self
                        .fresh_chart(&symbol, range)
                        .or_else(|| cached_history(cache, &symbol, range));
                    if let Some(history) = previous.as_ref() {
                        publish(json!({"kind":"history", "history":history, "startedAt":0}));
                    }
                    let mut previous_epoch = self.market_streams.epoch.load(Ordering::SeqCst);
                    let mut verified_repair = 0;
                    // Start REST without waiting for the socket. Subscription confirmation
                    // requests another reconciliation, and the renderer retains bars received
                    // during a download, closing any gap while the stream connects.
                    loop {
                        if *history_cancel.borrow() {
                            return;
                        }
                        let started_at = Utc::now().timestamp_millis();
                        let full_repair = repairs.borrow().1;
                        let reusable = (full_repair == verified_repair
                            && previous_epoch == self.market_streams.epoch.load(Ordering::SeqCst)
                            && previous
                                .as_ref()
                                .is_some_and(|h| !h.cached || previous_epoch <= 1))
                        .then_some(previous.as_ref())
                        .flatten();
                        let epoch = self.market_streams.epoch.load(Ordering::SeqCst);
                        let fresh = (full_repair == verified_repair)
                            .then(|| self.fresh_chart(&symbol, range))
                            .flatten();
                        let result = if let Some(history) = fresh {
                            Ok(history)
                        } else {
                            tokio::select! {
                                _ = history_cancel.changed() => return,
                                result = self.chart_history(&credentials, &symbol, range, realtime, reusable) => result,
                            }
                        };
                        let succeeded = result.is_ok();
                        match result {
                            Ok(history) => {
                                if epoch != self.market_streams.epoch.load(Ordering::SeqCst) {
                                    previous = None;
                                    continue;
                                }
                                self.remember_chart(&history, epoch);
                                let cache_warning = save_history(cache, &history).err();
                                previous = Some(history.clone());
                                verified_repair = full_repair;
                                previous_epoch = epoch;
                                publish(
                                    json!({"kind":"history", "history":history, "startedAt":started_at.min(history.fetched_at),
                            "warning": cache_warning}),
                                );
                            }
                            Err(_) => publish(json!({"kind":"history-error", "message":
                        "History refresh failed; retaining the last verified chart."})),
                        }
                        let now = Utc::now();
                        let schedule = tokio::select! {
                            _ = history_cancel.changed() => return,
                            result = self.market_schedule(&credentials, now) => result,
                        };
                        let delay = match schedule {
                            Ok(schedule) if succeeded => {
                                if schedule.session == AlpacaMarketSession::Closed {
                                    closed_since.get_or_insert(now);
                                } else {
                                    closed_since = None;
                                }
                                history_refresh_delay(seconds, &schedule, now, closed_since)
                            }
                            _ => StdDuration::from_secs(60),
                        };
                        tokio::select! {
                            _ = history_cancel.changed() => return,
                            _ = tokio::time::sleep(delay) => {},
                            _ = repairs.changed() => {},
                        }
                    }
                };
                tokio::select! {
                    _ = history => return,
                    result = history_selection.changed() => {
                        if result.is_err() { return; }
                    }
                }
            }
        };
        let mut warm_selection = selection.clone();
        let mut warm_cancel = cancel.clone();
        let warming = async {
            loop {
                let selected = warm_selection.borrow_and_update().clone();
                let symbols = self.market_streams.symbols();
                let mut order: Vec<String> = selected
                    .warm_symbols
                    .iter()
                    .filter(|s| symbols.contains(s))
                    .cloned()
                    .collect();
                for symbol in symbols {
                    if !order.contains(&symbol) {
                        order.push(symbol);
                    }
                }
                let index = order
                    .iter()
                    .position(|s| s == &selected.symbol)
                    .unwrap_or(0);
                let mut ordered = Vec::new();
                for distance in 1..order.len() {
                    for offset in [distance, order.len() - distance] {
                        let symbol = &order[(index + offset) % order.len()];
                        if symbol != &selected.symbol && !ordered.contains(symbol) {
                            ordered.push(symbol.clone());
                        }
                    }
                }
                let work = stream::iter(ordered).for_each_concurrent(2, |symbol| {
                    let selected = &selected;
                    let credentials = &credentials;
                    let publish = &publish;
                    async move {
                        let target = ChartSelection { symbol: symbol.clone(), ..selected.clone() };
                        let previous = self.fresh_chart(&symbol, selected.range).or_else(|| cached_history(cache, &symbol, selected.range));
                        if let Some(history) = &previous {
                            publish(target.tag(json!({"kind":"history", "history":history, "startedAt":0})));
                        }
                        if previous.as_ref().is_some_and(|history| !history.cached) { return; }
                        let epoch = self.market_streams.epoch.load(Ordering::SeqCst);
                        let realtime = sip && (selected.range == 0 || selected.range >= 31536000 || boats);
                        let started_at = Utc::now().timestamp_millis();
                        if let Ok(history) = self.chart_history(credentials, &symbol, selected.range, realtime, previous.as_ref().filter(|h| !h.cached || epoch <= 1)).await {
                            if epoch != self.market_streams.epoch.load(Ordering::SeqCst) { return; }
                            let _ = save_history(cache, &history);
                            self.remember_chart(&history, epoch);
                            publish(target.tag(json!({"kind":"history", "history":history, "startedAt":started_at})));
                        }
                    }
                });
                tokio::select! {
                    _ = warm_cancel.changed() => return,
                    changed = warm_selection.changed() => { if changed.is_err() { return; } continue; },
                    _ = work => {},
                }
                tokio::select! {
                    _ = warm_cancel.changed() => return,
                    changed = warm_selection.changed() => { if changed.is_err() { return; } },
                }
            }
        };
        let mut stream_selection = selection.clone();
        let streaming = async {
            loop {
                if *cancel.borrow() {
                    return;
                }
                let schedule = tokio::select! {
                    _ = cancel.changed() => return,
                    result = self.market_schedule(&credentials, Utc::now()) => result,
                };
                let schedule = match schedule {
                    Ok(schedule) => schedule,
                    Err(_) => {
                        publish(stream_selection.borrow().tag(
                            json!({"kind":"status", "status":"reconnecting", "session":"Unknown", "feed":"", "delayMinutes":0}),
                        ));
                        tokio::select! { _ = cancel.changed() => return, _ = tokio::time::sleep(StdDuration::from_secs(15)) => {} }
                        continue;
                    }
                };
                let session = schedule.session;
                if session == AlpacaMarketSession::Closed {
                    publish(stream_selection.borrow_and_update().tag(
                        json!({"kind":"status", "status":"closed", "session":session.label(), "feed":"", "delayMinutes":0}),
                    ));
                    let wait = (schedule.next_transition_at - Utc::now())
                        .num_seconds()
                        .max(1) as u64
                        + 1;
                    tokio::select! {
                        _ = cancel.changed() => return,
                        result = stream_selection.changed() => { if result.is_err() { return; } },
                        _ = tokio::time::sleep(StdDuration::from_secs(wait)) => {}
                    }
                    continue;
                }
                let feed = stream_feed(session, sip, boats);
                let status = |selected: &ChartSelection, state: &str| {
                    publish(selected.tag(json!({"kind":"status", "status":state,
                    "session":session.label(), "feed":feed, "delayMinutes":if feed == "delayed_sip" {15} else {0}})))
                };
                let result = match self
                    .alpaca_credentials()
                    .and_then(|credentials| self.market_streams.acquire(feed, credentials))
                {
                    Ok(shared) => tokio::select! {
                        _ = cancel.changed() => return,
                        result = consume_shared_chart(&shared, stream_selection.clone(), feed, schedule.next_transition_at,
                            &publish, &status, &repair) => result,
                    },
                    Err(error) => Err(error),
                };
                if *cancel.borrow() {
                    return;
                }
                if let Err(error) = result {
                    status(
                        &stream_selection.borrow(),
                        if error == "Stream access unavailable" {
                            "unavailable"
                        } else {
                            "reconnecting"
                        },
                    );
                    tokio::select! { _ = cancel.changed() => return, _ = tokio::time::sleep(StdDuration::from_secs(10)) => {} }
                }
            }
        };
        tokio::join!(histories, streaming, warming);
        Ok(())
    }
}

async fn consume_shared_chart<F, S>(
    shared: &market_stream::Feed,
    mut selection: tokio::sync::watch::Receiver<ChartSelection>,
    feed: &str,
    transition: DateTime<Utc>,
    publish: &F,
    status: &S,
    repair: &tokio::sync::watch::Sender<(u64, u64)>,
) -> Result<(), String>
where
    F: Fn(Value) + Send + Sync,
    S: Fn(&ChartSelection, &str) + Send + Sync,
{
    let mut events = shared.events.subscribe();
    let mut selected = selection.borrow_and_update().clone();
    let show = |selected: &ChartSelection| -> Result<(), String> {
        let current = shared
            .state
            .lock()
            .map_err(|_| "Market state unavailable")?
            .clone();
        if current.finished {
            return Err(current.error.unwrap_or_else(|| "Market paused".into()));
        }
        status(
            selected,
            if current.subscribed.contains(&selected.symbol) {
                "subscribed"
            } else if current.subscribed.is_empty() {
                "connecting"
            } else {
                "rest"
            },
        );
        if let Some(bars) = current.bars.get(&selected.symbol) {
            for bar in bars {
                publish_chart_item(bar, selected, feed, publish, repair)?;
            }
        }
        if let Some(item) = current.quotes.get(&selected.symbol) {
            publish_chart_item(item, selected, feed, publish, repair)?;
        }
        Ok(())
    };
    show(&selected)?;
    repair.send_modify(|r| r.0 += 1);
    let wait = tokio::time::sleep((transition - Utc::now()).to_std().unwrap_or_default());
    tokio::pin!(wait);
    loop {
        tokio::select! {
            biased;
            changed = selection.changed() => {
                if changed.is_err() { return Ok(()); }
                selected = selection.borrow_and_update().clone();
                show(&selected)?;
            }
            _ = &mut wait => return Ok(()),
            items = events.recv() => {
                let items = match items {
                    Ok(items) => items,
                    Err(_) => {
                        publish(selected.tag(json!({"kind":"invalidate", "feed":feed})));
                        repair.send_modify(|r| { r.0 += 1; r.1 += 1; });
                        return Err("Market history needs reconciliation".into());
                    }
                };
                for item in items {
                    match item["T"].as_str() {
                        Some("error") => return Err("Market disconnected".into()),
                        Some("subscription") => { show(&selected)?; repair.send_modify(|r| r.0 += 1); },
                        Some("c" | "x") if item["S"] != selected.symbol => {
                            if let Some(symbol) = item["S"].as_str() {
                                let target = ChartSelection { symbol: symbol.into(), ..selected.clone() };
                                publish(target.tag(json!({"kind":"invalidate", "feed":feed})));
                            }
                        }
                        _ if item["S"] == selected.symbol => publish_chart_item(&item, &selected, feed, publish, repair)?,
                        _ => {},
                    }
                }
            }
        }
    }
}

fn publish_chart_item<F: Fn(Value)>(
    item: &Value,
    selected: &ChartSelection,
    feed: &str,
    publish: &F,
    repair: &tokio::sync::watch::Sender<(u64, u64)>,
) -> Result<(), String> {
    match item["T"].as_str() {
        Some("c" | "x") => {
            publish(selected.tag(json!({"kind":"invalidate", "feed":feed})));
            repair.send_modify(|r| {
                r.0 += 1;
                r.1 += 1;
            });
        }
        Some("b" | "u") if feed != "overnight" => {
            let mut bar = parse_bar(item, if feed == "delayed_sip" { "sip" } else { feed })?;
            bar.end_time = bar.time + 60;
            publish(
                selected.tag(
                    json!({"kind":"bar", "bar":bar, "receivedAt":Utc::now().timestamp_millis()}),
                ),
            );
        }
        Some("q" | "t") => {
            let indicative = feed == "overnight";
            let price = if indicative {
                quote_midpoint(item)
            } else {
                item["p"].as_f64()
            };
            let time = item["t"]
                .as_str()
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok());
            if let (Some(price), Some(time)) = (price.filter(|p| p.is_finite() && *p > 0.0), time) {
                publish(selected.tag(json!({"kind":"quote", "price":price, "time":time.timestamp_millis() as f64 / 1000.0,
                    "feed":feed, "indicative":indicative})));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daily_history() -> PriceHistory {
        let now = DateTime::parse_from_rfc3339("2026-09-18T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let bars = (1..=40)
            .map(|day| {
                let time = (now - Duration::days(41 - day)).timestamp();
                PriceBar {
                    time,
                    end_time: time + 3600,
                    open: 100.0,
                    high: 101.0,
                    low: 99.0,
                    close: 100.0,
                    volume: 1000.0,
                    trades: 100,
                    feed: "sip".into(),
                }
            })
            .collect::<Vec<_>>();
        PriceHistory {
            symbol: "TEST".into(),
            range: 0,
            resolution: 86400,
            start: (now - Duration::days(41)).timestamp(),
            end: now.timestamp(),
            fetched_at: now.timestamp_millis(),
            full_verified_at: now.timestamp_millis(),
            calendar: None,
            bars,
            feeds: vec!["sip".into()],
            delay_minutes: 0,
            adjustment: "split".into(),
            cached: false,
        }
    }

    #[test]
    fn incremental_history_reuses_prefix_and_replaces_overlap_atomically() {
        let previous = daily_history();
        let now =
            DateTime::from_timestamp_millis(previous.fetched_at).unwrap() + Duration::hours(1);
        let start = incremental_start(&previous, "TEST", true, now).unwrap();
        assert!(start.timestamp() > previous.start);
        let mut recent = previous.clone();
        recent.start = start.timestamp();
        recent.end += 3600;
        recent.fetched_at += 3600000;
        recent.bars.retain(|bar| bar.time >= recent.start);
        let overlap_count = recent.bars.len();
        let mut new = previous.bars.last().unwrap().clone();
        new.time = previous.end;
        new.end_time = recent.end;
        recent.bars.push(new.clone());
        let merged = reconcile_history(&previous, recent).unwrap();
        assert!(overlap_count < previous.bars.len());
        assert_eq!(merged.bars.len(), previous.bars.len() + 1);
        assert_eq!(&merged.bars[..previous.bars.len()], previous.bars);
        assert_eq!(merged.bars.last(), Some(&new));
        assert_eq!(merged.full_verified_at, previous.full_verified_at);
        assert_eq!(merged.start, previous.start);
    }

    #[test]
    fn splits_corrections_deletions_and_missing_overlap_require_full_replacement() {
        let previous = daily_history();
        for kind in 0..4 {
            let mut recent = previous.clone();
            match kind {
                0 => {
                    for bar in &mut recent.bars {
                        bar.open /= 2.0;
                        bar.high /= 2.0;
                        bar.low /= 2.0;
                        bar.close /= 2.0;
                        bar.volume *= 2.0;
                    }
                }
                1 => recent.bars[3].volume += 1.0,
                2 => {
                    recent.bars.remove(3);
                }
                _ => recent.bars.clear(),
            }
            assert!(reconcile_history(&previous, recent).is_none());
        }
        let mut recent = previous.clone();
        recent.start = previous.end;
        recent.bars.clear();
        assert!(reconcile_history(&previous, recent).is_none());
        assert_eq!(previous.bars.len(), 40);
    }

    #[test]
    fn incremental_history_requires_recent_full_verification_and_identical_scope() {
        let previous = daily_history();
        let now = DateTime::from_timestamp_millis(previous.fetched_at).unwrap();
        assert!(incremental_start(&previous, "TEST", true, now).is_some());
        assert!(incremental_start(&previous, "OTHER", true, now).is_none());
        assert!(incremental_start(&previous, "TEST", false, now).is_none());
        assert!(incremental_start(&previous, "TEST", true, now + Duration::hours(11)).is_none());
        assert!(incremental_start(&previous, "TEST", true, now + Duration::days(1)).is_none());
        let mut legacy = previous.clone();
        legacy.full_verified_at = 0;
        assert!(incremental_start(&legacy, "TEST", true, now).is_none());
        legacy = previous.clone();
        legacy.range = 31536000;
        assert!(incremental_start(&legacy, "TEST", true, now).is_none());
        legacy = previous.clone();
        legacy.full_verified_at += 1;
        assert!(incremental_start(&legacy, "TEST", true, now).is_none());
    }

    #[test]
    fn history_cadence_tracks_resolution_transition_and_closed_tail() {
        let now = Utc::now();
        let mut schedule = AlpacaMarketSchedule {
            session: AlpacaMarketSession::Core,
            next_transition_at: now + Duration::hours(8),
        };
        for (resolution, expected) in [(60, 60), (300, 300), (1800, 1800), (86400, 3600)] {
            assert_eq!(
                history_refresh_delay(resolution, &schedule, now, None).as_secs(),
                expected
            );
        }
        schedule.next_transition_at = now + Duration::seconds(30);
        assert_eq!(
            history_refresh_delay(86400, &schedule, now, None).as_secs(),
            31
        );
        schedule.session = AlpacaMarketSession::Closed;
        schedule.next_transition_at = now + Duration::hours(8);
        assert_eq!(
            history_refresh_delay(60, &schedule, now, Some(now)).as_secs(),
            1200
        );
        assert_eq!(
            history_refresh_delay(60, &schedule, now, Some(now - Duration::minutes(21))).as_secs(),
            28801
        );
    }

    fn sample_bar(time: &str) -> Value {
        json!({"T":"b", "S":"TEST", "t":time, "o":100.0, "h":102.0,
            "l":99.0, "c":101.0, "v":20, "n":3})
    }

    #[test]
    fn every_intraday_range_queries_both_sessions_without_a_current_session_dependency() {
        for range in [86400, 604800, 2592000] {
            let (_, seconds) = resolution(range).unwrap();
            assert_eq!(history_feeds(seconds), ["sip", "boats"]);
        }
        for range in [31536000, 0] {
            assert_eq!(history_feeds(resolution(range).unwrap().1), ["sip"]);
        }
        assert!(resolution(123).is_err());
        for session in [
            AlpacaMarketSession::Core,
            AlpacaMarketSession::PreMarket,
            AlpacaMarketSession::AfterHours,
        ] {
            assert_eq!(stream_feed(session, true, false), "sip");
            assert_eq!(stream_feed(session, false, true), "delayed_sip");
        }
        assert_eq!(
            stream_feed(AlpacaMarketSession::Overnight, true, false),
            "overnight"
        );
        assert_eq!(
            stream_feed(AlpacaMarketSession::Overnight, false, true),
            "boats"
        );
    }

    #[test]
    fn chart_calendar_preserves_early_closes_and_rejects_invalid_sessions() {
        let days = parse_calendar(&json!([
            {"date":"2026-12-24","open":"09:30","close":"13:00"},
            {"date":"2026-12-28","open":"09:30","close":"16:00"}
        ]))
        .unwrap();
        assert_eq!(days[0].close, 13 * 3600);
        assert_eq!(days.len(), 2);
        assert!(parse_calendar(&json!([
            {"date":"2026-12-24","open":"16:00","close":"13:00"}
        ]))
        .is_err());
        assert!(parse_calendar(&json!([
            {"date":"2026-12-24","close":"13:00"}
        ]))
        .is_err());
    }

    #[test]
    fn daily_end_uses_provider_midnight_offset_and_calendar_early_close() {
        let calendar = parse_closes(&json!([
            {"date":"2026-03-06", "close":"16:00"},
            {"date":"2026-03-09", "close":"16:00"},
            {"date":"2026-11-27", "close":"13:00"}
        ]))
        .unwrap();
        for (start, end) in [
            ("2026-03-06T05:00:00Z", "2026-03-06T21:00:00Z"),
            ("2026-03-09T04:00:00Z", "2026-03-09T20:00:00Z"),
            ("2026-11-27T05:00:00Z", "2026-11-27T18:00:00Z"),
        ] {
            let bar = parse_bar(&sample_bar(start), "sip").unwrap();
            assert_eq!(
                bar_end(&bar, 86400, &calendar).unwrap(),
                DateTime::parse_from_rfc3339(end).unwrap().timestamp()
            );
        }
        let holiday = parse_bar(&sample_bar("2026-12-25T05:00:00Z"), "sip").unwrap();
        assert!(bar_end(&holiday, 86400, &calendar).is_err());
        assert!(parse_closes(&json!([{"date":"2026-01-01", "close":"bad"}])).is_err());
    }

    #[test]
    fn rejects_malformed_bars_and_retains_all_ohlcv_fields() {
        let value = sample_bar("2026-09-17T14:00:00Z");
        let bar = parse_bar(&value, "sip").unwrap();
        assert_eq!(
            (bar.open, bar.high, bar.low, bar.close, bar.volume, bar.trades),
            (100.0, 102.0, 99.0, 101.0, 20.0, 3)
        );
        for (field, invalid) in [
            ("c", json!(-1)),
            ("h", json!(90)),
            ("v", json!(-1)),
            ("n", Value::Null),
            ("t", json!("invalid")),
        ] {
            let mut invalid_bar = value.clone();
            invalid_bar[field] = invalid;
            assert!(parse_bar(&invalid_bar, "boats").is_err());
        }
    }

    #[test]
    fn cache_replacement_removes_corrections_and_failed_commit_preserves_prior_history() {
        let directory = std::env::temp_dir().join(format!("brief-price-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("prices.sqlite3");
        let mut bar = parse_bar(&sample_bar("2026-09-17T14:00:00Z"), "sip").unwrap();
        bar.end_time = bar.time + 60;
        let mut history = PriceHistory {
            symbol: "TEST".into(),
            range: 86400,
            resolution: 60,
            start: bar.time - 86400,
            end: bar.end_time,
            fetched_at: bar.end_time * 1000,
            full_verified_at: bar.end_time * 1000,
            calendar: None,
            bars: vec![bar],
            feeds: vec!["sip".into(), "boats".into()],
            delay_minutes: 15,
            adjustment: "split".into(),
            cached: false,
        };
        save_history(&path, &history).unwrap();
        assert!(cached_history(&path, "TEST", 86400).unwrap().cached);
        assert!(cached_history(&path, "OTHER", 86400).is_none());
        history.bars.clear();
        let db = cache_connection(&path).unwrap();
        db.execute_batch("CREATE TRIGGER fail_cache BEFORE INSERT ON price_history BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(save_history(&path, &history).is_err());
        assert_eq!(cached_history(&path, "TEST", 86400).unwrap().bars.len(), 1);
        db.execute_batch("DROP TRIGGER fail_cache").unwrap();
        save_history(&path, &history).unwrap();
        assert!(cached_history(&path, "TEST", 86400)
            .unwrap()
            .bars
            .is_empty());
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn basic_portfolio_keeps_iex_while_entitled_prices_share_the_canonical_feed() {
        assert_eq!(
            valuation_feed(AlpacaMarketSession::Core, false, false),
            "iex"
        );
        assert_eq!(
            stream_feed(AlpacaMarketSession::Core, false, false),
            "delayed_sip"
        );
        assert_eq!(
            valuation_feed(AlpacaMarketSession::Core, true, false),
            "sip"
        );
        assert_eq!(
            valuation_feed(AlpacaMarketSession::Overnight, true, true),
            "boats"
        );
        assert_eq!(
            valuation_feed(AlpacaMarketSession::Overnight, false, false),
            "overnight"
        );
    }

    #[test]
    fn warmed_history_cannot_survive_a_correction_or_expiry_as_verified() {
        let providers = Providers::new().unwrap();
        let mut history = daily_history();
        history.fetched_at = Utc::now().timestamp_millis();
        let epoch = providers.market_streams.epoch.load(Ordering::SeqCst);
        providers.remember_chart(&history, epoch);
        assert!(providers
            .fresh_chart(&history.symbol, history.range)
            .is_some());
        providers
            .market_streams
            .epoch
            .fetch_add(1, Ordering::SeqCst);
        assert!(providers
            .fresh_chart(&history.symbol, history.range)
            .is_none());
        providers.remember_chart(&history, epoch);
        assert!(providers
            .fresh_chart(&history.symbol, history.range)
            .is_none());
        history.fetched_at -= 86_400_000;
        providers.remember_chart(&history, epoch + 1);
        assert!(providers
            .fresh_chart(&history.symbol, history.range)
            .is_none());
    }

    #[tokio::test]
    async fn ticker_and_range_changes_reuse_shared_feed_and_reject_old_quotes() {
        let feed = market_stream::Feed::test();
        {
            let mut state = feed.state.lock().unwrap();
            state.subscribed = ["TEST".into(), "OTHER".into()].into();
            state.quotes.insert(
                "OTHER".into(),
                json!({"T":"t", "S":"OTHER", "p":104, "t":"2026-09-17T14:00:02Z"}),
            );
        }
        let (selection_tx, selection) = tokio::sync::watch::channel(ChartSelection {
            symbol: "TEST".into(),
            range: 86400,
            request_id: "first".into(),
            warm_symbols: Vec::new(),
        });
        let (observed_tx, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let (repair, _) = tokio::sync::watch::channel((0, 0));
        let publish = |value: Value| {
            observed_tx.send(value).unwrap();
        };
        let status = |selected: &ChartSelection, status: &str| {
            publish(selected.tag(json!({"kind":"status", "status":status})));
        };
        let consumer = consume_shared_chart(
            &feed,
            selection,
            "sip",
            Utc::now() + Duration::minutes(1),
            &publish,
            &status,
            &repair,
        );
        let driver = async {
            assert_eq!(observed.recv().await.unwrap()["status"], "subscribed");
            selection_tx.send_replace(ChartSelection {
                symbol: "OTHER".into(),
                range: 86400,
                request_id: "second".into(),
                warm_symbols: Vec::new(),
            });
            assert_eq!(observed.recv().await.unwrap()["status"], "subscribed");
            let quote = observed.recv().await.unwrap();
            assert_eq!(quote["requestId"], "second");
            assert_eq!(quote["price"].as_f64(), Some(104.0));
            feed.events
                .send(vec![
                    json!({"T":"t", "S":"TEST", "p":999, "t":"2026-09-17T14:00:03Z"}),
                    sample_bar("2026-09-17T14:00:00Z"),
                    json!({"T":"x", "S":"OTHER"}),
                ])
                .unwrap();
            assert_eq!(observed.recv().await.unwrap()["kind"], "invalidate");
            selection_tx.send_replace(ChartSelection {
                symbol: "OTHER".into(),
                range: 604800,
                request_id: "third".into(),
                warm_symbols: Vec::new(),
            });
            assert_eq!(observed.recv().await.unwrap()["requestId"], "third");
            assert_eq!(observed.recv().await.unwrap()["requestId"], "third");
            assert!(observed.try_recv().is_err());
            drop(selection_tx);
        };
        tokio::time::timeout(StdDuration::from_secs(5), async {
            let (_, result) = tokio::join!(driver, consumer);
            result.unwrap();
        })
        .await
        .unwrap();
        assert_eq!(*repair.borrow(), (2, 1));
        // Chart selection/cleanup leaves the shared transport and all symbols intact.
        assert_eq!(feed.state.lock().unwrap().subscribed.len(), 2);
    }
}
