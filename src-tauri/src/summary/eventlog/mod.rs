//! The event log: what happened to the portfolio, session by session, in six families with
//! where each fact came from. It explains the market-change term and nothing else.
//!
//! - Ledger: the user's trades, dividends, interest, fees, taxes, and transfers, read from the
//!   snapshot. Paired kinds (deposit and withdrawal, buy and sell) mean a reversal never needs a
//!   kind of its own.
//! - Corporate actions: splits and cash dividends from Alpaca's daily read, committed with the
//!   refresh. The ledger's credit is the truth; an action is the expectation.
//! - Disclosures: EDGAR current and periodic reports, keyed by accession number, with 8-K items
//!   as subtypes, amendments marked revised, and earnings and deal chains.
//! - Macro and market structure: the Federal Reserve's decisions and BLS releases from a
//!   reviewed calendar (`macro-calendar.json`), NYSE early closes, and S&P 500 rebalances by
//!   their published rule. They belong to the portfolio, so a decision day is said once.
//! - News: saved stories, clustered per company per day so a rewrite is never news, with the
//!   first report anchoring its cluster and each publisher's model-input class.
//! - Statistical: a holding's unusual own move, volume, 52-week extreme, or drawdown crossing,
//!   computed from closes. Always observed, never a cause.
//!
//! Records are built from the session's data and saved as built; which event stands for a fact
//! reported twice (precedence), chains, and clusters are worked out when a period is read.
use super::sessions::SessionRecord;
use super::*;
use crate::finance_contract::Transaction;
use crate::insights::reconcile::Shares;
use crate::numeric::round2;
use crate::providers::CorporateAction;
use crate::session::{self, Session};
use chrono::NaiveTime;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// A story or filing to open beside an event.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StoryLink {
    pub title: String,
    pub url: String,
    pub publisher: String,
    /// A story's relevance to the company; `None` for a filing.
    pub relevance: Option<u8>,
    /// When the source published it.
    #[serde(default)]
    pub published_at: Option<String>,
}

/// A holding's own move this many of its typical days is flagged.
pub(super) const ABNORMAL_Z: f64 = 2.5;
/// Volume this many times its median over the prior sessions is flagged.
const VOLUME_RATIO: f64 = 2.0;
const VOLUME_SESSIONS: usize = 20;
/// Sessions in a 52-week window, and the fewest that can say what an extreme is.
const YEAR_SESSIONS: usize = 252;
const MIN_YEAR_SESSIONS: usize = 200;
/// Falls from the 52-week high marked as they are crossed.
const DRAWDOWNS: [f64; 3] = [5.0, 10.0, 20.0];
/// Stories about one company this alike, within a day, are one story.
const SAME_STORY: f64 = 0.6;
/// A holding is quiet with its own move under this many typical days and ordinary volume.
pub(super) const QUIET_Z: f64 = 1.5;
const QUIET_VOLUME: f64 = 1.5;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    Ledger,
    CorporateAction,
    Disclosure,
    Macro,
    News,
    Statistical,
}

impl Family {
    /// The name it is saved under, as it serializes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ledger => "ledger",
            Self::CorporateAction => "corporateAction",
            Self::Disclosure => "disclosure",
            Self::Macro => "macro",
            Self::News => "news",
            Self::Statistical => "statistical",
        }
    }
}

/// Where a fact came from: a record of it, a measurement, or a report about it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "EventProvenance")]
pub enum Provenance {
    Sourced,
    Observed,
    Reported,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Scheduled,
    Confirmed,
    Estimated,
    Revised,
    Retracted,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Precision {
    Exact,
    Date,
    Estimated,
}

/// What a source may contribute to the on-device model: green may be read, amber only typed
/// by its headline, red never.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum InputClass {
    Green,
    Amber,
    Red,
}

