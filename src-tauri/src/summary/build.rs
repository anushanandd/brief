//! Builds a summary from its gathered inputs, without the network: the period's sessions in
//! layers, their events, today, the period, and the brief.
use super::*;

/// What a summary is built from once the ledger, saved evidence, quotes, and a year of prices
/// are in hand.
pub(super) struct Gathered {
    pub summary: InvestmentSummary,
    pub ledger: Ledger,
    pub evidence: Vec<Evidence>,
    pub sectors: Sectors,
    pub market_news: Vec<MarketNewsArticle>,
    pub quotes: Quotes,
    pub prices: Prices,
}

pub(super) fn build(
    gathered: Gathered,
    snapshot: &Snapshot,
    period: Period,
    ticker: &Option<String>,
    kept: Kept,
    trace: &mut Trace,
) -> Prepared {
    let Gathered {
        mut summary,
        ledger,
        mut evidence,
        sectors,
        market_news,
        quotes,
        prices,
    } = gathered;
    // Each session in the period in layers: saved ones as they were, the rest from closes,
    // and the latest session from its closes (or live prices while it trades).
    let tickers: Vec<String> = quotes
        .held
        .iter()
        .map(|security| security.ticker.clone())
        .collect();
    let priors: BTreeMap<String, &'static str> = sectors
        .iter()
        .map(|(ticker, (fund, _))| (ticker.clone(), *fund))
        .collect();
    let shares: BTreeMap<String, Shares> = ledger
        .held
        .iter()
        .filter_map(|security| {
            let rebuilt = Shares::rebuild(
                &security.ticker,
                security.shares,
                &ledger.trades,
                period.start,
            )?;
            Some((security.ticker.clone(), rebuilt))
        })
        .collect();
    let inputs = sessions::Inputs {
        returns: &prices.returns,
        closes: &prices.closes,
        priors: &priors,
        shares: &shares,
    };
    let Kept {
        sessions: kept_sessions,
        events: kept_events,
        corporate_actions,
    } = kept;
    let mut layers = Layers::new(kept_sessions, &tickers, &inputs, period);
    trace.stage(
        "sessions",
        if prices.returns.is_empty() {
            "skipped"
        } else {
            "ok"
        },
        Some(format!(
            "{} saved, {} filled, latest {}",
            layers.saved.len(),
            layers.filled.len(),
            if layers.latest.is_empty() {
                "missing"
            } else {
                "made"
            }
        )),
    );
    {
        let records = layers.records(period);
        for holding in &mut evidence {
            holding.own = sessions::own_series(&records, &holding.ticker);
        }
    }
    // Each newly made session's events, from this run's data, and whether it was quiet.
    let performance = &snapshot.brokerage_performance;
    let mut accounts = reconcile::investment_accounts(performance);
    accounts.extend(ledger.trades.iter().map(|trade| trade.account_id.clone()));
    let held: BTreeSet<String> = tickers.iter().cloned().collect();
    let ingested_at = Utc::now().to_rfc3339();
    let event_sources = eventlog::Sources {
        trades: &ledger.trades,
        transactions: &ledger.transactions,
        accounts: &accounts,
        corporate_actions: &corporate_actions,
        evidence: &evidence,
        market_news: &market_news,
        closes: &prices.closes,
        volumes: &prices.volumes,
        shares: &shares,
        held: &held,
        ingested_at: &ingested_at,
    };
    let typical_day = reconcile::typical_day(performance);
    let mut fresh_events = Vec::new();
    for record in layers.filled.iter_mut().chain(layers.latest.iter_mut()) {
        let events = eventlog::session_events(record.session, Some(record), &event_sources);
        record.quiet = Some(eventlog::quiet(
            record,
            &events,
            &prices.volumes,
            typical_day,
        ));
        fresh_events.extend(events);
    }
    let saved_sessions: BTreeSet<NaiveDate> = layers
        .saved_in(period)
        .map(|record| record.session)
        .collect();
    let period_events: Vec<Event> = kept_events
        .into_iter()
        .filter(|event| saved_sessions.contains(&event.session))
        .chain(fresh_events.iter().cloned())
        .filter(|event| {
            ticker.is_none()
                || event.symbol.is_none()
                || event.symbol.as_deref() == ticker.as_deref()
        })
        .collect();
    let links = eventlog::link(&period_events);
    let worth: f64 = ledger.held.iter().map(|security| security.value).sum();
    let per_holding = if ticker.is_some() { LISTED_EVENTS } else { 1 };
    summary.events = eventlog::listed(&period_events, &links, worth, per_holding, LISTED_EVENTS);
    summary.calendar = market_calendar(&period_events);
    summary.ahead = upcoming(period.end, &event_sources);
    summary.quiet = layers
        .latest
        .first()
        .and_then(|record| record.quiet.clone());
    trace.stage(
        "events",
        "ok",
        Some(format!(
            "{} saved, {} made, {} listed",
            period_events.len() - fresh_events.len().min(period_events.len()),
            fresh_events.len(),
            summary.events.len()
        )),
    );
    let records = layers.records(period);
    let today_row = |ticker: &str| {
        layers
            .latest
            .first()
            .and_then(|record| record.holdings.iter().find(|row| row.ticker == ticker))
    };
    // Today is the latest session, from its closes; live prices count only while it trades.
    let snapshots = session_snapshots(&quotes.snapshots, &prices.closes, period.end);
    let portfolio_today = portfolio::portfolio_today(&quotes.held, &snapshots, &sectors);
    summary.today = match &ticker {
        Some(ticker) => holding_today(ticker, sectors.get(ticker).copied(), &snapshots),
        None => portfolio_today_move(&portfolio_today, &ledger.held),
    };
    // Each position is named by its largest layer in the latest session's record.
    if let Some(today) = summary.today.as_mut() {
        if let Some(ticker) = &ticker {
            today.kind = today_row(ticker).and_then(sessions::session_kind);
        }
        for position in &mut today.positions {
            position.kind = today_row(&position.ticker).and_then(sessions::session_kind);
        }
    }
    // The period reconciled, and each holding's own return over it for naming unusual moves.
    let report = reconcile::report(snapshot, &prices.closes, period.start, ticker.as_deref());
    let changes: BTreeMap<String, f64> = report
        .attribution
        .iter()
        .flat_map(|attribution| &attribution.contributions)
        .filter_map(|item| Some((item.key.clone(), item.return_pct?)))
        .collect();
    if report.statement.is_none() {
        trace.stage(
            "statement",
            "skipped",
            Some("No flows in the history".into()),
        );
    }
    summary.report = Some(report);
    if let Some(ticker) = &ticker {
        holding_period_move(
            &mut summary,
            ticker,
            &sectors,
            &records,
            &prices.closes,
            period,
        );
    } else if !records.is_empty() {
        let expected = layers.days.len() + usize::from(!layers.latest.is_empty());
        summary.period_layers = Some(sessions::period_layers(&records, expected));
    }
    trace.stage(
        "period",
        if summary.unavailable.iter().any(|item| item.part == "prices") {
            "partial"
        } else {
            "ok"
        },
        None,
    );
    summary.coverage = coverage(&evidence, &period);
    if ticker.is_none() {
        summary.notable = notable_moves(&records, &tickers, &changes);
    }
    summary.prices_as_of = summary
        .today
        .as_ref()
        .and_then(|today| today.as_of.clone())
        .or_else(|| Some(summary.session.closes_at.clone()));
    let stories: Vec<MarketNewsArticle> = evidence
        .iter()
        .flat_map(|holding| holding.articles.iter().cloned())
        .chain(market_news.iter().cloned())
        .collect();
    summary.brief = summary.today.as_ref().map(|today| {
        day_brief(
            today,
            ticker.as_deref(),
            &ledger.held,
            &stories,
            &summary.session,
            period.end,
        )
    });
    trace.stage(
        "brief",
        if summary.brief.is_some() {
            "ok"
        } else {
            "skipped"
        },
        summary
            .brief
            .as_ref()
            .map(|brief| format!("{} headlines", brief.headlines.len())),
    );
    // Closed sessions are saved for the portfolio with their events; today's waits for the
    // close.
    let latest_closed =
        period.end < summary.session.date || summary.session.state != MarketState::Open;
    let closed = if ticker.is_none() {
        layers.closed(latest_closed, fresh_events)
    } else {
        Closed::default()
    };
    Prepared {
        summary,
        closed,
        held: ledger.held,
    }
}

