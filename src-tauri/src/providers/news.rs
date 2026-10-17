//! Ticker news from Alpaca (Benzinga), each story scored for how much it is about the company,
//! the whole portfolio's news in one request, and 8-K filings from SEC EDGAR.
use super::portfolio::held_securities;
use super::*;
use news_cache::{NewsCache, NewsResult};
use std::sync::LazyLock;

const MAX_HEADLINE_CHARS: usize = 500;
const MAX_SUMMARY_CHARS: usize = 1_200;
/// A story naming more tickers than this is a roundup, not news about one company.
const ROUNDUP_TICKERS: usize = 5;
pub(crate) const MARKET_FUND: (&str, &str) = ("SPY", "S&P 500");

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Benchmark {
    pub symbol: String,
    pub name: String,
    pub change_pct: f64,
}

impl Providers {
    /// The news database, opened on first use and then kept open. Holding it serializes the
    /// news, filing, and earnings work that reads or writes it.
    pub(crate) async fn news_cache(
        &self,
    ) -> Result<tokio::sync::MappedMutexGuard<'_, NewsCache>, String> {
        let mut slot = self.news.lock().await;
        if slot.is_none() {
            *slot = Some(NewsCache::new(self.news_cache_path.as_deref())?);
        }
        tokio::sync::MutexGuard::try_map(slot, Option::as_mut)
            .map_err(|_| "News storage is unavailable".to_string())
    }

    /// When a holding's stories were last saved, for Data health.
    pub async fn newest_story(&self) -> Option<DateTime<Utc>> {
        let saved = self.news_cache().await.ok()?.newest_saved().ok()??;
        DateTime::from_timestamp(saved, 0)
    }

    pub async fn market_news(
        &self,
        symbols: Vec<String>,
        refresh: bool,
    ) -> Result<NewsResult, String> {
        let symbol = one_symbol(symbols)?;
        {
            let mut cache = self.news_cache().await?;
            let now = Utc::now().timestamp();
            if !refresh {
                return cache.result(&symbol, now, None);
            }
            if let Err(error) = cache.begin_refresh(&symbol, now) {
                return cache.result(&symbol, now, Some(error));
            }
        }
        let credentials = self
            .read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| "Connect Alpaca in Settings to refresh news".to_string())?;
        let contact = self.sec_contact()?;
        let generation = self.credential_generation.load(Ordering::SeqCst);
        // The refresh interval keeps a ticker's refreshes apart, so the lock is not held
        // across the network.
        let (fetched, company) = tokio::join!(
            async {
                self.alpaca_request(
                    &credentials,
                    "/v1beta1/news",
                    &[
                        ("symbols", symbol.as_str()),
                        ("limit", "50"),
                        ("sort", "desc"),
                    ],
                )
                .await
                .and_then(|payload| parse_alpaca_news(&payload, &symbol))
            },
            async {
                match &contact {
                    Some(contact) => Some(self.sec_company(contact, &symbol).await),
                    None => None,
                }
            }
        );
        if self.credential_generation.load(Ordering::SeqCst) != generation {
            return Err("News credentials changed; retry the request".into());
        }
        let cache = self.news_cache().await?;
        let now = Utc::now().timestamp();
        let warning = match fetched {
            Ok(articles) => cache.save(&symbol, &articles, now).err(),
            Err(error) => Some(error),
        };
        if let Some(ref warning) = warning {
            cache.failure(&symbol, warning)?;
        }
        match company {
            Some(Ok(company)) => cache.save_filings(&symbol, company.as_ref(), now)?,
            Some(Err(error)) => cache.filings_failure(&symbol, &error)?,
            None => {}
        }
        cache.result(&symbol, now, warning)
    }
}

/// The outcome of a portfolio news refresh.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PortfolioNews {
    /// `false` when one ran within the last ten minutes, so nothing was requested.
    pub refreshed: bool,
    pub stories: usize,
    pub filings: usize,
}

/// Pages of portfolio news per refresh: 200 stories covers a week for most portfolios.
const PORTFOLIO_NEWS_PAGES: usize = 4;
const PORTFOLIO_NEWS_DAYS: i64 = 7;
/// EDGAR filings older than this are checked again on a portfolio refresh, a few per refresh.
const PORTFOLIO_FILINGS_AGE: i64 = 12 * 3_600;
const PORTFOLIO_FILINGS_PER_REFRESH: usize = 12;

