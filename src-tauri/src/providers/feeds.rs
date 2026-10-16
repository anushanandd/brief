//! CNBC's RSS feeds for its market sections. Editors' headlines name causes ("Nvidia, Oracle and
//! other AI stocks sink on OpenAI revenue report") that a ticker-tagged wire often buries under
//! filler. The feeds are public, with no feed-specific terms found; Brief shows each story's
//! headline as published, credited, with a link to the article.
//!
//! RSS names companies, not tickers, so each story's tickers are read from its headline and
//! description: tickers written out (`(NVDA)`, `$NVDA`), held companies' names, and the names of
//! companies news most often moves together.
use super::*;
use std::sync::LazyLock;

/// CNBC sections that report on markets: top news, technology, investing, the economy, and the
/// day's biggest movers.
const CNBC_FEEDS: &[&str] = &["100003114", "19854910", "15839069", "20910258", "20409666"];
const FEED_BYTES: usize = 1_000_000;
const MAX_HEADLINE_CHARS: usize = 500;
const MAX_SUMMARY_CHARS: usize = 600;

/// Names headlines use for companies news often moves together, matched as capitalized words.
/// Held companies are matched by their own names as well.
const NAMES: &[(&str, &str)] = &[
    ("Nvidia", "NVDA"),
    ("Oracle", "ORCL"),
    ("CoreWeave", "CRWV"),
    ("Microsoft", "MSFT"),
    ("Apple", "AAPL"),
    ("Amazon", "AMZN"),
    ("Alphabet", "GOOG"),
    ("Google", "GOOG"),
    ("Meta", "META"),
    ("Tesla", "TSLA"),
    ("Broadcom", "AVGO"),
    ("AMD", "AMD"),
    ("Advanced Micro Devices", "AMD"),
    ("Intel", "INTC"),
    ("Micron", "MU"),
    ("TSMC", "TSM"),
    ("Taiwan Semiconductor", "TSM"),
    ("Qualcomm", "QCOM"),
    ("Marvell", "MRVL"),
    ("Super Micro", "SMCI"),
    ("Supermicro", "SMCI"),
    ("Dell", "DELL"),
    ("Palantir", "PLTR"),
    ("Nebius", "NBIS"),
    ("IREN", "IREN"),
    ("Applied Materials", "AMAT"),
    ("Lam Research", "LRCX"),
    ("ASML", "ASML"),
    ("Salesforce", "CRM"),
    ("Adobe", "ADBE"),
    ("ServiceNow", "NOW"),
    ("Snowflake", "SNOW"),
    ("Netflix", "NFLX"),
    ("Uber", "UBER"),
    ("Coinbase", "COIN"),
    ("Robinhood", "HOOD"),
    ("ExxonMobil", "XOM"),
    ("Exxon", "XOM"),
    ("Chevron", "CVX"),
    ("JPMorgan", "JPM"),
    ("Goldman Sachs", "GS"),
    ("Morgan Stanley", "MS"),
    ("Bank of America", "BAC"),
    ("Wells Fargo", "WFC"),
    ("Walmart", "WMT"),
    ("Costco", "COST"),
    ("PepsiCo", "PEP"),
    ("Coca-Cola", "KO"),
    ("Starbucks", "SBUX"),
    ("Chipotle", "CMG"),
    ("Boeing", "BA"),
    ("Disney", "DIS"),
    ("Eli Lilly", "LLY"),
    ("UnitedHealth", "UNH"),
    ("Pfizer", "PFE"),
    ("Vistra", "VST"),
    ("Constellation Energy", "CEG"),
    ("Bloom Energy", "BE"),
];

static WRITTEN_TICKER: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?:\(|\$|NASDAQ:|NYSE:)\s*([A-Z]{1,5}(?:\.[A-Z])?)(\.PVT)?\b")
        .expect("valid pattern")
});
static ITEM: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<item\b[^>]*>(.*?)</item>").expect("valid pattern"));

