//! The investment summary as the renderer receives it. Rust writes every figure and sentence;
//! the renderer only styles them.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentSummary {
    pub start: NaiveDate,
    /// The latest session that had opened when the summary was made.
    pub end: NaiveDate,
    /// The session the summary describes and where the market stood: outside regular hours,
    /// today is the latest session's close, never a pre-market or after-hours price.
    pub session: SessionClock,
    /// When the prices it describes were observed, and when it was written: two times, since
    /// a narrative can be written well after its prices.
    pub prices_as_of: Option<String>,
    pub generated_at: String,
    pub ticker: Option<String>,
    /// The period reconciled: the statement identity, two labeled returns, the waterfall, risk,
    /// and the return split among the holdings (on a holding's page, it and the rest).
    pub report: Option<PeriodReport>,
    /// Holdings whose period was unusual for them, largest first; the portfolio only.
    pub notable: Vec<NotableMove>,
    /// A holding's total return against its sector fund and the S&P 500, with its layers over
    /// the period; `None` for the portfolio or without prices.
    pub period_move: Option<PeriodMove>,
    /// The portfolio's period in layers: each session's market, group, and own dollars summed;
    /// `None` for a holding.
    pub period_layers: Option<PeriodLayers>,
    pub activity: Activity,
    /// The period's events from the event log, most consequential first by visible
    /// components: each fact once, with its provenance and three timestamps.
    pub events: Vec<ListedEvent>,
    /// Market-wide days in the period (a Federal Reserve decision, an index rebalance, an early
    /// close), said once for the portfolio.
    pub calendar: Vec<MarketDay>,
    /// Scheduled events for the holdings and the market in the month ahead.
    pub ahead: Vec<Upcoming>,
    /// Whether the latest session was quiet for the portfolio and which holdings were.
    pub quiet: Option<Quiet>,
    pub fundamentals: Option<Fundamentals>,
    /// A holding's open-market insider trades in the period, newest first.
    pub insiders: Vec<InsiderTrade>,
    /// The latest session's move: the portfolio's, with the holdings that moved it, or the
    /// holding's against its benchmarks; `None` without live prices.
    pub today: Option<TodayMove>,
    /// Each stock's relevant stories in the period, for the holdings list.
    pub coverage: Vec<Coverage>,
    /// The latest session in a few lines and the headlines that explain it; `None` without
    /// its prices.
    pub brief: Option<Brief>,
    /// Parts that could not be loaded or were left out, each with the symbols affected.
    pub unavailable: Vec<Unavailable>,
}

/// A part of the summary that could not be loaded or was left out, never silently.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unavailable {
    /// `prices` (no latest or start prices), `fundamentals`, or `history` (a year of daily
    /// closes cut short by the request's page limit).
    pub part: &'static str,
    /// The symbols affected; empty when the whole part failed.
    pub symbols: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayMove {
    /// When the prices were observed; a weekend shows the last session.
    pub as_of: Option<String>,
    pub change_pct: Option<f64>,
    /// The portfolio's dollar change; `None` for a holding.
    pub change: Option<f64>,
    pub market: Option<Benchmark>,
    /// A holding's sector fund; `None` for the portfolio.
    pub sector: Option<Benchmark>,
    /// A holding's move against its benchmarks, as on its page; `None` for the portfolio.
    pub kind: Option<&'static str>,
    /// The holdings that moved the portfolio, largest dollar effect first.
    pub positions: Vec<TodayPosition>,
    /// The portfolio's largest sectors and how its holdings in each moved.
    pub sectors: Vec<SectorToday>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorToday {
    pub name: String,
    /// The sector's share of the portfolio's value.
    pub weight_pct: f64,
    /// How the portfolio's holdings in the sector moved together.
    pub change_pct: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayPosition {
    pub ticker: String,
    pub change_pct: f64,
    pub change: f64,
    /// Its largest layer today; `None` without a fit.
    pub kind: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub ticker: String,
    /// Stories at least `RELEVANT` to the company in the period.
    pub stories: usize,
    pub top_relevance: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotableMove {
    pub ticker: String,
    pub change_pct: Option<f64>,
    /// Its layers over the period; the own part was unusual for it.
    pub layers: HoldingPeriod,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodMove {
    pub change_pct: f64,
    pub market: Option<Benchmark>,
    pub sector: Option<Benchmark>,
    /// Its market, group, and own layers summed over the period's sessions, with the fit behind
    /// them; `None` without a year of prices.
    pub layers: Option<HoldingPeriod>,
    /// `quiet`, `withSector`, `withMarket`, or `ownMove`, by those layers.
    pub kind: Option<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub buys: usize,
    pub sells: usize,
    /// The newest trades, newest first.
    pub trades: Vec<TradeFact>,
    /// Estimated gain on the period's sales; `None` without sales or when a sale has no
    /// known cost.
    pub realized: Option<f64>,
    pub dividends: f64,
    pub dividend_payments: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFact {
    pub date: String,
    /// `buy` or `sell`.
    pub side: &'static str,
    pub ticker: String,
    pub units: Option<f64>,
    pub amount: f64,
    pub realized_gain: Option<f64>,
}

/// A market-wide event in the period.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketDay {
    pub session: NaiveDate,
    /// `fomc`, `cpi`, `employment`, `earlyClose`, or `indexRebalance`.
    pub kind: String,
    pub title: String,
    pub at: Option<String>,
    pub url: Option<String>,
}

/// Something scheduled ahead, with whether it is confirmed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upcoming {
    pub date: NaiveDate,
    /// `cashDividend`, `split`, `fomc`, `cpi`, `employment`, `earlyClose`, `indexRebalance`, or
    /// `holiday`.
    pub kind: String,
    pub ticker: Option<String>,
    pub title: String,
    /// `scheduled` from a calendar or rule, `confirmed` from an announced corporate action.
    pub status: eventlog::Status,
    /// A dividend's expected amount for the shares held.
    pub amount: Option<f64>,
    pub url: Option<String>,
}

impl InvestmentSummary {
    /// An empty summary of `period`, filled in as each part is computed.
    pub(super) fn new(period: Period, ticker: Option<String>, session: SessionClock) -> Self {
        Self {
            start: period.start,
            end: period.end,
            session,
            prices_as_of: None,
            generated_at: Utc::now().to_rfc3339(),
            ticker,
            report: None,
            notable: Vec::new(),
            period_move: None,
            period_layers: None,
            activity: Activity::default(),
            events: Vec::new(),
            fundamentals: None,
            insiders: Vec::new(),
            today: None,
            coverage: Vec::new(),
            brief: None,
            calendar: Vec::new(),
            ahead: Vec::new(),
            quiet: None,
            unavailable: Vec::new(),
        }
    }

    /// Notes a part that could not be loaded at all, once.
    pub(super) fn missing(&mut self, part: &'static str) {
        self.missing_for(part, Vec::new());
    }

    /// Notes symbols left out of a part; a part noted for every symbol stays so.
    pub(super) fn missing_for(&mut self, part: &'static str, symbols: Vec<String>) {
        match self.unavailable.iter_mut().find(|item| item.part == part) {
            Some(item) if item.symbols.is_empty() => {}
            Some(_) if symbols.is_empty() => {
                self.unavailable.retain(|item| item.part != part);
                self.unavailable.push(Unavailable { part, symbols });
            }
            Some(item) => {
                for symbol in symbols {
                    if !item.symbols.contains(&symbol) {
                        item.symbols.push(symbol);
                    }
                }
            }
            None => self.unavailable.push(Unavailable { part, symbols }),
        }
    }
}