impl Providers {
    /// Saves the past week's stories for every held stock from one paged Alpaca request, and
    /// checks EDGAR for the holdings whose filings are oldest, so the portfolio's summary and
    /// holdings list have evidence without opening each holding. At most once every ten
    /// minutes; free of charge on Alpaca's and EDGAR's free access.
    pub async fn refresh_portfolio_news(
        &self,
        snapshot: &crate::finance_contract::Snapshot,
    ) -> Result<PortfolioNews, String> {
        let held = held_securities(&snapshot.holdings);
        let stocks = crate::summary::modeled(&held, None);
        let skipped = PortfolioNews {
            refreshed: false,
            stories: 0,
            filings: 0,
        };
        if stocks.is_empty() {
            return Ok(skipped);
        }
        let credentials = self
            .read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY)?
            .ok_or_else(|| "Connect Alpaca in Settings to refresh news".to_string())?;
        {
            if !self
                .news_cache()
                .await?
                .begin_portfolio_refresh(Utc::now().timestamp())?
            {
                return Ok(skipped);
            }
        }
        let generation = self.credential_generation.load(Ordering::SeqCst);
        let symbols = stocks.join(",");
        let since = (Utc::now() - Duration::days(PORTFOLIO_NEWS_DAYS)).to_rfc3339();
        let mut stories = Vec::new();
        let query = [
            ("symbols", symbols.as_str()),
            ("limit", "50"),
            ("sort", "desc"),
            ("start", since.as_str()),
        ];
        self.alpaca_pages(
            &credentials,
            "/v1beta1/news",
            &query,
            PORTFOLIO_NEWS_PAGES,
            |payload| {
                stories.extend(parse_alpaca_stories(payload)?);
                Ok(())
            },
        )
        .await?;
        // CNBC's market sections, whose headlines name the causes of market, macro, and theme
        // moves (a report on a private company, say) that name no holding.
        let names: Vec<(String, String)> = held
            .iter()
            .map(|security| (security.ticker.clone(), security.name.clone()))
            .collect();
        let feed = self.publisher_feeds(&names).await;
        if self.credential_generation.load(Ordering::SeqCst) != generation {
            return Err("News credentials changed; retry the request".into());
        }
        let now = Utc::now().timestamp();
        let stale: Vec<String> = {
            let cache = self.news_cache().await?;
            cache.save_market_news(&feed, now)?;
            for symbol in &stocks {
                let own: Vec<MarketNewsArticle> = stories
                    .iter()
                    .filter(|story| story.symbols.contains(symbol))
                    .cloned()
                    .collect();
                if !own.is_empty() {
                    cache.save(symbol, &own, now)?;
                }
            }
            let mut stale = Vec::new();
            for symbol in &stocks {
                let saved = cache.filings_saved(symbol)?;
                if saved.is_none_or(|at| at > now || now - at >= PORTFOLIO_FILINGS_AGE) {
                    stale.push((saved.unwrap_or(0), symbol.clone()));
                }
            }
            stale.sort();
            stale
                .into_iter()
                .take(PORTFOLIO_FILINGS_PER_REFRESH)
                .map(|(_, symbol)| symbol)
                .collect()
        };
        // EDGAR asks for a contact; without one, filings wait for the holding's page.
        let mut filings = 0;
        if let Some(contact) = self.sec_contact()? {
            for symbol in stale {
                let company = self.sec_company(&contact, &symbol).await;
                let cache = self.news_cache().await?;
                match company {
                    Ok(company) => {
                        cache.save_filings(&symbol, company.as_ref(), Utc::now().timestamp())?;
                        filings += 1;
                    }
                    Err(error) => cache.filings_failure(&symbol, &error)?,
                }
            }
        }
        Ok(PortfolioNews {
            refreshed: true,
            stories: stories.len(),
            filings,
        })
    }
}

fn one_symbol(symbols: Vec<String>) -> Result<String, String> {
    let mut symbols = normalize_stock_symbols(symbols);
    if symbols.len() != 1 {
        return Err("Choose one ticker for news".into());
    }
    Ok(symbols.remove(0))
}