/// A publisher's model-input class.
pub fn input_class(publisher: &str) -> InputClass {
    let publisher = publisher.to_lowercase();
    if [
        "dow jones",
        "wall street journal",
        "wsj",
        "barron",
        "bloomberg",
        "financial times",
    ]
    .iter()
    .any(|name| publisher.contains(name))
    {
        InputClass::Red
    } else if ["cnbc", "yahoo"]
        .iter()
        .any(|name| publisher.contains(name))
    {
        InputClass::Amber
    } else {
        InputClass::Green
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventDates {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub announced: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub payable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effective: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filed: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventSource {
    /// `alpaca`, `cnbc`, `sec`, `snapshot`, `federalReserve`, `bls`, `nyse`, `spdji`, or
    /// `brief` for a measurement.
    pub provider: String,
    pub provider_id: Option<String>,
    pub url: Option<String>,
    pub title: String,
    pub publisher: String,
    /// When the source published it, as distinct from the event's own time and when Brief
    /// first saved it.
    pub published_at: Option<String>,
    pub input_class: InputClass,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "LoggedEvent")]
pub struct Event {
    /// Stable for its fact: an accession number, a provider's id, a transaction's id, or the
    /// symbol, kind, and session of a measurement, so building a session again changes nothing.
    pub id: String,
    pub family: Family,
    pub kind: String,
    pub subtype: Option<String>,
    pub symbol: Option<String>,
    pub account_id: Option<String>,
    /// The session whose prices it belongs to.
    pub session: NaiveDate,
    /// When it happened, as precisely as known.
    pub at: Option<String>,
    pub precision: Precision,
    pub dates: EventDates,
    pub provenance: Provenance,
    pub status: Status,
    pub source: Option<EventSource>,
    /// Kind-specific figures: shares, rates, ratios.
    pub quantities: Value,
    /// Its effect on the portfolio in dollars, when known.
    pub effect: Option<f64>,
    /// The holding's own move that session in its typical days, when measured.
    pub z: Option<f64>,
    pub ingested_at: String,
}

/// A story's event id, from its address.
pub(super) fn news_id(url: &str) -> String {
    format!("news:{}", short_hash(url))
}

fn short_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// What one session's events are built from.
pub(super) struct Sources<'a> {
    pub trades: &'a [Trade],
    pub transactions: &'a [Transaction],
    /// The investment accounts the ledger's events come from.
    pub accounts: &'a BTreeSet<String>,
    pub corporate_actions: &'a [CorporateAction],
    pub evidence: &'a [Evidence],
    pub market_news: &'a [MarketNewsArticle],
    pub closes: &'a DailyCloses,
    pub volumes: &'a BTreeMap<String, BTreeMap<NaiveDate, f64>>,
    pub shares: &'a BTreeMap<String, Shares>,
    pub held: &'a BTreeSet<String>,
    pub ingested_at: &'a str,
}

/// The session a dated or timed fact belongs to: an instant is priced by the first session
/// that closes after it; a posted date by its own session or the next.
fn session_of(value: &str) -> Option<NaiveDate> {
    reacting_session(value)
}

fn event(id: String, family: Family, kind: &str, session: NaiveDate, sources: &Sources) -> Event {
    let (provenance, precision) = match family {
        Family::Statistical => (Provenance::Observed, Precision::Date),
        Family::News => (Provenance::Reported, Precision::Exact),
        _ => (Provenance::Sourced, Precision::Date),
    };
    Event {
        id,
        family,
        kind: kind.into(),
        subtype: None,
        symbol: None,
        account_id: None,
        session,
        at: None,
        precision,
        dates: EventDates::default(),
        provenance,
        status: Status::Confirmed,
        source: None,
        quantities: Value::Null,
        effect: None,
        z: None,
        ingested_at: sources.ingested_at.into(),
    }
}

mod families;
mod link;
mod rank;

use families::*;
pub(super) use families::{
    action_dollars, action_kind, macro_events, scheduled_macro, volume_ratio,
};
pub(super) use link::link;
pub use link::Links;
pub(super) use rank::rank;
pub use rank::Rank;

/// Every event in one session, as built from its data.
pub(super) fn session_events(
    day: NaiveDate,
    record: Option<&SessionRecord>,
    sources: &Sources,
) -> Vec<Event> {
    let mut events = ledger(day, sources);
    events.extend(corporate_actions(day, sources));
    events.extend(disclosures(day, sources));
    events.extend(macro_events(day, sources));
    events.extend(news(day, sources));
    events.extend(statistical(day, record, sources));
    // A filing or story is worth its holding's own dollars that session.
    for event in &mut events {
        if matches!(event.family, Family::Disclosure | Family::News) && event.effect.is_none() {
            event.effect = record
                .zip(event.symbol.as_ref())
                .and_then(|(record, symbol)| {
                    record.holdings.iter().find(|row| &row.ticker == symbol)
                })
                .and_then(|row| row.own);
        }
    }
    events
}

/// An event as the summary lists it: what happened, where it came from, when (its own time,
/// its source's, and Brief's first save), and the components it ranks by.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ListedEvent {
    pub id: String,
    pub family: Family,
    pub kind: String,
    pub subtype: Option<String>,
    pub ticker: Option<String>,
    pub session: NaiveDate,
    pub at: Option<String>,
    pub published_at: Option<String>,
    pub ingested_at: String,
    /// The headline or filing's title; for an observed event, what was measured, in Rust's
    /// words.
    pub title: String,
    pub url: Option<String>,
    pub publisher: String,
    /// Where it came from, in a word: the form, `Action`, `Observed`, or the publisher.
    pub source_label: String,
    /// The holding's own move around it, when that was more than ordinary.
    pub reaction: Option<String>,
    pub provenance: Provenance,
    /// `revised` when a later amendment changed it.
    pub status: Status,
    pub effect: Option<f64>,
    pub z: Option<f64>,
    pub rank: Rank,
    /// The chain it belongs to (an earnings report and its quarterly filing, a deal and its
    /// completion); only a chain's first and completing steps are listed.
    pub chain: Option<String>,
    /// Stories about the same fact that this event stands for.
    pub coverage: Vec<StoryLink>,
    /// Statistical flags measured on it, such as `unusualVolume`.
    pub flags: Vec<String>,
    pub relevance: Option<u8>,
}

