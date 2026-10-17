//! Headlines that explain the day, quoted as published and credited. Brief never works out a
//! cause itself: an editor's headline that names a holding and says it moved the way it did
//! ("Nvidia, Oracle and other AI stocks sink on OpenAI revenue report"), or a market wrap that
//! says the market moved the way it did, is shown with its publisher, time, and link. A story
//! about a holding that points the other way, or names none of the day's movers, is left out.
use super::*;
use crate::providers::feeds;
use std::sync::LazyLock;

/// Headlines shown, at most.
pub(super) const HEADLINES: usize = 2;
/// The market's day gets a headline only when it moved at least this much.
const MARKET_MOVE_PCT: f64 = 0.3;
/// A headline that doesn't say which way a stock moved stands for it only when it is about a
/// company or two, not a broad theme.
const SPECIFIC_TICKERS: usize = 2;
/// Two headlines sharing this share of their three-word phrases are one story told twice.
const SAME_STORY: f64 = 0.6;

static RISING: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"\b(rises?|rising|rose|jumps?|jumped|surges?|surged|climbs?|climbed|spikes?|spiked|soars?|soared|rall(y|ies|ied)|gains?|higher|tops|rebounds?|rebounded|hits? .{0,20}high)\b",
    )
});
static FALLING: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"\b(falls?|fell|falling|drops?|dropped|slides?|slid|slips?|slipped|slumps?|tumbles?|tumbled|sinks?|sank|plunges?|plunged|dips?|dipped|sags?|lower|retreats?|declines?|declined|sell-?off)\b",
    )
});
/// Reporters' accounts of the whole market's session.
static ROUNDUP: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"stock market today|market (update|wrap|recap)|live updates|\b(stocks|u\.s\. stocks|us stocks|dow|nasdaq|s&p 500|s&p|equity indexes|wall street)\b",
    )
});
/// Filler that says a stock moved without saying why ("Why Is X Stock Falling?").
static FILLER: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"\bwhy\b.{0,80}\b(stocks?|shares)\b|what[’']?s going on|\b(stock|shares) (is|are) (trading|moving) (higher|lower)|\b(stock|shares) (is|are) (falling|rising|sliding|soaring|surging|tumbling|plunging|climbing|sinking|down|up)\b|\bshares are (soaring|surging|sliding|falling|tumbling|rising|jumping|plunging|climbing|sinking)|\bmovers\b|\bmid-?day\b|\bwhale",
    )
});

/// A headline that says a stock moved without saying why.
pub(super) fn filler(headline: &str) -> bool {
    FILLER.is_match(headline)
}

/// A reporter's market wrap.
pub(super) fn is_roundup(headline: &str) -> bool {
    ROUNDUP.is_match(headline)
}

/// Two headlines that are one story told twice: most of their three-word phrases shared.
pub(crate) fn same_story(a: &str, b: &str) -> bool {
    let (a, b) = (shingles(a), shingles(b));
    let shared = a.intersection(&b).count() as f64;
    let all = a.union(&b).count() as f64;
    all > 0.0 && shared / all >= SAME_STORY
}

fn shingles(text: &str) -> BTreeSet<String> {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    if words.len() < 3 {
        return words.into_iter().collect();
    }
    words.windows(3).map(|window| window.join(" ")).collect()
}

/// How a headline fits a move: it says the move's way (2), says nothing of a way (1), or says
/// only the other way (`None`).
fn fits(headline: &str, change_pct: f64) -> Option<u8> {
    let (rising, falling) = (RISING.is_match(headline), FALLING.is_match(headline));
    let (same, other) = if change_pct >= 0.0 {
        (rising, falling)
    } else {
        (falling, rising)
    };
    if same {
        Some(2)
    } else if other {
        None
    } else {
        Some(1)
    }
}

/// A headline shown beside the day's figures, as published.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    /// The holdings it names among the day's movers, or none for the market's.
    pub tickers: Vec<String>,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub published_at: String,
}

impl Headline {
    fn of(story: &MarketNewsArticle, tickers: Vec<String>) -> Self {
        Self {
            tickers,
            title: story.headline.clone(),
            publisher: story.source.clone(),
            url: story.url.clone(),
            published_at: story.created_at.clone(),
        }
    }
}