/// Company stories recent enough to explain today's move, events first, then newest.
fn bounded(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// Provider text is shown as plain text, so drop markup and decode common entities.
pub(super) fn plain_text(value: &str) -> String {
    static TAGS: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("valid pattern"));
    let text = TAGS.replace_all(value, " ");
    let text = text
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A story may be shown and opened only with an https link and the requested ticker.
pub(super) fn safe_article(article: &MarketNewsArticle, symbol: &str) -> bool {
    safe_link(article) && article.symbols.iter().any(|candidate| candidate == symbol)
}

/// A story may be shown and opened only with an https link, a headline, a source, and a time.
pub(super) fn safe_link(article: &MarketNewsArticle) -> bool {
    let Ok(url) = Url::parse(&article.url) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && !article.headline.trim().is_empty()
        && !article.source.trim().is_empty()
        && DateTime::parse_from_rfc3339(&article.created_at).is_ok()
}

pub(crate) fn pattern(source: &str) -> regex::Regex {
    regex::RegexBuilder::new(source)
        .case_insensitive(true)
        .build()
        .expect("valid pattern")
}

/// Generated pages that compare a company with peers or summarize options and short
/// interest. They say nothing about what happened.
pub(super) static FILLER: LazyLock<regex::Regex> = LazyLock::new(|| {
    pattern(
        r"competitive space|competitor dynamics|industry peers|industry competitors|options activity|options frenzy|unusual options|what the options market|short interest|p/e ratio|price over earnings|whales",
    )
});
/// Analyst ratings and price targets: opinions about the stock, not events at the company.
pub(super) static ANALYST: LazyLock<regex::Regex> = LazyLock::new(|| {
    pattern(
        r"\b(maintains|reiterates|upgrades?|downgrades?|initiates)\b|price target|\b(outperform|underperform|overweight|underweight)\b",
    )
});
pub(super) static ROUNDUP: LazyLock<regex::Regex> = LazyLock::new(|| {
    pattern(
        r"stock market today|stocks to watch|\btop \w+ (us )?stocks|\b\d+ stocks\b|bulls and bears|most-searched|movers|market (update|wrap|recap)|futures (rise|fall|edge|slip)",
    )
});
static TAGS: LazyLock<Vec<(NewsTag, regex::Regex)>> = LazyLock::new(|| {
    [
        (NewsTag::Earnings, r"\bearnings\b|\beps\b|\bq[1-4] (results|sales|revenue|earnings)|quarterly results|(beats|misses|tops) (estimates|expectations)"),
        (NewsTag::Guidance, r"\bguidance\b|\boutlook\b|\bforecasts?\b|\b(raises|cuts|lowers|sees) fy"),
        (NewsTag::Product, r"\b(launch(es|ed)?|unveil(s|ed)?|rolls? out|rolled out|introduc(es|ed)|debuts?|releases?|released|ships?|shipping)\b"),
        (NewsTag::Deal, r"\b(acquires?|acquisition|merger|buyout|takeover|to buy|partnership|partners with|teams up|wins? .{0,40}contract|awarded .{0,40}contract|signs? .{0,40}(deal|agreement|contract))\b"),
        (NewsTag::Legal, r"\b(lawsuit|sues|sued|probe|investigation|antitrust|settlement|recall|subpoena|indicted)\b|\b(ftc|doj)\b"),
        (NewsTag::Leadership, r"\b(steps down|resigns|resignation|appoints|appointed|successor|ousted|retires)\b|\bnames new\b|\bnew (ceo|cfo)\b"),
        (NewsTag::Capital, r"\bdividend|\bbuyback|\brepurchase|stock split|share offering|secondary offering"),
        // Same-day explainers of a move (Benzinga's "Why It's Moving"): they say why it moved.
        (NewsTag::Explainer, r"\bwhy\b.{0,80}\b(stocks?|shares)\b|what[’']?s going on with|\b(stock|shares) (is|are) (trading|moving) (higher|lower)|\bshares are (soaring|surging|sliding|falling|tumbling|rising|jumping|plunging|climbing|sinking)"),
    ]
    .into_iter()
    .map(|(tag, source)| (tag, pattern(source)))
    .collect()
});
/// Some summaries are the start of an article body rather than a summary.
static BODY_SUMMARY: LazyLock<regex::Regex> =
    LazyLock::new(|| pattern(r"click here|note the following"));

/// Validates a saved or fetched story for `symbol` and recomputes its scope, tags, and
/// relevance. Generated filler and analyst ratings are dropped. `key` is the company's name as
/// headlines write it (see `company_key`), when EDGAR lists the ticker.
pub(super) fn classify_story(
    mut article: MarketNewsArticle,
    symbol: &str,
    key: Option<&str>,
) -> Option<MarketNewsArticle> {
    if !safe_article(&article, symbol)
        || FILLER.is_match(&article.headline)
        || ANALYST.is_match(&article.headline)
    {
        return None;
    }
    if BODY_SUMMARY.is_match(&article.summary) {
        article.summary.clear();
    }
    article.scope =
        if article.symbols.len() > ROUNDUP_TICKERS || ROUNDUP.is_match(&article.headline) {
            NewsScope::Market
        } else {
            NewsScope::Company
        };
    article.tags = TAGS
        .iter()
        .filter(|(_, pattern)| pattern.is_match(&article.headline))
        .map(|(tag, _)| *tag)
        .collect();
    article.relevance = relevance(&article, symbol, key);
    Some(article)
}

#[cfg(test)]
pub(super) fn classify(article: MarketNewsArticle, symbol: &str) -> Option<MarketNewsArticle> {
    classify_story(article, symbol, None)
}

/// A story at least this relevant is about the company; the rest only mention it.
pub(crate) const RELEVANT: u8 = 40;

/// The word or words headlines use for a company: its EDGAR name without legal suffixes,
/// "NVIDIA CORP" as `nvidia`, "Nebius Group N.V." as `nebius`. A generic first word keeps the
/// next one ("Advanced Micro Devices" as `advanced micro`).
pub(super) fn company_key(name: &str) -> Option<String> {
    const LEGAL: &[&str] = &[
        "inc",
        "incorporated",
        "corp",
        "corporation",
        "co",
        "company",
        "ltd",
        "limited",
        "plc",
        "nv",
        "sa",
        "ag",
        "se",
        "group",
        "holdings",
        "holding",
        "class",
        "the",
        "com",
        "lp",
        "llc",
        "trust",
        "de",
        "del",
    ];
    const GENERIC: &[&str] = &[
        "advanced",
        "american",
        "general",
        "united",
        "first",
        "international",
        "global",
        "national",
        "new",
        "digital",
        "applied",
        "micro",
        "energy",
        "royal",
        "bank",
    ];
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 2 && !LEGAL.contains(word))
        .collect();
    let first = *words.first()?;
    if first.chars().count() >= 4 && !GENERIC.contains(&first) {
        Some(first.to_owned())
    } else if words.len() >= 2 {
        Some(words[..2].join(" "))
    } else {
        (first.chars().count() >= 3).then(|| first.to_owned())
    }
}