/// What a measurement saw, said as observed and never as a cause.
fn observed(event: &Event) -> String {
    let z = event.z.unwrap_or(0.0);
    match event.kind.as_str() {
        "abnormalReturn" => format!(
            "Moved {:.1} typical days {} on its own (observed); no sourced event found",
            z.abs(),
            if z < 0.0 { "lower" } else { "higher" }
        ),
        "newHigh" => "A new 52-week high (observed)".into(),
        "newLow" => "A new 52-week low (observed)".into(),
        "drawdown" => format!(
            "{}% below its 52-week high (observed)",
            event.subtype.as_deref().unwrap_or("")
        ),
        "unusualVolume" => format!(
            "Traded {:.1}× its usual volume (observed)",
            event.quantities["ratio"].as_f64().unwrap_or(0.0)
        ),
        _ => "Observed".into(),
    }
}

fn story_link(event: &Event) -> Option<StoryLink> {
    let source = event.source.as_ref()?;
    Some(StoryLink {
        title: source.title.clone(),
        url: source.url.clone()?,
        publisher: source.publisher.clone(),
        relevance: event.quantities["relevance"]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok()),
        published_at: source.published_at.clone(),
    })
}

/// The events worth listing for a period, most consequential first: never the ledger (the
/// summary states it apart) or the calendar, never a fact listed twice, only a cluster's first
/// report, only a chain's ends, a flag only when nothing sourced or reported explains it, and
/// never a kind that rarely matters when the stock plainly ignored it.
pub(super) fn listed(
    events: &[Event],
    links: &Links,
    portfolio: f64,
    per_symbol: usize,
    total: usize,
) -> Vec<ListedEvent> {
    let mut chain_ends: BTreeMap<&String, (&String, &String)> = BTreeMap::new();
    let mut ordered: Vec<&Event> = events.iter().collect();
    ordered.sort_by(|a, b| a.session.cmp(&b.session).then(a.id.cmp(&b.id)));
    for event in &ordered {
        if let Some(chain) = links.chain.get(&event.id) {
            chain_ends
                .entry(chain)
                .and_modify(|(_, last)| *last = &event.id)
                .or_insert((&event.id, &event.id));
        }
    }
    let mut candidates: Vec<(Rank, &Event)> = ordered
        .into_iter()
        .filter(|event| !matches!(event.family, Family::Ledger | Family::Macro))
        .filter(|event| !links.superseded_by.contains_key(&event.id))
        .filter(|event| !links.evidence_for.contains_key(&event.id))
        .filter(|event| event.kind != "unusualVolume")
        .filter(|event| {
            !matches!(
                event.subtype.as_deref(),
                Some("explainer" | "opinion" | "roundup")
            ) || event.family != Family::News
        })
        .filter(|event| event.family != Family::News || links.first.contains(&event.id))
        .filter(|event| {
            links.chain.get(&event.id).is_none_or(|chain| {
                chain_ends
                    .get(chain)
                    .is_some_and(|(first, last)| *first == &event.id || *last == &event.id)
            })
        })
        .map(|event| (rank(event, links, portfolio), event))
        .filter(|(rank, event)| {
            !(event.z.is_some() && rank.key().0 == 0 && rank.z_bucket == 0 && rank.prior == 1)
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.0.key()
            .cmp(&a.0.key())
            .then(b.1.session.cmp(&a.1.session))
            .then(a.1.id.cmp(&b.1.id))
    });
    let mut per: BTreeMap<Option<&String>, usize> = BTreeMap::new();
    let mut listed = Vec::new();
    for (rank, event) in candidates {
        let count = per.entry(event.symbol.as_ref()).or_default();
        if *count >= per_symbol || listed.len() >= total {
            continue;
        }
        *count += 1;
        let source = event.source.as_ref();
        listed.push(ListedEvent {
            id: event.id.clone(),
            family: event.family,
            kind: event.kind.clone(),
            subtype: event.subtype.clone(),
            ticker: event.symbol.clone(),
            session: event.session,
            at: event.at.clone(),
            published_at: source.and_then(|source| source.published_at.clone()),
            ingested_at: event.ingested_at.clone(),
            title: if event.family == Family::Statistical {
                observed(event)
            } else {
                source.map_or_else(String::new, |source| source.title.clone())
            },
            url: source.and_then(|source| source.url.clone()),
            publisher: source.map_or_else(String::new, |source| source.publisher.clone()),
            source_label: match event.family {
                Family::Statistical => "Observed".into(),
                Family::Disclosure => event.kind.clone(),
                Family::CorporateAction => "Action".into(),
                _ => source.map_or_else(String::new, |source| source.publisher.clone()),
            },
            reaction: event
                .z
                .filter(|_| rank.z_bucket >= 1 && event.family != Family::Statistical)
                .map(|z| format!("{:.1}× its typical day", z.abs())),
            provenance: event.provenance,
            status: if links.revised.contains(&event.id) {
                Status::Revised
            } else {
                event.status
            },
            effect: event.effect,
            z: event.z,
            rank,
            chain: links.chain.get(&event.id).cloned(),
            coverage: events
                .iter()
                .filter(|other| links.superseded_by.get(&other.id) == Some(&event.id))
                .filter(|other| other.family == Family::News)
                .filter_map(story_link)
                .collect(),
            flags: events
                .iter()
                .filter(|other| links.evidence_for.get(&other.id) == Some(&event.id))
                .map(|other| other.kind.clone())
                .collect(),
            relevance: event.quantities["relevance"]
                .as_u64()
                .and_then(|value| u8::try_from(value).ok()),
        });
    }
    listed
}

