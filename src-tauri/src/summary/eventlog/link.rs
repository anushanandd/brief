//! How a period's events relate: precedence for a fact reported twice, chains, and clusters.
use super::*;

/// How each event relates to the others, worked out over a period: which stand for a fact
/// reported twice, which belong to one chain or cluster, which are the first report.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    /// The event that stands for this one's fact: a ledger credit for an expected dividend, an
    /// action or filing for news about it.
    pub superseded_by: BTreeMap<String, String>,
    /// A statistical flag attached to the sourced or reported event it measures.
    pub evidence_for: BTreeMap<String, String>,
    pub chain: BTreeMap<String, String>,
    pub cluster: BTreeMap<String, String>,
    /// News that was the first report of its cluster.
    pub first: BTreeSet<String>,
    /// Reports a later amendment revised; the amendment folds into what it amends.
    pub revised: BTreeSet<String>,
}

pub(super) fn shingles(text: &str) -> BTreeSet<String> {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    words.windows(3).map(|window| window.join(" ")).collect()
}

pub(super) fn instant(event: &Event) -> Option<DateTime<Utc>> {
    event.at.as_deref().and_then(session::instant)
}

/// The quarter a 2.02 filed on `filed` reports: the calendar quarter before.
pub(super) fn reported_quarter(filed: NaiveDate) -> String {
    use chrono::Datelike;
    let (year, quarter) = match (filed.month() - 1) / 3 {
        0 => (filed.year() - 1, 4),
        index => (filed.year(), index),
    };
    format!("{year}Q{quarter}")
}

/// Works out precedence, chains, and clusters over `events`, in time order.
pub(in crate::summary) fn link(events: &[Event]) -> Links {
    let mut links = Links::default();
    let mut ordered: Vec<&Event> = events.iter().collect();
    ordered.sort_by(|a, b| {
        a.session
            .cmp(&b.session)
            .then(instant(a).cmp(&instant(b)))
            .then(a.id.cmp(&b.id))
    });
    cluster_news(&ordered, &mut links);
    chain_disclosures(&ordered, &mut links);
    credit_dividends(&ordered, &mut links);
    supersede_news(&ordered, &mut links);
    attach_flags(&ordered, &mut links);
    links
}

/// A filing's item codes, such as 2.02 for earnings.
fn filing_codes(event: &Event) -> Vec<String> {
    event
        .subtype
        .as_deref()
        .map(|codes| codes.split(',').map(str::to_owned).collect())
        .unwrap_or_default()
}

/// News: per company (or the market) per day, alike headlines are one cluster; the first
/// report anchors it.
fn cluster_news(ordered: &[&Event], links: &mut Links) {
    let mut anchors: Vec<(&Event, BTreeSet<String>)> = Vec::new();
    for event in ordered.iter().filter(|event| event.family == Family::News) {
        let title = event
            .source
            .as_ref()
            .map_or("", |source| source.title.as_str());
        let own = shingles(title);
        let anchor = anchors.iter().find(|(anchor, words)| {
            anchor.symbol == event.symbol
                && instant(event)
                    .zip(instant(anchor))
                    .is_some_and(|(at, then)| at - then <= Duration::hours(24))
                && {
                    let shared = own.intersection(words).count() as f64;
                    let all = own.union(words).count() as f64;
                    all > 0.0 && shared / all >= SAME_STORY
                }
        });
        match anchor {
            Some((anchor, _)) => {
                links.cluster.insert(event.id.clone(), anchor.id.clone());
            }
            None => {
                links.cluster.insert(event.id.clone(), event.id.clone());
                links.first.insert(event.id.clone());
                anchors.push((event, own));
            }
        }
    }
}

