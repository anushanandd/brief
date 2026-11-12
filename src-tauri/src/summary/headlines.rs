//! Headlines that explain the day, quoted as published and credited. Brief never works out a
//! cause itself: an editor's headline that names a holding and says it moved the way it did, or
//! a market wrap that says the market moved the way it did, is shown with its publisher, time, and link. A story
//! about a holding that points the other way, or names none of the day's movers, is left out.
use super::*;
use crate::providers::feeds;
use std::sync::LazyLock;

/// Holdings given a reason, at most: one per holding that led the day.
pub(super) const HEADLINES: usize = 3;
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

/// A headline shown beside the day's figures, as published, with the sentence that says why.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    /// The holdings it names among the day's movers, or none for the market's.
    pub tickers: Vec<String>,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub published_at: String,
    /// A sentence quoted from the story, as written: the one that says why the stock moved
    /// (picked from its text by a model or a rule), or the publisher's summary.
    pub quote: Option<String>,
    /// Who picked the quote: `apple`, `local:<model>`, `rule`, or `summary` for the
    /// publisher's own summary.
    pub quoted_by: Option<String>,
}

impl Headline {
    fn of(story: &MarketNewsArticle, tickers: Vec<String>) -> Self {
        Self {
            tickers,
            title: story.headline.clone(),
            publisher: story.source.clone(),
            url: story.url.clone(),
            published_at: story.created_at.clone(),
            quote: None,
            quoted_by: None,
        }
    }

    fn quoting(mut self, quote: Option<(String, String)>) -> Self {
        if let Some((quote, by)) = quote {
            self.quote = Some(quote);
            self.quoted_by = Some(by);
        }
        self
    }
}

/// The day's reasons: for each mover, in order, a story that names it and says why it moved
/// its way; and the market's wrap.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Picked {
    pub movers: Vec<Headline>,
    pub market: Option<Headline>,
}

/// A summary worth quoting: written by the publisher as a summary, not the start of a page.
fn summary_quote(story: &MarketNewsArticle) -> Option<(String, String)> {
    let summary = story.summary.trim();
    (summary.chars().count() >= 30 && !same_story(summary, &story.headline))
        .then(|| (summary.to_owned(), "summary".to_owned()))
}

