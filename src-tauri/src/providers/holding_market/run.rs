//! A selected chart while it is open: its history kept current over REST, the other held
//! symbols' histories warmed for switching, and its live bars and quotes streamed. Each part
//! runs until the chart is cancelled.
use super::*;

/// A chart's feed access: SIP is the paid consolidated feed, BOATS the overnight one.
#[derive(Clone, Copy)]
struct Access {
    sip: bool,
    boats: bool,
}

impl Access {
    /// Whether a range's REST bars come from a real-time feed.
    fn realtime(self, range: u64) -> bool {
        self.sip && (range == 0 || range >= 31_536_000 || self.boats)
    }
}

/// What every part of a running chart shares.
struct ChartRun<'a, F> {
    providers: &'a Providers,
    cache: &'a Path,
    credentials: AlpacaCredentials,
    access: Access,
    publish: &'a F,
}

impl Providers {
    pub async fn run_holding_chart<F>(
        &self,
        selection: tokio::sync::watch::Receiver<ChartSelection>,
        cache: &Path,
        cancel: tokio::sync::watch::Receiver<bool>,
        publish: F,
    ) -> Result<(), String>
    where
        F: Fn(Value) + Send + Sync,
    {
        let initial = selection.borrow().clone();
        let symbol = stock_symbol(&initial.symbol)?;
        let credentials = self.alpaca_credentials()?;
        let (sip, boats) = self.market_access(&symbol).await?;
        let run = ChartRun {
            providers: self,
            cache,
            credentials,
            access: Access { sip, boats },
            publish: &publish,
        };
        let (repair, repairs) = tokio::sync::watch::channel((0_u64, 0_u64));
        tokio::join!(
            run.histories(selection.clone(), cancel.clone(), repairs),
            run.streaming(selection.clone(), cancel.clone(), &repair),
            run.warming(selection, cancel),
        );
        Ok(())
    }
}