/// A feed's element text: CDATA unwrapped, markup dropped, entities decoded.
fn element(item: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = item.find(&open)?;
    let body_start = start + item[start..].find('>')? + 1;
    let end = body_start + item[body_start..].find(&format!("</{tag}>"))?;
    let raw = item[body_start..end].trim();
    let raw = raw
        .strip_prefix("<![CDATA[")
        .and_then(|text| text.strip_suffix("]]>"))
        .unwrap_or(raw);
    Some(news::plain_text(raw))
}

/// The tickers a story names, among `names` (name, ticker) and tickers written out.
pub(crate) fn named_tickers(text: &str, names: &[(String, String)]) -> Vec<String> {
    let mut found: BTreeSet<String> = WRITTEN_TICKER
        .captures_iter(text)
        // Private companies are sometimes written `OPAI.PVT`; they have no price.
        .filter(|capture| capture.get(2).is_none())
        .filter_map(|capture| capture.get(1))
        .map(|ticker| ticker.as_str().to_owned())
        .filter(|ticker| {
            !matches!(
                ticker.as_str(),
                "BTC" | "ETH" | "AI" | "CEO" | "IPO" | "ETF"
            )
        })
        .filter(|ticker| alpaca::stock_symbol(ticker).is_ok())
        .collect();
    for (name, ticker) in names {
        let at = text.match_indices(name.as_str()).any(|(index, _)| {
            let before = text[..index].chars().next_back();
            let after = text[index + name.len()..].chars().next();
            before.is_none_or(|c| !c.is_alphanumeric())
                && after.is_none_or(|c| !c.is_alphanumeric())
        });
        if at {
            found.insert(ticker.clone());
        }
    }
    found.into_iter().collect()
}

