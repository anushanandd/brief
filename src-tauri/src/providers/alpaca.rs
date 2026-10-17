use super::market_stream::{
    consume_market_stream, newest_market_time, previous_close_snapshots, publish_market_backfill,
    stream_snapshot, update_stream_price, ChartFrames, StreamPacing,
};
use super::*;

impl Providers {
    pub async fn market_snapshots(&self, symbols: Vec<String>) -> Result<MarketSnapshots, String> {
        let symbols = normalize_stock_symbols(symbols);
        if symbols.is_empty() {
            return Ok(MarketSnapshots {
                snapshots: BTreeMap::new(),
                session: AlpacaMarketSession::Closed.label().into(),
                feed: AlpacaMarketSession::Closed.feed().into(),
                delay_minutes: 0,
                as_of: None,
                next_transition_at: None,
                poll_interval_ms: None,
                history_feed: AlpacaMarketSession::Closed.history_feed().into(),
                history_delay_minutes: 0,
            });
        }
        let credentials = self.alpaca_credentials()?;
        let now = Utc::now();
        let snapshot_request = async {
            let schedule = self.market_schedule(&credentials, now).await?;
            let (sip, boats) = self.market_access(&symbols[0]).await?;
            let feed = holding_market::valuation_feed(schedule.session, sip, boats);
            let joined = symbols.join(",");
            let payload = self
                .alpaca_request(
                    &credentials,
                    "/v2/stocks/snapshots",
                    &[("symbols", joined.as_str()), ("feed", feed)],
                )
                .await?;
            Ok::<_, String>((schedule, payload, feed))
        };
        let (result, weekly_references) = tokio::join!(
            snapshot_request,
            self.weekly_reference_closes(&credentials, &symbols, now)
        );
        let (schedule, payload, feed) = result?;
        let session = schedule.session;
        let mut snapshots = parse_market_snapshots(&payload, feed == "overnight");
        for (symbol, snapshot) in &mut snapshots {
            if let Some(reference) = weekly_references.get(symbol) {
                snapshot.weekly_change_pct =
                    Some((snapshot.price - reference.price) / reference.price * 100.0);
                snapshot.weekly_reference_price = Some(reference.price);
                snapshot.weekly_reference_date = Some(reference.date.clone());
            }
        }
        let as_of = snapshots
            .values()
            .map(|snapshot| snapshot.as_of.as_str())
            .filter(|value| !value.is_empty())
            .max()
            .map(str::to_owned);
        Ok(MarketSnapshots {
            snapshots,
            session: session.label().into(),
            feed: feed.into(),
            delay_minutes: if feed == "delayed_sip" { 15 } else { 0 },
            as_of,
            next_transition_at: Some(schedule.next_transition_at.to_rfc3339()),
            poll_interval_ms: session.poll_interval_ms(),
            history_feed: if matches!(feed, "sip" | "boats") {
                feed
            } else {
                session.history_feed()
            }
            .into(),
            history_delay_minutes: if matches!(feed, "sip" | "boats") {
                0
            } else {
                session.history_delay_minutes()
            },
        })
    }

    pub async fn market_history(
        &self,
        symbols: &[String],
        market: &MarketSnapshots,
    ) -> Result<Vec<MarketFrame>, String> {
        let symbols = normalize_stock_symbols(symbols.to_vec());
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        let credentials = self.alpaca_credentials()?;
        let end = Utc::now() - Duration::minutes(i64::from(market.history_delay_minutes));
        let start = end - Duration::hours(24);
        let joined = symbols.join(",");
        let start = start.to_rfc3339();
        let end = end.to_rfc3339();
        let mut frames = BTreeMap::<String, BTreeMap<String, MarketSnapshot>>::new();
        let parameters = [
            ("symbols", joined.as_str()),
            ("timeframe", "1Min"),
            ("start", start.as_str()),
            ("end", end.as_str()),
            ("limit", "10000"),
            ("feed", market.history_feed.as_str()),
            ("adjustment", "split"),
            ("sort", "asc"),
        ];
        self.alpaca_pages(
            &credentials,
            "/v2/stocks/bars",
            &parameters,
            20,
            |payload| {
                for frame in parse_market_bar_frames(payload, &market.snapshots) {
                    frames
                        .entry(frame.as_of)
                        .or_default()
                        .extend(frame.snapshots);
                }
                Ok(())
            },
        )
        .await?;
        Ok(frames
            .into_iter()
            .map(|(as_of, snapshots)| MarketFrame { as_of, snapshots })
            .collect())
    }