/// For each mover, the story that best explains its move, and the sentence to quote. In order
/// of preference:
/// 1. a "why it moved" story whose cause agrees with the move;
/// 2. a headline naming the company and its direction (or no direction, for a story about one
///    or two companies);
/// 3. an explainer whose summary fits.
///
/// Then, when the market moved at least `MARKET_MOVE_PCT`, the newest market wrap that agrees.
/// `names` are (name, ticker) pairs; `causes` are saved readings by story link.
pub(super) fn pick(
    stories: &[MarketNewsArticle],
    names: &[(String, String)],
    movers: &[(String, f64)],
    market_pct: Option<f64>,
    causes: &BTreeMap<String, reading::Cause>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Picked {
    let published = |story: &MarketNewsArticle| {
        DateTime::parse_from_rfc3339(&story.created_at)
            .ok()
            .map(|at| at.with_timezone(&Utc))
    };
    let mut pool: Vec<(&MarketNewsArticle, DateTime<Utc>)> = Vec::new();
    let mut explainers: Vec<(&MarketNewsArticle, DateTime<Utc>)> = Vec::new();
    for story in stories {
        let Some(at) = published(story).filter(|at| *at >= since && *at <= until) else {
            continue;
        };
        let explains =
            story.tags.contains(&NewsTag::Explainer) || reading::explainer(&story.headline);
        let kept = if explains { &mut explainers } else { &mut pool };
        if (!explains && filler(&story.headline))
            || story.url.to_ascii_lowercase().contains("/opinion/")
            || kept.iter().any(|(kept, _)| {
                kept.url == story.url || same_story(&kept.headline, &story.headline)
            })
        {
            continue;
        }
        kept.push((story, at));
    }
    // Newest first: the later account of a day usually says why.
    pool.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    explainers.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    let mut picked: Vec<Headline> = Vec::new();
    let used = |picked: &[Headline], story: &MarketNewsArticle| {
        picked.iter().any(|headline| headline.url == story.url)
    };
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
        // A "why it moved" story is about the holding when it's tagged with it and its
        // headline names it; its cause must not point the other way.
        let explained = |quote: &dyn Fn(&MarketNewsArticle) -> Option<(String, String)>| {
            explainers
                .iter()
                .filter(|(story, _)| !used(&picked, story))
                .filter(|(story, _)| {
                    story.symbols.len() <= SPECIFIC_TICKERS
                        && story.symbols.contains(ticker)
                        && feeds::named_tickers(&story.headline, names).contains(ticker)
                        && fits(&story.headline, *change_pct).is_some()
                })
                .find_map(|(story, _)| {
                    let quote = quote(story)?;
                    fits(&quote.0, *change_pct).map(|_| (*story, quote))
                })
        };
        let cause = |story: &MarketNewsArticle| {
            causes
                .get(&story.url)
                .map(|cause| (cause.sentence.clone(), cause.reader.clone()))
        };
        if let Some((story, quote)) = explained(&cause) {
            picked.push(Headline::of(story, vec![ticker.clone()]).quoting(Some(quote)));
            continue;
        }
        let candidates: Vec<(u8, &MarketNewsArticle, Vec<String>)> = pool
            .iter()
            .filter(|(story, _)| !used(&picked, story))
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
            let quote =
                summary_quote(story).filter(|(quote, _)| fits(quote, *change_pct).is_some());
            picked.push(Headline::of(story, tickers).quoting(quote));
        } else if let Some((story, quote)) = explained(&summary_quote) {
            picked.push(Headline::of(story, vec![ticker.clone()]).quoting(Some(quote)));
        }
    }
    let market = market_pct
        .filter(|pct| pct.abs() >= MARKET_MOVE_PCT)
        .and_then(|market_pct| {
            pool.iter().find(|(story, _)| {
                is_roundup(&story.headline)
                    && story.scope == NewsScope::Market
                    && fits(&story.headline, market_pct) == Some(2)
                    && !used(&picked, story)
            })
        })
        .map(|(story, _)| Headline::of(story, Vec::new()).quoting(summary_quote(story)));
    Picked {
        movers: picked,
        market,
    }
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
            ("Testco".to_owned(), "TEST".to_owned()),
            ("Peerco".to_owned(), "PEER".to_owned()),
        ];
        let stories = vec![
            // Good news for AI names on a day they fell, naming none of them: never shown.
            story(
                "Lawmakers Unveil $2.4B in AI Pledges to Boost Research",
                "2026-10-08T13:00:00Z",
                "Bloomberg",
            ),
            // Says Testco rose on a day it fell: left out.
            story(
                "Testco rises premarket on supply deal",
                "2026-10-08T11:00:00Z",
                "Benzinga",
            ),
            story(
                "Why Is Testco Stock Falling Today?",
                "2026-10-08T17:00:00Z",
                "Benzinga",
            ),
            story(
                "Testco, Othercorp and other AI stocks sink on a rival's revenue report",
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
            &[("TEST".into(), -2.1), ("PEER".into(), -3.4)],
            Some(-0.7),
            &BTreeMap::new(),
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        // The explainer has no cause read from its text, so the editor's headline stands.
        let titles: Vec<&str> = picked
            .movers
            .iter()
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(
            titles,
            ["Testco, Othercorp and other AI stocks sink on a rival's revenue report"]
        );
        assert_eq!(picked.movers[0].tickers, ["TEST"]);
        assert_eq!(picked.movers[0].publisher, "CNBC");
        let market = picked.market.unwrap();
        assert_eq!(market.title, "Stocks fall as oil prices jump");
        assert!(market.tickers.is_empty());
    }

    #[test]
    fn a_why_it_moved_story_is_quoted_by_the_sentence_that_says_why() {
        let names = vec![("Testco".to_owned(), "TEST".to_owned())];
        let mut explainer = story(
            "Why Is Testco Stock Surging On Friday?",
            "2026-10-09T16:00:00Z",
            "Benzinga",
        );
        explainer.symbols = vec!["TEST".into()];
        explainer.summary =
            "Testco shares gain Friday, alongside an industry-wide cloud rebound.".into();
        let mut other_way = story(
            "Why Is Testco Stock Falling On Friday?",
            "2026-10-09T17:00:00Z",
            "Benzinga",
        );
        other_way.symbols = vec!["TEST".into()];
        let causes = BTreeMap::from([(
            explainer.url.clone(),
            reading::Cause {
                sentence: "The stock rose after the company signed a multi-year capacity deal."
                    .into(),
                reader: "apple".into(),
            },
        )]);
        let picked = pick(
            &[explainer.clone(), other_way],
            &names,
            &[("TEST".into(), 8.2)],
            Some(0.2),
            &causes,
            at("2026-10-08T20:00:00Z"),
            at("2026-10-12T13:30:00Z"),
        );
        assert_eq!(picked.movers.len(), 1);
        assert_eq!(picked.movers[0].url, explainer.url);
        assert_eq!(
            picked.movers[0].quote.as_deref(),
            Some("The stock rose after the company signed a multi-year capacity deal.")
        );
        assert_eq!(picked.movers[0].quoted_by.as_deref(), Some("apple"));
        // Too small a market move for a wrap.
        assert!(picked.market.is_none());
        // Without a reading, the publisher's summary is quoted, credited as theirs.
        let picked = pick(
            &[explainer],
            &names,
            &[("TEST".into(), 8.2)],
            None,
            &BTreeMap::new(),
            at("2026-10-08T20:00:00Z"),
            at("2026-10-12T13:30:00Z"),
        );
        assert_eq!(picked.movers[0].quoted_by.as_deref(), Some("summary"));
        // A cause that says the stock fell never stands for a rise.
        let falling = BTreeMap::from([(
            "https://example.com/38".to_owned(),
            reading::Cause {
                sentence: "Shares fell after the company cut its outlook.".into(),
                reader: "rule".into(),
            },
        )]);
        let mut cut = story(
            "Why Is Testco Stock Moving Today?",
            "2026-10-09T16:00:00Z",
            "Benzinga",
        );
        cut.symbols = vec!["TEST".into()];
        cut.url = "https://example.com/38".into();
        let picked = pick(
            &[cut],
            &names,
            &[("TEST".into(), 8.2)],
            None,
            &falling,
            at("2026-10-08T20:00:00Z"),
            at("2026-10-12T13:30:00Z"),
        );
        assert!(picked.movers.is_empty());
    }

    #[test]
    fn a_quiet_market_or_a_headline_the_other_way_gets_nothing() {
        let names = vec![("Testco".to_owned(), "TEST".to_owned())];
        let stories = vec![
            story(
                "Testco jumps on record orders",
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
            &[("TEST".into(), -1.5)],
            Some(-0.1),
            &BTreeMap::new(),
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        assert_eq!(picked, Picked::default());
        // A plain company headline, about Testco alone, stands for its move either way.
        let plain = vec![story(
            "Testco names a new chief operating officer",
            "2026-10-08T15:00:00Z",
            "Benzinga",
        )];
        let picked = pick(
            &plain,
            &names,
            &[("TEST".into(), -1.5)],
            None,
            &BTreeMap::new(),
            at("2026-10-07T20:00:00Z"),
            at("2026-10-09T13:30:00Z"),
        );
        assert_eq!(picked.movers.len(), 1);
    }
}