/// Whether `text` names the company: its ticker as a word (`NBIS`, `$NBIS`, `BRK.B`), or its
/// name's key, ignoring case and punctuation.
fn mentions(text: &str, symbol: &str, key: Option<&str>) -> bool {
    let ticker = symbol.len() >= 2
        && text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
            .any(|word| word.trim_end_matches('.') == symbol);
    ticker
        || key.is_some_and(|key| {
            let words: Vec<String> = text
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect();
            format!(" {} ", words.join(" ")).contains(&format!(" {key} "))
        })
}

/// How much a story is about the company, from 0 to 100, from what is checkable in the story
/// itself, in the manner of RavenPack's relevance score: a story tagged with few tickers that
/// names the company in its headline is about it; one tagged with many, or that names it only
/// in passing, is about something else. Broker research about another company (a Morgan Stanley
/// note on a peer) and market roundups fall below `RELEVANT`.
pub(super) fn relevance(article: &MarketNewsArticle, symbol: &str, key: Option<&str>) -> u8 {
    let breadth: u8 = match article.symbols.len() {
        0 | 1 => 50,
        2 => 40,
        3 => 30,
        4 | 5 => 20,
        _ => 10,
    };
    let headline = mentions(&article.headline, symbol, key);
    let named = if headline {
        40
    } else if mentions(&article.summary, symbol, key) {
        10
    } else {
        0
    };
    let event = if headline && !article.tags.is_empty() {
        10
    } else {
        0
    };
    let score = breadth + named + event;
    // A story whose headline is about something else only mentions the company, unless the
    // company is all it was tagged with.
    let score = if headline || article.symbols.len() <= 1 {
        score
    } else {
        score.min(RELEVANT - 5)
    };
    match article.scope {
        NewsScope::Market => score.min(25),
        NewsScope::Company => score.min(100),
    }
}

pub(super) fn parse_alpaca_news(
    payload: &Value,
    symbol: &str,
) -> Result<Vec<MarketNewsArticle>, String> {
    Ok(parse_alpaca_stories(payload)?
        .into_iter()
        .filter(|article| safe_article(article, symbol))
        .collect())
}