    pub async fn stream_market_updates<F>(
        &self,
        symbols: Vec<String>,
        mut market: MarketSnapshots,
        update_interval: StdDuration,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        mut publish: F,
    ) -> Result<(), String>
    where
        F: FnMut(MarketStreamTick) -> Result<(), String> + Send,
    {
        let symbols = normalize_stock_symbols(symbols);
        if symbols.is_empty() {
            return Ok(());
        }

        let mut chart = ChartFrames::new(previous_close_snapshots(&market.snapshots, &[]));
        if let Ok(backfill) = self.market_history(&symbols, &market).await {
            publish_market_backfill(&market, backfill, &mut chart, &mut publish)?;
        }
        if market.poll_interval_ms.is_none() {
            return Ok(());
        }

        let transition = market
            .next_transition_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .ok_or_else(|| "Alpaca did not return the next market transition".to_string())?;
        let pacing = StreamPacing {
            until: transition,
            every: update_interval,
        };
        let mut attempt = 0;
        loop {
            if *cancel.borrow() || Utc::now() >= transition {
                return Ok(());
            }
            let feed = self
                .market_streams
                .acquire(&market.feed, self.alpaca_credentials()?)?;
            // Keep the canonical feed warm even when no Holdings chart is mounted.
            let canonical = self.canonical_stream_feed(&symbols[0]).await?;
            let canonical_feed = self
                .market_streams
                .acquire(&canonical, self.alpaca_credentials()?)?;
            let mut canonical_events = canonical_feed.events.subscribe();
            let canonical_failure = async {
                if canonical_feed.state.lock().unwrap().finished {
                    return;
                }
                loop {
                    match canonical_events.recv().await {
                        Ok(events) if !events.iter().any(|item| item["T"] == "error") => {}
                        _ => return,
                    }
                }
            };
            let consumption = consume_market_stream(
                &feed,
                &symbols,
                &pacing,
                &mut market,
                &mut chart,
                &mut cancel,
                &mut publish,
            );
            let result = tokio::select! {
                result = consumption => Some(result),
                _ = canonical_failure => Some(Err("Canonical feed disconnected".into())),
                _ = tokio::time::sleep(StdDuration::from_secs(300)) => None,
            };
            match result {
                Some(Ok(())) => return Ok(()),
                Some(Err(error)) => {
                    if error != "Market correction requires reconciliation" {
                        attempt += 1;
                    }
                    if attempt >= 3 {
                        return Err(error);
                    }
                }
                None => {
                    let refreshed = tokio::select! {
                        _ = cancel.changed() => return Ok(()),
                        result = self.market_snapshots(symbols.clone()) => result,
                    };
                    if let Ok(mut refreshed) = refreshed {
                        if refreshed.feed == market.feed {
                            for (symbol, snapshot) in &market.snapshots {
                                update_stream_price(
                                    &mut refreshed.snapshots,
                                    symbol,
                                    snapshot.price,
                                    &snapshot.as_of,
                                );
                            }
                            refreshed.as_of = newest_market_time(&refreshed.snapshots);
                        }
                        market = refreshed;
                        publish(MarketStreamTick {
                            market: market.clone(),
                            chart_snapshots: None,
                            chart_as_of: None,
                            latest: true,
                        })?;
                    }
                    continue;
                }
            }
            let refreshed = tokio::select! {
                _ = cancel.changed() => return Ok(()),
                result = self.market_snapshots(symbols.clone()) => result,
            };
            if let Ok(refreshed) = refreshed {
                market = refreshed;
                publish(MarketStreamTick {
                    market: market.clone(),
                    chart_snapshots: None,
                    chart_as_of: None,
                    latest: true,
                })?;
            }
            if let Ok(backfill) = self.market_history(&symbols, &market).await {
                publish_market_backfill(&market, backfill, &mut chart, &mut publish)?;
            }
            let backoff = StdDuration::from_secs(1 << attempt);
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    pub(super) async fn weekly_reference_closes(
        &self,
        credentials: &AlpacaCredentials,
        symbols: &[String],
        now: DateTime<Utc>,
    ) -> BTreeMap<String, WeeklyReference> {
        let (reference_date, start, end) = weekly_reference_window(now);
        let missing = if let Ok(mut cache) = self.weekly_closes.lock() {
            if cache.reference_date != reference_date {
                *cache = WeeklyCloseCache {
                    reference_date: reference_date.clone(),
                    ..WeeklyCloseCache::default()
                };
            }
            let missing = symbols
                .iter()
                .filter(|symbol| !cache.references.contains_key(*symbol))
                .cloned()
                .collect::<Vec<_>>();
            let recently_attempted = cache
                .attempted_at
                .is_some_and(|attempted| attempted.elapsed() < StdDuration::from_secs(300));
            if missing.is_empty()
                || (recently_attempted
                    && missing
                        .iter()
                        .all(|symbol| cache.attempted_symbols.contains(symbol)))
            {
                return cache.references.clone();
            }
            missing
        } else {
            symbols.to_vec()
        };

        let joined = missing.join(",");
        let mut references = BTreeMap::new();
        let parameters = [
            ("symbols", joined.as_str()),
            ("timeframe", "1Day"),
            ("start", start.as_str()),
            ("end", end.as_str()),
            ("limit", "1000"),
            ("feed", "iex"),
            ("adjustment", "split"),
            ("sort", "asc"),
        ];
        // A failed page keeps the references read before it.
        let _ = self
            .alpaca_pages(credentials, "/v2/stocks/bars", &parameters, 20, |payload| {
                for (symbol, reference) in parse_weekly_reference_closes(payload, &reference_date) {
                    references
                        .entry(symbol)
                        .and_modify(|current: &mut WeeklyReference| {
                            if reference.date > current.date {
                                *current = reference.clone();
                            }
                        })
                        .or_insert(reference);
                }
                Ok(())
            })
            .await;

        if let Ok(mut cache) = self.weekly_closes.lock() {
            cache.attempted_symbols.extend(missing);
            cache.references.extend(references.clone());
            cache.attempted_at = Some(Instant::now());
            return cache.references.clone();
        }
        references
    }

    pub async fn sync_benchmark(&self) -> Result<Value, String> {
        let credentials = self
            .read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| "Alpaca credentials are not configured".to_string())?;
        let mut histories = self
            .sync_security_history_from_alpaca(&credentials, &["VOO".into()], "all")
            .await?;
        histories
            .remove("VOO")
            .filter(|points| !points.is_empty())
            .map(Value::Array)
            .ok_or_else(|| "Alpaca returned no historical prices for VOO".to_string())
    }

    /// Daily closes for every symbol `security_history_symbols` names.
    pub async fn sync_security_history(
        &self,
        plaid: &PlaidData,
        snaptrade: Option<&SnapTradeData>,
    ) -> Result<BTreeMap<String, Vec<Value>>, String> {
        let symbols = security_history_symbols(plaid, snaptrade);
        if symbols.is_empty() {
            return Ok(BTreeMap::new());
        }
        let credentials = self
            .read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| {
                "Alpaca credentials are required for adjusted price history".to_string()
            })?;
        self.sync_security_history_from_alpaca(&credentials, &symbols, "split")
            .await
    }

