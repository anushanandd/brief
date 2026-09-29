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
        let mut page_token: Option<String> = None;
        let mut frames = BTreeMap::<String, BTreeMap<String, MarketSnapshot>>::new();
        for _ in 0..20 {
            let mut parameters = vec![
                ("symbols", joined.as_str()),
                ("timeframe", "1Min"),
                ("start", start.as_str()),
                ("end", end.as_str()),
                ("limit", "10000"),
                ("feed", market.history_feed.as_str()),
                ("adjustment", "split"),
                ("sort", "asc"),
            ];
            if let Some(token) = page_token.as_deref() {
                parameters.push(("page_token", token));
            }
            let payload = self
                .alpaca_request(&credentials, "/v2/stocks/bars", &parameters)
                .await?;
            for frame in parse_market_bar_frames(&payload, &market.snapshots) {
                frames
                    .entry(frame.as_of)
                    .or_default()
                    .extend(frame.snapshots);
            }
            page_token = payload
                .get("next_page_token")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if page_token.is_none() {
                break;
            }
        }
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

        let chart_seed = previous_close_snapshots(&market.snapshots, &[]);
        let mut bar_frames = BTreeMap::new();
        if let Ok(backfill) = self.market_history(&symbols, &market).await {
            publish_market_backfill(
                &market,
                backfill,
                &chart_seed,
                &mut bar_frames,
                &mut publish,
            )?;
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
                transition,
                &mut market,
                &chart_seed,
                &mut bar_frames,
                update_interval,
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
                })?;
            }
            if let Ok(backfill) = self.market_history(&symbols, &market).await {
                publish_market_backfill(
                    &market,
                    backfill,
                    &chart_seed,
                    &mut bar_frames,
                    &mut publish,
                )?;
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
        let mut page_token: Option<String> = None;
        for _ in 0..20 {
            let mut parameters = vec![
                ("symbols", joined.as_str()),
                ("timeframe", "1Day"),
                ("start", start.as_str()),
                ("end", end.as_str()),
                ("limit", "1000"),
                ("feed", "iex"),
                ("adjustment", "split"),
                ("sort", "asc"),
            ];
            if let Some(token) = page_token.as_deref() {
                parameters.push(("page_token", token));
            }
            let Ok(payload) = self
                .alpaca_request(credentials, "/v2/stocks/bars", &parameters)
                .await
            else {
                break;
            };
            for (symbol, reference) in parse_weekly_reference_closes(&payload, &reference_date) {
                references
                    .entry(symbol)
                    .and_modify(|current: &mut WeeklyReference| {
                        if reference.date > current.date {
                            *current = reference.clone();
                        }
                    })
                    .or_insert(reference);
            }
            page_token = payload
                .get("next_page_token")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if page_token.is_none() {
                break;
            }
        }

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
            .map(Value::Array)
            .ok_or_else(|| "Alpaca returned no historical prices for VOO".to_string())
    }

    pub async fn sync_security_history(
        &self,
        plaid: &PlaidData,
    ) -> Result<BTreeMap<String, Vec<Value>>, String> {
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
        let symbols = plaid
            .securities
            .iter()
            .filter(|security| {
                security
                    .get("security_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| security_ids.contains(id))
            })
            .filter_map(|security| security.get("ticker_symbol").and_then(Value::as_str))
            .filter_map(|symbol| stock_symbol(symbol).ok())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

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
                .await?;
            let points = parse_alpaca_history(&payload);
            if points.is_empty() {
                return Err(format!("Alpaca returned no historical prices for {symbol}"));
            }
            histories.insert(symbol.clone(), points);
        }
        Ok(histories)
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
        match request.send().await {
            Ok(response) if retry.is_some() && retryable_status(response.status()) => {
                retry_pause(&response).await;
                let response = retry
                    .ok_or("Alpaca request could not be retried")?
                    .send()
                    .await
                    .map_err(|_| "Could not reach Alpaca".to_string())?;
                provider_response("Alpaca", path, response).await
            }
            Ok(response) => provider_response("Alpaca", path, response).await,
            Err(_) if retry.is_some() => {
                tokio::time::sleep(StdDuration::from_millis(250)).await;
                let response = retry
                    .ok_or("Alpaca request could not be retried")?
                    .send()
                    .await
                    .map_err(|_| "Could not reach Alpaca".to_string())?;
                provider_response("Alpaca", path, response).await
            }
            Err(_) => Err("Could not reach Alpaca".into()),
        }
    }
}