/// Disclosures: amendments revise what they amend; earnings and deals chain.
fn chain_disclosures(ordered: &[&Event], links: &mut Links) {
    let mut deals: BTreeMap<String, (NaiveDate, String)> = BTreeMap::new();
    let mut earnings: BTreeMap<String, (NaiveDate, String)> = BTreeMap::new();
    for event in ordered
        .iter()
        .filter(|event| event.family == Family::Disclosure)
    {
        let symbol = event.symbol.clone().unwrap_or_default();
        let codes = filing_codes(event);
        if event.kind.ends_with("/A") {
            let base = event.kind.trim_end_matches("/A");
            if let Some(original) = ordered.iter().rev().find(|other| {
                other.family == Family::Disclosure
                    && other.symbol == event.symbol
                    && other.kind == base
                    && other.session <= event.session
                    && (event.session - other.session).num_days() <= 90
            }) {
                links.revised.insert(original.id.clone());
                links
                    .superseded_by
                    .insert(event.id.clone(), original.id.clone());
                if let Some(chain) = links.chain.get(&original.id).cloned() {
                    links.chain.insert(event.id.clone(), chain);
                }
            }
            continue;
        }
        if codes.iter().any(|code| code == "2.02") {
            let chain = format!("earnings:{symbol}:{}", reported_quarter(event.session));
            earnings.insert(symbol.clone(), (event.session, chain.clone()));
            links.chain.insert(event.id.clone(), chain);
        } else if event.kind == "10-Q" || event.kind == "10-K" {
            if let Some((started, chain)) = earnings.get(&symbol) {
                if (event.session - *started).num_days() <= 60 {
                    links.chain.insert(event.id.clone(), chain.clone());
                }
            }
        }
        if codes.iter().any(|code| code == "1.01") {
            let chain = format!("deal:{symbol}:{}", event.id.trim_start_matches("filing:"));
            deals.insert(symbol.clone(), (event.session, chain.clone()));
            links.chain.insert(event.id.clone(), chain);
        } else if codes
            .iter()
            .any(|code| matches!(code.as_str(), "5.07" | "2.01" | "3.01"))
        {
            if let Some((started, chain)) = deals.get(&symbol) {
                if (event.session - *started).num_days() <= 365 {
                    links.chain.insert(event.id.clone(), chain.clone());
                }
            }
        }
    }
}

/// The ledger's credit beats the action that expected it.
fn credit_dividends(ordered: &[&Event], links: &mut Links) {
    for action in ordered
        .iter()
        .filter(|event| event.family == Family::CorporateAction && event.kind == "cashDividend")
    {
        let until = action
            .dates
            .payable
            .as_deref()
            .and_then(session::date_of)
            .unwrap_or(action.session)
            + Duration::days(7);
        if let Some(credit) = ordered.iter().find(|event| {
            event.family == Family::Ledger
                && matches!(event.kind.as_str(), "dividend" | "reinvestment")
                && event.symbol == action.symbol
                && event.session >= action.session
                && event.session <= until
        }) {
            links
                .superseded_by
                .insert(action.id.clone(), credit.id.clone());
        }
    }
}

/// An action beats news about it, and a filing beats news about the same event.
fn supersede_news(ordered: &[&Event], links: &mut Links) {
    let category_of_codes = |codes: &[String]| -> Option<&'static str> {
        codes.iter().find_map(|code| match code.as_str() {
            "2.02" => Some("earnings"),
            "5.02" => Some("leadership"),
            "1.01" | "1.02" | "2.01" | "5.01" => Some("deal"),
            _ => None,
        })
    };
    for story in ordered.iter().filter(|event| event.family == Family::News) {
        let Some(kind) = story.subtype.as_deref() else {
            continue;
        };
        let near = |other: &Event| {
            other.symbol == story.symbol
                && story.symbol.is_some()
                && (story.session - other.session).num_days().abs() <= 3
        };
        let action = ordered.iter().find(|other| {
            other.family == Family::CorporateAction
                && near(other)
                && match other.kind.as_str() {
                    "cashDividend" => kind == "dividend",
                    "split" => kind == "split",
                    _ => false,
                }
        });
        let filing = ordered.iter().find(|other| {
            other.family == Family::Disclosure
                && near(other)
                && category_of_codes(&filing_codes(other)) == Some(kind)
        });
        if let Some(winner) = action.or(filing) {
            links
                .superseded_by
                .insert(story.id.clone(), winner.id.clone());
        }
    }
}

/// A flag never duplicates a sourced or reported event; it is evidence for it.
fn attach_flags(ordered: &[&Event], links: &mut Links) {
    for flag in ordered
        .iter()
        .filter(|event| event.family == Family::Statistical)
    {
        if let Some(event) = ordered.iter().find(|other| {
            matches!(
                other.family,
                Family::CorporateAction | Family::Disclosure | Family::News
            ) && other.symbol == flag.symbol
                && other.session == flag.session
                && !links.superseded_by.contains_key(&other.id)
                && other.subtype.as_deref() != Some("explainer")
        }) {
            links.evidence_for.insert(flag.id.clone(), event.id.clone());
        }
    }
}