    /// Splits and cash dividends for these symbols since `since`, through dividends announced
    /// for the coming months.
    pub async fn sync_corporate_actions(
        &self,
        symbols: &[String],
        since: chrono::NaiveDate,
    ) -> Result<Vec<CorporateAction>, String> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        let credentials = self
            .read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| {
                "Alpaca credentials are required for splits and dividends".to_string()
            })?;
        let start = since.format("%Y-%m-%d").to_string();
        let end = (Utc::now() + Duration::days(CORPORATE_ACTIONS_AHEAD_DAYS))
            .format("%Y-%m-%d")
            .to_string();
        let mut actions = Vec::new();
        for chunk in symbols.chunks(CORPORATE_ACTION_SYMBOLS_PER_REQUEST) {
            let joined = chunk.join(",");
            let parameters = [
                ("symbols", joined.as_str()),
                ("types", "forward_split,reverse_split,cash_dividend"),
                ("start", start.as_str()),
                ("end", end.as_str()),
                ("limit", "1000"),
            ];
            self.alpaca_pages(
                &credentials,
                "/v1/corporate-actions",
                &parameters,
                CORPORATE_ACTION_PAGES,
                |payload| {
                    actions.extend(corporate_actions::parse(payload));
                    Ok(())
                },
            )
            .await?;
        }
        Ok(corporate_actions::normalized(actions))
    }

    /// Two years of daily closes for each symbol; a symbol Alpaca has none for maps to an empty
    /// list. A symbol whose request fails is left out, and the whole call fails only when every
    /// request did.
    pub(super) async fn sync_security_history_from_alpaca(
        &self,
        credentials: &AlpacaCredentials,
        symbols: &[String],
        adjustment: &str,
    ) -> Result<BTreeMap<String, Vec<Value>>, String> {
        let since = (Utc::now() - Duration::days(730))
            .format("%Y-%m-%d")
            .to_string();
        let mut histories = BTreeMap::new();
        let mut failure = None;
        for symbol in symbols {
            let path = format!("/v2/stocks/{symbol}/bars");
            let payload = self
                .alpaca_request(
                    credentials,
                    &path,
                    &[
                        ("timeframe", "1Day"),
                        ("start", since.as_str()),
                        ("limit", "1000"),
                        ("feed", "iex"),
                        ("adjustment", adjustment),
                    ],
                )
                .await;
            match payload {
                Ok(payload) => {
                    histories.insert(symbol.clone(), parse_alpaca_history(&payload));
                }
                Err(error) => failure = failure.or(Some(error)),
            }
        }
        match failure {
            Some(error) if histories.is_empty() => Err(error),
            _ => Ok(histories),
        }
    }

    pub(super) async fn market_schedule(
        &self,
        credentials: &AlpacaCredentials,
        now: DateTime<Utc>,
    ) -> Result<AlpacaMarketSchedule, String> {
        if let Some(schedule) = self
            .market_schedule
            .lock()
            .map_err(|_| "The market schedule cache is unavailable".to_string())?
            .as_ref()
            .filter(|schedule| schedule.next_transition_at > now + Duration::seconds(1))
            .cloned()
        {
            return Ok(schedule);
        }

        let payload = self.alpaca_clock_request(credentials).await?;
        let schedule = parse_alpaca_market_schedule(&payload, now)?;
        *self
            .market_schedule
            .lock()
            .map_err(|_| "The market schedule cache is unavailable".to_string())? =
            Some(schedule.clone());
        Ok(schedule)
    }

    pub(super) async fn alpaca_clock_request(
        &self,
        credentials: &AlpacaCredentials,
    ) -> Result<Value, String> {
        let mut errors = Vec::new();
        for base in [
            "https://paper-api.alpaca.markets",
            "https://api.alpaca.markets",
        ] {
            let response = self
                .http
                .get(format!("{base}/v3/clock"))
                .header("APCA-API-KEY-ID", &credentials.key_id)
                .header("APCA-API-SECRET-KEY", &credentials.secret_key)
                .query(&[("markets", "NASDAQ,BOATS")])
                .send()
                .await;
            match response {
                Ok(response) => {
                    match provider_response("Alpaca market clock", "/v3/clock", response).await {
                        Ok(payload) => return Ok(payload),
                        Err(error) => errors.push(error),
                    }
                }
                Err(_) => errors.push("Could not reach Alpaca market clock".into()),
            }
        }
        Err(errors.join(" · "))
    }

    /// Reads an Alpaca endpoint page by page, at most `max_pages`, handing each page to `page`.
    /// `Ok(false)` when pages remained, or a token repeated, after the last one read.
    pub(super) async fn alpaca_pages(
        &self,
        credentials: &AlpacaCredentials,
        path: &str,
        parameters: &[(&str, &str)],
        max_pages: usize,
        mut page: impl FnMut(&Value) -> Result<(), String>,
    ) -> Result<bool, String> {
        let mut token: Option<String> = None;
        let mut seen = BTreeSet::new();
        for _ in 0..max_pages {
            let mut query = parameters.to_vec();
            if let Some(token) = token.as_deref() {
                query.push(("page_token", token));
            }
            let payload = self.alpaca_request(credentials, path, &query).await?;
            page(&payload)?;
            match payload
                .get("next_page_token")
                .and_then(Value::as_str)
                .filter(|next| !next.is_empty())
            {
                None => return Ok(true),
                Some(next) if !seen.insert(next.to_owned()) => return Ok(false),
                Some(next) => token = Some(next.to_owned()),
            }
        }
        Ok(false)
    }

    pub(super) async fn alpaca_request(
        &self,
        credentials: &AlpacaCredentials,
        path: &str,
        parameters: &[(&str, &str)],
    ) -> Result<Value, String> {
        let request = self
            .http
            .get(format!("https://data.alpaca.markets{path}"))
            .header("APCA-API-KEY-ID", &credentials.key_id)
            .header("APCA-API-SECRET-KEY", &credentials.secret_key)
            .query(parameters);
        let retry = request.try_clone();
        let response =
            send_with_retry("Alpaca", request, retry)
                .await
                .map_err(|(_, attempts)| {
                    http::observe(&http::ProviderError::network("Alpaca", path, attempts));
                    "Could not reach Alpaca".to_string()
                })?;
        provider_response("Alpaca", path, response).await
    }
}