/// Up to two headlines for the day: first for the movers, in order, each naming one and saying
/// it moved its way (or, about one or two companies, saying no way); then a market wrap saying
/// the market moved its way. `names` are (name, ticker) pairs as headlines write them.
pub(super) fn pick(
    stories: &[MarketNewsArticle],
    names: &[(String, String)],
    movers: &[(String, f64)],
    market_pct: Option<f64>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Vec<Headline> {
    let published = |story: &MarketNewsArticle| {
        DateTime::parse_from_rfc3339(&story.created_at)
            .ok()
            .map(|at| at.with_timezone(&Utc))
    };
    let mut pool: Vec<(&MarketNewsArticle, DateTime<Utc>)> = Vec::new();
    for story in stories {
        let Some(at) = published(story).filter(|at| *at >= since && *at <= until) else {
            continue;
        };
        if filler(&story.headline)
            || story.tags.contains(&NewsTag::Explainer)
            || story.url.to_ascii_lowercase().contains("/opinion/")
            || pool.iter().any(|(kept, _)| {
                kept.url == story.url || same_story(&kept.headline, &story.headline)
            })
        {
            continue;
        }
        pool.push((story, at));
    }
    // Newest first: the later account of a day usually says why.
    pool.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    let mut picked: Vec<Headline> = Vec::new();
    for (ticker, change_pct) in movers {
        if picked.len() == HEADLINES {
            break;
        }
        if picked
            .iter()
            .any(|headline| headline.tickers.contains(ticker))
        {
            continue;
        }
        let candidates: Vec<(u8, &MarketNewsArticle, Vec<String>)> = pool
            .iter()
            .filter(|(story, _)| !picked.iter().any(|headline| headline.url == story.url))
            .filter_map(|(story, _)| {
                let named = feeds::named_tickers(&story.headline, names);
                if !named.contains(ticker) {
                    return None;
                }
                let fit = fits(&story.headline, *change_pct)?;
                (fit == 2 || named.len() <= SPECIFIC_TICKERS).then_some((fit, *story, named))
            })
            .collect();
        // The newest that says the move's way, else the newest about the company.
        let best = candidates
            .iter()
            .find(|(fit, ..)| *fit == 2)
            .or(candidates.first());
        if let Some((_, story, named)) = best {
            let tickers = movers
                .iter()
                .map(|(ticker, _)| ticker)
                .filter(|ticker| named.contains(ticker))
                .cloned()
                .collect();
            picked.push(Headline::of(story, tickers));
        }
    }
    if let Some(market_pct) = market_pct.filter(|pct| pct.abs() >= MARKET_MOVE_PCT) {
        if picked.len() < HEADLINES {
            if let Some((story, _)) = pool.iter().find(|(story, _)| {
                is_roundup(&story.headline)
                    && fits(&story.headline, market_pct) == Some(2)
                    && !picked.iter().any(|headline| headline.url == story.url)
            }) {
                picked.push(Headline::of(story, Vec::new()));
            }
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(headline: &str, at: &str, source: &str) -> MarketNewsArticle {
        MarketNewsArticle {
            headline: headline.into(),
            summary: String::new(),
            source: source.into(),
            url: format!("https://example.com/{}", headline.len()),
            created_at: at.into(),
            symbols: Vec::new(),
            scope: NewsScope::Market,
            tags: Vec::new(),
            relevance: 0,
        }
    }

    fn at(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    #[test]
    fn quotes_the_headline_that_names_a_mover_and_its_way_and_the_market_wrap() {
        let names = vec![
            ("Nvidia".to_owned(), "NVDA".to_owned()),
            ("Nebius".to_owned(), "NBIS".to_owned()),
        ];
        let stories = vec![
            // Good news for AI names on a day they fell, naming none of them: never shown.
            story(
                "Trump to Unveil $2.4B in AI Pledges to Boost Research",
                "2026-10-08T13:00:00Z",
                "Bloomberg",
            ),
            // Says Nvidia rose on a day it fell: left out.
            story(
                "Nvidia rises premarket on supply deal",
                "2026-10-08T11:00:00Z",
                "Benzinga",
            ),
            story(
                "Why Is Nvidia Stock Falling Today?",
                "2026-10-08T17:00:00Z",
                "Benzinga",
            ),
            story(
                "Nvidia, Oracle and other AI stocks sink on OpenAI revenue report",
                "2026-10-08T18:12:00Z",
                "CNBC",
            ),
            story(
                "Stocks fall as oil prices jump",
                "2026-10-08T20:30:00Z",
                "CNBC",
            ),
            // Yesterday's: before the session's window.
            story("Stocks slide on jobs data", "2026-10-07T15:00:00Z", "CNBC"),
        ];
        let picked = pick(
            &stories,
            &names,
            &[("NVDA".into(), -2.1), ("NBIS".into(), -3.4)],
            Some(-0.7),
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        let titles: Vec<&str> = picked.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Nvidia, Oracle and other AI stocks sink on OpenAI revenue report",
                "Stocks fall as oil prices jump"
            ]
        );
        assert_eq!(picked[0].tickers, ["NVDA"]);
        assert!(picked[1].tickers.is_empty());
        assert_eq!(picked[0].publisher, "CNBC");
    }

    #[test]
    fn a_quiet_market_or_a_headline_the_other_way_gets_nothing() {
        let names = vec![("Apple".to_owned(), "AAPL".to_owned())];
        let stories = vec![
            story(
                "Apple jumps on record iPhone orders",
                "2026-10-08T15:00:00Z",
                "CNBC",
            ),
            story(
                "Stocks edge lower in quiet trade",
                "2026-10-08T20:30:00Z",
                "CNBC",
            ),
        ];
        let picked = pick(
            &stories,
            &names,
            &[("AAPL".into(), -1.5)],
            Some(-0.1),
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        assert!(picked.is_empty());
        // A plain company headline, about Apple alone, stands for its move either way.
        let plain = vec![story(
            "Apple names a new chief operating officer",
            "2026-10-08T15:00:00Z",
            "Benzinga",
        )];
        let picked = pick(
            &plain,
            &names,
            &[("AAPL".into(), -1.5)],
            None,
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        assert_eq!(picked.len(), 1);
    }
}