/// Whether the session was quiet for the portfolio and for each holding: no holding flagged
/// and the day's dollars within one of the portfolio's typical days; a holding quiet with no
/// sourced event, its own move under `QUIET_Z` of its typical days, and ordinary volume.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Quiet {
    pub portfolio: bool,
    /// The portfolio's dollars that session against its typical day, in typical days.
    pub z: Option<f64>,
    pub holdings: Vec<String>,
}

pub(super) fn quiet(
    record: &SessionRecord,
    events: &[Event],
    volumes: &BTreeMap<String, BTreeMap<NaiveDate, f64>>,
    typical_day: Option<f64>,
) -> Quiet {
    let sourced = |symbol: &str| {
        events.iter().any(|event| {
            event.symbol.as_deref() == Some(symbol)
                && matches!(
                    event.family,
                    Family::Ledger | Family::CorporateAction | Family::Disclosure
                )
        })
    };
    let holdings: Vec<String> = record
        .holdings
        .iter()
        .filter(|row| {
            row.own_z.is_some_and(|z| z.abs() < QUIET_Z)
                && volume_ratio(volumes.get(&row.ticker), record.session)
                    .is_none_or(|ratio| ratio < QUIET_VOLUME)
                && !sourced(&row.ticker)
        })
        .map(|row| row.ticker.clone())
        .collect();
    let flagged = events
        .iter()
        .any(|event| event.family == Family::Statistical);
    let dollars: Option<f64> = record.holdings.iter().map(|row| row.change).sum();
    let z = dollars
        .zip(typical_day)
        .filter(|(_, typical)| *typical > 0.0)
        .map(|(dollars, typical)| round2(dollars / typical));
    Quiet {
        portfolio: !flagged && z.is_some_and(|z| z.abs() < 1.0),
        z,
        holdings,
    }
}

#[cfg(test)]
mod tests;