/// Every story in one page of Alpaca news, whatever tickers it names.
pub(super) fn parse_alpaca_stories(payload: &Value) -> Result<Vec<MarketNewsArticle>, String> {
    let news = payload
        .get("news")
        .and_then(Value::as_array)
        .ok_or("Alpaca response omitted news")?;
    let mut seen = BTreeSet::new();
    let articles = news
        .iter()
        .filter_map(|item| {
            let created_at = DateTime::parse_from_rfc3339(item["created_at"].as_str()?)
                .ok()?
                .with_timezone(&Utc);
            let source = item["source"].as_str()?.trim();
            let mut characters = source.chars();
            let source = characters
                .next()
                .map(|first| first.to_uppercase().chain(characters).collect::<String>())?;
            let symbols = normalize_stock_symbols(
                item["symbols"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            );
            let article = MarketNewsArticle {
                headline: bounded(&plain_text(item["headline"].as_str()?), MAX_HEADLINE_CHARS),
                summary: bounded(
                    &plain_text(item["summary"].as_str().unwrap_or("")),
                    MAX_SUMMARY_CHARS,
                ),
                source,
                url: item["url"].as_str()?.trim().into(),
                created_at: created_at.to_rfc3339(),
                symbols,
                scope: NewsScope::Company,
                tags: Vec::new(),
                relevance: 0,
            };
            let ticker = article.symbols.first()?.clone();
            (safe_article(&article, &ticker) && seen.insert(article.url.clone())).then_some(article)
        })
        .collect();
    Ok(articles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(headline: &str, symbols: &[&str]) -> MarketNewsArticle {
        MarketNewsArticle {
            headline: headline.into(),
            summary: "Synthetic evidence".into(),
            source: "Benzinga".into(),
            url: "https://example.com/story".into(),
            created_at: "2026-09-21T12:00:00+00:00".into(),
            symbols: symbols.iter().map(|s| (*s).into()).collect(),
            scope: NewsScope::Company,
            tags: vec![],
            relevance: 0,
        }
    }

    #[test]
    fn alpaca_news_keeps_safe_ticker_stories_as_plain_text() {
        let item = json!({
            "id": 1, "headline": "TEST &amp; partner <b>expand</b> deal", "author": "Synthetic",
            "created_at": "2026-09-21T12:00:00Z", "updated_at": "2026-09-21T12:00:00Z",
            "summary": "<p>Synthetic&nbsp;evidence</p>", "content": "", "images": [],
            "symbols": ["TEST", "OTHER"], "source": "benzinga", "url": "https://example.com/story"
        });
        let mut unsafe_item = item.clone();
        unsafe_item["url"] = "javascript:alert(1)".into();
        let mut no_url = item.clone();
        no_url["url"] = "".into();
        let mut other = item.clone();
        other["symbols"] = json!(["OTHER"]);
        other["url"] = "https://example.com/other".into();
        let articles = parse_alpaca_news(
            &json!({"news": [item.clone(), unsafe_item, no_url, other, item], "next_page_token": null}),
            "TEST",
        )
        .unwrap();
        assert_eq!(articles.len(), 1);
        let value = serde_json::to_value(&articles[0]).unwrap();
        assert_eq!(value["headline"], "TEST & partner expand deal");
        assert_eq!(value["summary"], "Synthetic evidence");
        assert_eq!(value["source"], "Benzinga");
        assert_eq!(value["createdAt"], "2026-09-21T12:00:00+00:00");
        assert_eq!(value["symbols"], json!(["OTHER", "TEST"]));
        assert!(parse_alpaca_news(&json!({"message": "forbidden"}), "TEST").is_err());
    }

    #[test]
    fn stories_are_filtered_scoped_and_tagged_from_their_headlines() {
        for headline in [
            "Synthetic Bank Maintains Overweight on TEST, Lowers Price Target to $355",
            "Synthetic Securities Downgrades TEST to Underperform",
            "Synthetic Capital Initiates Coverage On TEST",
        ] {
            assert!(classify(story(headline, &["TEST"]), "TEST").is_none());
        }
        let earnings = classify(
            story("TEST Beats Estimates, Raises FY Guidance", &["TEST"]),
            "TEST",
        )
        .unwrap();
        assert_eq!(earnings.tags, vec![NewsTag::Earnings, NewsTag::Guidance]);
        // Launches and contract wins are what most often move smaller companies.
        let launch = classify(
            story(
                "Nebius Rolls Out Agentic Open Model Beam: 501B Total Parameters, 23B Active",
                &["TEST"],
            ),
            "TEST",
        )
        .unwrap();
        assert_eq!(launch.tags, vec![NewsTag::Product]);
        let contract =
            classify(story("TEST Wins $40M Defense Contract", &["TEST"]), "TEST").unwrap();
        assert_eq!(contract.tags, vec![NewsTag::Deal]);
        assert_eq!(
            classify(
                story("Why Is TEST Stock Soaring Tuesday?", &["TEST"]),
                "TEST"
            )
            .unwrap()
            .tags,
            vec![NewsTag::Explainer]
        );
        let many: Vec<_> = (0..8).map(|i| format!("T{i}")).collect();
        let mut roundup = story("Nasdaq Climbs: Stock Market Today", &["TEST"]);
        roundup.symbols.extend(many);
        assert_eq!(classify(roundup, "TEST").unwrap().scope, NewsScope::Market);
        assert_eq!(
            classify(
                story("Top Three US Stocks to Watch This Week", &["TEST"]),
                "TEST"
            )
            .unwrap()
            .scope,
            NewsScope::Market
        );
        assert!(classify(
            story(
                "Exploring The Competitive Space: TEST Versus Industry Peers",
                &["TEST"]
            ),
            "TEST"
        )
        .is_none());
        let mut body = story("TEST Rallies", &["TEST"]);
        body.summary = "Please click here for an enlarged chart. Note the following:".into();
        assert!(classify(body, "TEST").unwrap().summary.is_empty());
        assert!(classify(story("TEST Rallies", &["OTHER"]), "TEST").is_none());
    }

    #[test]
    fn company_names_reduce_to_the_words_headlines_use() {
        assert_eq!(company_key("NVIDIA CORP").as_deref(), Some("nvidia"));
        assert_eq!(company_key("Nebius Group N.V.").as_deref(), Some("nebius"));
        assert_eq!(company_key("Amazon Com Inc").as_deref(), Some("amazon"));
        assert_eq!(
            company_key("Advanced Micro Devices Inc").as_deref(),
            Some("advanced micro")
        );
        assert_eq!(company_key("Inc."), None);
    }

    #[test]
    fn relevance_favors_stories_about_the_company_over_passing_mentions() {
        let scored = |headline: &str, summary: &str, symbols: &[&str]| {
            let mut article = story(headline, symbols);
            article.summary = summary.into();
            classify_story(article, "NBIS", Some("nebius"))
                .unwrap()
                .relevance
        };
        // Named in the headline, tagged with it alone, and an event: about the company.
        let launch = scored(
            "Nebius Rolls Out Agentic Open Model Beam: 501B Total Parameters, 23B Active",
            "",
            &["NBIS"],
        );
        assert_eq!(launch, 100);
        // Broker research about a peer that mentions the company only in passing.
        let peer = scored(
            "Morgan Stanley Raises CoreWeave Price Objective On AI Demand",
            "The firm also cited Nebius among neocloud peers.",
            &["CRWV", "MS", "NBIS"],
        );
        assert!(peer < RELEVANT, "peer note scored {peer}");
        // A roundup never counts as company news, however it is tagged.
        let roundup = scored("Nebius, Oracle Lead Stocks to Watch", "", &["NBIS", "ORCL"]);
        assert!(roundup <= 25);
        // Tagged only with the company, a story is about it even without naming it.
        assert!(scored("AI Cloud Provider Expands In Finland", "", &["NBIS"]) >= RELEVANT);
        // The ticker counts as a name, as a whole word only.
        assert!(scored("NBIS Shares Jump", "", &["NBIS", "NVDA"]) >= RELEVANT);
        assert!(scored("NBISX Fund Update", "", &["NBIS", "NVDA"]) < RELEVANT);
    }

    #[test]
    fn portfolio_news_keeps_every_ticker_and_the_next_page() {
        let item = |url: &str, symbols: serde_json::Value| {
            json!({
                "headline": "Synthetic story", "created_at": "2026-09-21T12:00:00Z",
                "summary": "", "symbols": symbols, "source": "benzinga", "url": url
            })
        };
        let stories = parse_alpaca_stories(&json!({
            "news": [
                item("https://example.com/a", json!(["AAA", "BBB"])),
                item("https://example.com/b", json!(["CCC"])),
                item("javascript:alert(1)", json!(["AAA"])),
                item("https://example.com/c", json!([])),
            ],
            "next_page_token": "token"
        }))
        .unwrap();
        assert_eq!(stories.len(), 2);
        assert_eq!(stories[0].symbols, vec!["AAA", "BBB"]);
    }
}