pub fn normalize_stock_symbols(symbols: Vec<String>) -> Vec<String> {
    symbols
        .into_iter()
        .filter_map(|symbol| stock_symbol(&symbol).ok())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Dividends announced this far ahead are fetched with the history.
const CORPORATE_ACTIONS_AHEAD_DAYS: i64 = 120;
const CORPORATE_ACTION_SYMBOLS_PER_REQUEST: usize = 50;
const CORPORATE_ACTION_PAGES: usize = 20;

/// The symbols whose daily closes price an account's past: Plaid stock-plan holdings, and every
/// symbol a SnapTrade account holds or changed shares of, so its history can be rebuilt from
/// its trades. Allocated stock-plan shares arrive without a price, so they need closes too.
pub(crate) fn security_history_symbols(
    plaid: &PlaidData,
    snaptrade: Option<&SnapTradeData>,
) -> Vec<String> {
    let stock_plan_accounts = plaid
        .investment_accounts
        .iter()
        .filter(|account| {
            account
                .get("subtype")
                .and_then(Value::as_str)
                .is_some_and(|subtype| subtype.eq_ignore_ascii_case("stock plan"))
        })
        .filter_map(|account| account.get("account_id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let security_ids = plaid
        .holdings
        .iter()
        .filter(|holding| {
            holding
                .get("account_id")
                .and_then(Value::as_str)
                .is_some_and(|id| stock_plan_accounts.contains(id))
        })
        .filter_map(|holding| holding.get("security_id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let label = |label: &Value| {
        label
            .get("raw_symbol")
            .or_else(|| label.get("symbol"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let held = snaptrade
        .into_iter()
        .flat_map(|data| data.positions.values().flatten())
        .filter_map(|position| label(position.get("instrument")?));
    let traded = snaptrade
        .into_iter()
        .flat_map(|data| data.activities.values().flatten())
        .filter(|activity| {
            activity
                .get("units")
                .and_then(|units| units.as_f64().or_else(|| units.as_str()?.parse().ok()))
                .is_some_and(|units: f64| units != 0.0)
        })
        .filter_map(|activity| label(activity.get("symbol")?));
    plaid
        .securities
        .iter()
        .filter(|security| {
            security
                .get("security_id")
                .and_then(Value::as_str)
                .is_some_and(|id| security_ids.contains(id))
        })
        .filter_map(|security| security.get("ticker_symbol").and_then(Value::as_str))
        .map(str::to_owned)
        .chain(held)
        .chain(traded)
        .filter_map(|symbol| stock_symbol(&symbol).ok())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn stock_symbol(value: &str) -> Result<String, String> {
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

pub(super) fn parse_alpaca_market_schedule(
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

pub(super) fn quote_midpoint(value: &Value) -> Option<f64> {
    let bid = value.get("bp").and_then(Value::as_f64)?;
    let ask = value.get("ap").and_then(Value::as_f64)?;
    (bid.is_finite() && ask.is_finite() && bid > 0.0 && ask > 0.0).then_some((bid + ask) / 2.0)
}

pub(super) fn weekly_reference_window(now: DateTime<Utc>) -> (String, String, String) {
    let target = now.date_naive() - Duration::days(7);
    (
        target.to_string(),
        (target - Duration::days(7)).to_string(),
        (target + Duration::days(1)).to_string(),
    )
}

pub(super) fn parse_weekly_reference_closes(
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

pub(super) fn parse_market_snapshots(
    payload: &Value,
    prefer_quote: bool,
) -> BTreeMap<String, MarketSnapshot> {
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

pub(super) fn parse_market_bar_frames(
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

pub(super) fn parse_alpaca_history(payload: &Value) -> Vec<Value> {
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

#[cfg(test)]
mod tests;