impl<F> ChartRun<'_, F>
where
    F: Fn(Value) + Send + Sync,
{
    /// The selected chart's history: the saved copy first, then REST on the market's schedule
    /// and whenever the stream asks for a repair. Starts over when the selection changes.
    async fn histories(
        &self,
        mut selection: tokio::sync::watch::Receiver<ChartSelection>,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        mut repairs: tokio::sync::watch::Receiver<(u64, u64)>,
    ) {
        let providers = self.providers;
        loop {
            let selected = selection.borrow_and_update().clone();
            let symbol = selected.symbol.clone();
            let range = selected.range;
            let (_, seconds) = resolution(range).expect("Validated chart range");
            let realtime = self.access.realtime(range);
            let publish = |payload| (self.publish)(selected.tag(payload));
            let history = async {
                let mut closed_since = None;
                let mut previous = providers
                    .fresh_chart(&symbol, range)
                    .or_else(|| cached_history(self.cache, &symbol, range));
                if let Some(history) = previous.as_ref() {
                    publish(json!({"kind": "history", "history": history, "startedAt": 0}));
                }
                let mut previous_epoch = providers.market_streams.epoch.load(Ordering::SeqCst);
                let mut verified_repair = 0;
                // Start REST without waiting for the socket. Subscription confirmation requests
                // another reconciliation, and the renderer retains bars received during a
                // download, closing any gap while the stream connects.
                loop {
                    if *cancel.borrow() {
                        return;
                    }
                    let started_at = Utc::now().timestamp_millis();
                    let full_repair = repairs.borrow().1;
                    let reusable = (full_repair == verified_repair
                        && previous_epoch == providers.market_streams.epoch.load(Ordering::SeqCst)
                        && previous
                            .as_ref()
                            .is_some_and(|h| !h.cached || previous_epoch <= 1))
                    .then_some(previous.as_ref())
                    .flatten();
                    let epoch = providers.market_streams.epoch.load(Ordering::SeqCst);
                    let fresh = (full_repair == verified_repair)
                        .then(|| providers.fresh_chart(&symbol, range))
                        .flatten();
                    let result = if let Some(history) = fresh {
                        Ok(history)
                    } else {
                        let download = providers.chart_history(
                            &self.credentials,
                            &symbol,
                            range,
                            realtime,
                            reusable,
                        );
                        tokio::select! {
                            _ = cancel.changed() => return,
                            result = download => result,
                        }
                    };
                    let succeeded = result.is_ok();
                    match result {
                        Ok(history) => {
                            if epoch != providers.market_streams.epoch.load(Ordering::SeqCst) {
                                previous = None;
                                continue;
                            }
                            providers.remember_chart(&history, epoch);
                            let cache_warning = save_history(self.cache, &history).err();
                            previous = Some(history.clone());
                            verified_repair = full_repair;
                            previous_epoch = epoch;
                            publish(json!({
                                "kind": "history",
                                "history": history,
                                "startedAt": started_at.min(history.fetched_at),
                                "warning": cache_warning,
                            }));
                        }
                        Err(_) => publish(json!({
                            "kind": "history-error",
                            "message": "History refresh failed; retaining the last verified chart.",
                        })),
                    }
                    let now = Utc::now();
                    let schedule = tokio::select! {
                        _ = cancel.changed() => return,
                        result = providers.market_schedule(&self.credentials, now) => result,
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
                        _ = cancel.changed() => return,
                        _ = tokio::time::sleep(delay) => {},
                        _ = repairs.changed() => {},
                    }
                }
            };
            tokio::select! {
                _ = history => return,
                result = selection.changed() => {
                    if result.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// The other streamed symbols' histories at the selected range, nearest neighbors first,
    /// so switching charts shows bars at once. Waits for the next selection once done.
    async fn warming(
        &self,
        mut selection: tokio::sync::watch::Receiver<ChartSelection>,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) {
        let providers = self.providers;
        loop {
            let selected = selection.borrow_and_update().clone();
            let ordered = warm_order(&selected, providers.market_streams.symbols());
            let work =
                stream::iter(ordered).for_each_concurrent(2, |symbol| {
                    let selected = &selected;
                    async move {
                        let target = ChartSelection {
                            symbol: symbol.clone(),
                            ..selected.clone()
                        };
                        let range = selected.range;
                        let previous = providers
                            .fresh_chart(&symbol, range)
                            .or_else(|| cached_history(self.cache, &symbol, range));
                        if let Some(history) = &previous {
                            (self.publish)(target.tag(
                                json!({"kind": "history", "history": history, "startedAt": 0}),
                            ));
                        }
                        if previous.as_ref().is_some_and(|history| !history.cached) {
                            return;
                        }
                        let epoch = providers.market_streams.epoch.load(Ordering::SeqCst);
                        let started_at = Utc::now().timestamp_millis();
                        let reusable = previous.as_ref().filter(|h| !h.cached || epoch <= 1);
                        let Ok(history) = providers
                            .chart_history(
                                &self.credentials,
                                &symbol,
                                range,
                                self.access.realtime(range),
                                reusable,
                            )
                            .await
                        else {
                            return;
                        };
                        if epoch != providers.market_streams.epoch.load(Ordering::SeqCst) {
                            return;
                        }
                        let _ = save_history(self.cache, &history);
                        providers.remember_chart(&history, epoch);
                        (self.publish)(target.tag(
                            json!({"kind": "history", "history": history, "startedAt": started_at}),
                        ));
                    }
                });
            tokio::select! {
                _ = cancel.changed() => return,
                changed = selection.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    continue;
                },
                _ = work => {},
            }
            tokio::select! {
                _ = cancel.changed() => return,
                changed = selection.changed() => {
                    if changed.is_err() {
                        return;
                    }
                },
            }
        }
    }

    /// Live bars and quotes for the selected symbol from the shared feed for the current
    /// session, reconnecting after a failure and pausing while the market is closed.
    async fn streaming(
        &self,
        mut selection: tokio::sync::watch::Receiver<ChartSelection>,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        repair: &tokio::sync::watch::Sender<(u64, u64)>,
    ) {
        let providers = self.providers;
        let Access { sip, boats } = self.access;
        loop {
            if *cancel.borrow() {
                return;
            }
            let schedule = tokio::select! {
                _ = cancel.changed() => return,
                result = providers.market_schedule(&self.credentials, Utc::now()) => result,
            };
            let schedule = match schedule {
                Ok(schedule) => schedule,
                Err(_) => {
                    (self.publish)(selection.borrow().tag(json!({
                        "kind": "status",
                        "status": "reconnecting",
                        "session": "Unknown",
                        "feed": "",
                        "delayMinutes": 0,
                    })));
                    tokio::select! {
                        _ = cancel.changed() => return,
                        _ = tokio::time::sleep(StdDuration::from_secs(15)) => {}
                    }
                    continue;
                }
            };
            let session = schedule.session;
            if session == AlpacaMarketSession::Closed {
                (self.publish)(selection.borrow_and_update().tag(json!({
                    "kind": "status",
                    "status": "closed",
                    "session": session.label(),
                    "feed": "",
                    "delayMinutes": 0,
                })));
                let wait = (schedule.next_transition_at - Utc::now())
                    .num_seconds()
                    .max(1) as u64
                    + 1;
                tokio::select! {
                    _ = cancel.changed() => return,
                    result = selection.changed() => {
                        if result.is_err() {
                            return;
                        }
                    },
                    _ = tokio::time::sleep(StdDuration::from_secs(wait)) => {}
                }
                continue;
            }
            let feed = stream_feed(session, sip, boats);
            let status = |selected: &ChartSelection, state: &str| {
                (self.publish)(selected.tag(json!({
                    "kind": "status",
                    "status": state,
                    "session": session.label(),
                    "feed": feed,
                    "delayMinutes": if feed == "delayed_sip" { 15 } else { 0 },
                })))
            };
            let result = match providers
                .alpaca_credentials()
                .and_then(|credentials| providers.market_streams.acquire(feed, credentials))
            {
                Ok(shared) => {
                    let consumer = consume_shared_chart(
                        &shared,
                        selection.clone(),
                        feed,
                        schedule.next_transition_at,
                        self.publish,
                        &status,
                        repair,
                    );
                    tokio::select! {
                        _ = cancel.changed() => return,
                        result = consumer => result,
                    }
                }
                Err(error) => Err(error),
            };
            if *cancel.borrow() {
                return;
            }
            if let Err(error) = result {
                status(
                    &selection.borrow(),
                    if error == "Stream access unavailable" {
                        "unavailable"
                    } else {
                        "reconnecting"
                    },
                );
                tokio::select! {
                    _ = cancel.changed() => return,
                    _ = tokio::time::sleep(StdDuration::from_secs(10)) => {}
                }
            }
        }
    }
}

/// The streamed symbols to warm besides the selected one: the page's order first, then the
/// rest, alternating outward from the selected symbol so its neighbors come first.
fn warm_order(selected: &ChartSelection, streamed: Vec<String>) -> Vec<String> {
    let mut order: Vec<String> = selected
        .warm_symbols
        .iter()
        .filter(|s| streamed.contains(s))
        .cloned()
        .collect();
    for symbol in streamed {
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
    ordered
}

pub(super) async fn consume_shared_chart<F, S>(
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

    #[test]
    fn warming_starts_with_the_selected_symbols_neighbors() {
        let selected = ChartSelection {
            symbol: "CCC".into(),
            range: 86_400,
            request_id: "synthetic".into(),
            warm_symbols: vec!["AAA".into(), "BBB".into(), "CCC".into(), "DDD".into()],
        };
        let streamed = ["DDD", "CCC", "EEE", "BBB", "AAA"]
            .map(String::from)
            .to_vec();
        // The page's order, then EEE; from CCC: DDD and BBB, then EEE and AAA.
        assert_eq!(
            warm_order(&selected, streamed),
            ["DDD", "BBB", "EEE", "AAA"]
        );
        let alone = warm_order(&selected, vec!["CCC".into()]);
        assert!(alone.is_empty());
    }
}