/// The period's sessions in layers: saved ones as they were, the rest filled from closes, and
/// the latest session from its closes (or live prices while it trades).
struct Layers {
    saved: BTreeMap<NaiveDate, SessionRecord>,
    /// The market's sessions inside the period, before the latest.
    days: Vec<NaiveDate>,
    filled: Vec<SessionRecord>,
    latest: Vec<SessionRecord>,
}

impl Layers {
    fn new(
        kept: Vec<SessionRecord>,
        tickers: &[String],
        inputs: &sessions::Inputs,
        period: Period,
    ) -> Self {
        let saved: BTreeMap<NaiveDate, SessionRecord> = kept
            .into_iter()
            .filter(|record| record.method == sessions::METHOD)
            .map(|record| (record.session, record))
            .collect();
        let days: Vec<NaiveDate> = inputs
            .returns
            .get(MARKET_FUND.0)
            .map(|market| {
                market
                    .range(period.start.succ_opt().unwrap_or(period.start)..period.end)
                    .map(|(day, _)| *day)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            filled: sessions::backfill(&days, tickers, inputs, &saved),
            latest: sessions::backfill(&[period.end], tickers, inputs, &BTreeMap::new()),
            saved,
            days,
        }
    }

    /// Saved sessions strictly inside the period; the latest is always made fresh.
    fn saved_in(&self, period: Period) -> impl Iterator<Item = &SessionRecord> {
        self.saved
            .values()
            .filter(move |record| record.session > period.start && record.session < period.end)
    }

    /// Every session in the period, oldest layer first.
    fn records(&self, period: Period) -> Vec<&SessionRecord> {
        self.saved_in(period)
            .chain(&self.filled)
            .chain(&self.latest)
            .collect()
    }

    /// The sessions made this run that are over, with their events, to save.
    fn closed(self, latest_closed: bool, fresh_events: Vec<Event>) -> Closed {
        let sessions: Vec<SessionRecord> = self
            .filled
            .into_iter()
            .chain(self.latest.into_iter().filter(|_| latest_closed))
            .collect();
        let days: BTreeSet<NaiveDate> = sessions.iter().map(|record| record.session).collect();
        Closed {
            events: fresh_events
                .into_iter()
                .filter(|event| days.contains(&event.session))
                .collect(),
            sessions,
        }
    }
}

/// The period's scheduled macro releases.
fn market_calendar(events: &[Event]) -> Vec<MarketDay> {
    events
        .iter()
        .filter(|event| event.family == eventlog::Family::Macro)
        .map(|event| MarketDay {
            session: event.session,
            kind: event.kind.clone(),
            title: event
                .source
                .as_ref()
                .map_or_else(String::new, |source| source.title.clone()),
            at: event.at.clone(),
            url: event.source.as_ref().and_then(|source| source.url.clone()),
        })
        .collect()
}
