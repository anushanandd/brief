//! Security charts are independent of portfolio valuation. REST bars are canonical;
//! live trades/indicative quotes are separate observations, never historical closes.
use super::alpaca::{quote_midpoint, stock_symbol};
use super::*;
use chrono::Timelike;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

mod cache;
mod run;

use cache::save_history;
pub use cache::{cached_histories, cached_history, clear_cache};

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

/// Which real-time feeds the saved Alpaca keys can read, checked for one credential generation.
#[derive(Clone, Copy)]
pub(super) struct MarketAccess {
    generation: u64,
    checked: Instant,
    /// The consolidated feed, a paid subscription.
    sip: bool,
    /// The overnight feed.
    boats: bool,
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
        if let Some(access) = *self
            .market_access
            .lock()
            .map_err(|_| "Market access unavailable")?
        {
            if access.generation == generation
                && access.checked.elapsed() < StdDuration::from_secs(300)
            {
                return Ok((access.sip, access.boats));
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
            .map_err(|_| "Market access unavailable")? = Some(MarketAccess {
            generation,
            checked: Instant::now(),
            sip,
            boats,
        });
        Ok((sip, boats))
    }
    /// Each symbol's week of five-minute bars for the portfolio chart, shared with the holding
    /// chart's week: memory first, then Alpaca, saved back to the price cache. Any symbol that
    /// can't be read fails the whole set, since a partial portfolio would misstate its value.
    pub async fn week_bars(
        &self,
        cache: &Path,
        symbols: &[String],
    ) -> Result<BTreeMap<String, PriceHistory>, String> {
        const WEEK: u64 = 604_800;
        let credentials = self.alpaca_credentials()?;
        let mut histories = BTreeMap::new();
        for symbol in symbols {
            let epoch = self.market_streams.epoch.load(Ordering::SeqCst);
            let history = match self.fresh_chart(symbol, WEEK) {
                Some(history) => history,
                None => {
                    let (sip, boats) = self.market_access(symbol).await?;
                    let history = self
                        .chart_history(&credentials, symbol, WEEK, sip && boats, None)
                        .await?;
                    self.remember_chart(&history, epoch);
                    // The cache is disposable; a failed save only costs the next launch.
                    let _ = save_history(cache, &history);
                    history
                }
            };
            histories.insert(symbol.clone(), history);
        }
        Ok(histories)
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
            let start_text = start.to_rfc3339();
            let end_text = end.to_rfc3339();
            let params = [
                ("timeframe", timeframe),
                ("start", &start_text),
                ("end", &end_text),
                ("limit", "10000"),
                ("feed", *feed),
                ("adjustment", "split"),
                ("sort", "asc"),
            ];
            let path = format!("/v2/stocks/{symbol}/bars");
            let complete = self
                .alpaca_pages(credentials, &path, &params, 20, |result| {
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
                    Ok(())
                })
                .await?;
            if !complete {
                return Err("Price history pagination is incomplete".into());
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
}

#[cfg(test)]
mod tests;