/// The names to look for: the common ones and each held company's, as headlines write it.
pub(crate) fn names(held: &[(String, String)]) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = NAMES
        .iter()
        .map(|(name, ticker)| ((*name).to_owned(), (*ticker).to_owned()))
        .collect();
    for (ticker, name) in held {
        if let Some(key) = news::company_key(name) {
            // Headlines capitalize names: "nebius" as `Nebius`, "advanced micro" as `Advanced Micro`.
            let written = key
                .split(' ')
                .map(|word| {
                    let mut chars = word.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ");
            if written.chars().count() >= 4 {
                names.push((written, ticker.clone()));
            }
        }
    }
    names
}

/// Every safe story in an RSS document. `publisher` credits the feed; stories carry the tickers
/// they name.
pub(super) fn parse_rss(
    xml: &str,
    publisher: &str,
    names: &[(String, String)],
) -> Vec<MarketNewsArticle> {
    ITEM.captures_iter(xml)
        .filter_map(|capture| {
            let item = capture.get(1)?.as_str();
            if element(item, "metadata:sponsored").is_some_and(|value| value == "true") {
                return None;
            }
            let headline: String = element(item, "title")?
                .chars()
                .take(MAX_HEADLINE_CHARS)
                .collect();
            let summary: String = element(item, "description")
                .unwrap_or_default()
                .chars()
                .take(MAX_SUMMARY_CHARS)
                .collect();
            let url = element(item, "link")?;
            let created_at = DateTime::parse_from_rfc2822(&element(item, "pubDate")?)
                .ok()?
                .with_timezone(&Utc)
                .to_rfc3339();
            let article = MarketNewsArticle {
                symbols: named_tickers(&format!("{headline} {summary}"), names),
                headline,
                summary,
                source: publisher.to_owned(),
                url,
                created_at,
                scope: NewsScope::Market,
                tags: Vec::new(),
                relevance: 0,
            };
            news::safe_link(&article).then_some(article)
        })
        .collect()
}

/// Every feed to read, with its publisher.
pub(super) fn feed_urls() -> Vec<(String, &'static str)> {
    CNBC_FEEDS
        .iter()
        .map(|id| {
            (
                format!(
                    "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id={id}"
                ),
                "CNBC",
            )
        })
        .collect()
}

impl Providers {
    /// Reads every feed at once; a feed that fails is skipped. Stories found in more than one
    /// section appear once.
    pub(super) async fn publisher_feeds(
        &self,
        held: &[(String, String)],
    ) -> Vec<MarketNewsArticle> {
        let names = names(held);
        let reads = feed_urls().into_iter().map(|(url, publisher)| {
            let names = &names;
            async move {
                let response = self
                    .http
                    .get(&url)
                    .header(reqwest::header::USER_AGENT, "Brief personal finance reader")
                    .header(
                        reqwest::header::ACCEPT,
                        "application/rss+xml, application/xml",
                    )
                    .timeout(StdDuration::from_secs(15))
                    .send()
                    .await
                    .ok()?;
                if !response.status().is_success() {
                    return None;
                }
                let bytes = http::response_bytes(response, FEED_BYTES).await.ok()?;
                Some(parse_rss(
                    &String::from_utf8_lossy(&bytes),
                    publisher,
                    names,
                ))
            }
        });
        let mut seen = BTreeSet::new();
        futures_util::future::join_all(reads)
            .await
            .into_iter()
            .flatten()
            .flatten()
            .filter(|story| seen.insert(story.url.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CNBC: &str = r#"<rss><channel><title>US Top News and Analysis</title>
      <item><link>https://www.cnbc.com/2026/10/08/open-ai-revenue-nvidia-oracle-coreweave.html</link>
      <guid isPermaLink="false">1</guid><metadata:sponsored>false</metadata:sponsored>
      <title>Nvidia, Oracle, CoreWeave and other AI stocks sink on OpenAI revenue report</title>
      <description><![CDATA[OpenAI told investors it hit about $50 billion in annualized revenue.]]></description>
      <pubDate>Thu, 08 Oct 2026 18:18:31 GMT</pubDate></item>
      <item><link>https://www.cnbc.com/sponsored</link><metadata:sponsored>true</metadata:sponsored>
      <title>Sponsored</title><pubDate>Thu, 08 Oct 2026 18:18:31 GMT</pubDate></item>
      <item><link>javascript:alert(1)</link><title>Unsafe</title>
      <pubDate>Thu, 08 Oct 2026 18:18:31 GMT</pubDate></item>
    </channel></rss>"#;

    #[test]
    fn rss_stories_carry_the_tickers_their_headlines_name() {
        let held = [("NBIS".to_string(), "Nebius Group N.V.".to_string())];
        let stories = parse_rss(CNBC, "CNBC", &names(&held));
        assert_eq!(stories.len(), 1);
        let story = &stories[0];
        assert_eq!(story.symbols, ["CRWV", "NVDA", "ORCL"]);
        assert_eq!(story.source, "CNBC");
        assert_eq!(story.created_at, "2026-10-08T18:18:31+00:00");
        assert!(story.summary.starts_with("OpenAI told investors"));
    }

    #[test]
    fn names_match_whole_capitalized_words_and_written_tickers() {
        let held = [("NBIS".to_string(), "Nebius Group N.V.".to_string())];
        let names = names(&held);
        assert_eq!(
            named_tickers("Nebius (NBIS) and $IREN slide; Metaverse fades", &names),
            ["IREN", "NBIS"]
        );
        // A private company's symbol and crypto have no stock price.
        assert!(named_tickers("OpenAI (OPAI.PVT) and bitcoin (BTC)", &names).is_empty());
        // "oracle" in lowercase prose, or inside another word, is not the company.
        assert!(named_tickers("an oracle of markets; Oracles", &names).is_empty());
    }

    #[test]
    fn feeds_cover_cnbcs_market_sections() {
        let urls = feed_urls();
        assert_eq!(urls.len(), 5);
        assert!(urls
            .iter()
            .all(|(url, publisher)| *publisher == "CNBC"
                && url.starts_with("https://search.cnbc.com/")));
    }
}
