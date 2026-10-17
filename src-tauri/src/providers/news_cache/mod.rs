//! Local news, SEC filings, insider trades and reported quarters, and the Alpha Vantage
//! earnings budget, separate from financial state.
use super::{
    fundamentals::Fundamentals,
    insiders::InsiderTrade,
    news::{classify_story, company_key},
    sec::{filing_title, sec_ticker, SecCompany, SecFiling, FILING_DAYS},
    MarketNewsArticle,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

mod edgar;
mod quota;
#[cfg(test)]
mod tests;

const DAY: i64 = 86_400;
const MINUTE: i64 = 60;
/// Alpaca's free plan allows 200 calls a minute across all market data, so one news refresh per
/// ticker per minute leaves the portfolio stream and charts their share.
const NEWS_REFRESH_INTERVAL: i64 = MINUTE;
/// One request covers every holding's news, so the whole portfolio refreshes at most this often.
const PORTFOLIO_NEWS_INTERVAL: i64 = 10 * MINUTE;
/// Stories kept per ticker, newest first, across per-ticker and portfolio refreshes.
const SAVED_STORIES: usize = 120;
/// Stories from CNBC's market feeds are kept this many days.
const MARKET_NEWS_DAYS: i64 = 3;

#[derive(Debug, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "MarketNewsResult")]
pub struct NewsResult {
    pub articles: Vec<MarketNewsArticle>,
    pub saved_at: Option<String>,
    pub warning: Option<String>,
    pub can_refresh: bool,
    /// The company's name in EDGAR, when EDGAR lists the ticker.
    pub company: Option<String>,
    /// Current reports (8-K, or 6-K from foreign issuers) accepted in the last 30 days, newest
    /// first.
    pub filings: Vec<SecFiling>,
    /// When EDGAR was last checked for this ticker; `None` until the first check.
    pub filings_saved_at: Option<String>,
    pub filings_warning: Option<String>,
}

/// Saved stories, when they were saved, and the last refresh warning.
type SavedRow = (Vec<MarketNewsArticle>, Option<i64>, Option<String>);

#[derive(Deserialize, Serialize)]
struct SavedNews {
    articles: Vec<MarketNewsArticle>,
}

pub struct NewsCache(Connection);

impl NewsCache {
    pub fn new(path: Option<&Path>) -> Result<Self, String> {
        let mut db = match path {
            Some(path) => Connection::open(path),
            None => Connection::open_in_memory(),
        }
        .map_err(|_| "News storage is unavailable")?;
        if let Some(path) = path {
            crate::database::secure_file(path)?;
        }
        db.execute_batch("CREATE TABLE IF NOT EXISTS news (symbol TEXT PRIMARY KEY, payload TEXT, saved INTEGER, attempted INTEGER NOT NULL, warning TEXT);
            CREATE TABLE IF NOT EXISTS requests (at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL, period TEXT NOT NULL DEFAULT 'minute');
            CREATE TABLE IF NOT EXISTS earnings (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL, saved INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sec_tickers (ticker TEXT PRIMARY KEY, cik INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sec_meta (id INTEGER PRIMARY KEY CHECK(id=1), saved INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS filings (symbol TEXT PRIMARY KEY, payload TEXT, saved INTEGER, warning TEXT);
            CREATE TABLE IF NOT EXISTS fundamentals (symbol TEXT PRIMARY KEY, payload TEXT, saved INTEGER NOT NULL, failed INTEGER);
            CREATE TABLE IF NOT EXISTS portfolio_news (id INTEGER PRIMARY KEY CHECK(id=1), attempted INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS insider_trades (url TEXT PRIMARY KEY, payload TEXT NOT NULL, saved INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS market_news (url TEXT PRIMARY KEY, created TEXT NOT NULL, payload TEXT NOT NULL);
            DROP TABLE IF EXISTS assessments;
            DROP TABLE IF EXISTS explanations;
            DROP TABLE IF EXISTS period_explanations;")
            .map_err(|_| "News storage is unavailable")?;
        let has_period: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('cooldown') WHERE name='period')",
                [],
                |row| row.get(0),
            )
            .map_err(|_| "News storage is unavailable")?;
        if !has_period {
            let tx = db
                .transaction()
                .map_err(|_| "News storage is unavailable")?;
            tx.execute(
                "ALTER TABLE cooldown ADD COLUMN period TEXT NOT NULL DEFAULT 'minute'",
                [],
            )
            .map_err(|_| "News storage is unavailable")?;
            // Older builds treated an unspecified rate limit as daily. Reclassify that
            // ambiguous saved state as a one-minute throttle from its triggering request.
            tx.execute(
                "UPDATE cooldown SET until=MIN(until, COALESCE((SELECT MAX(at) + ?1 FROM requests), until))",
                [MINUTE],
            )
            .map_err(|_| "News storage is unavailable")?;
            tx.commit().map_err(|_| "News storage is unavailable")?;
        }
        // Older builds treated a single daily-limit response as a full-day block.
        // Give that state one short retry when Brief has barely used its local budget.
        db.execute(
            "UPDATE cooldown SET period='daily-probe', until=MIN(until, COALESCE((SELECT MAX(at) + ?1 FROM requests), 0))
             WHERE period='daily' AND (SELECT COUNT(*) FROM requests WHERE at > (SELECT MAX(at) FROM requests) - ?2) < 25",
            params![MINUTE, DAY],
        )
        .map_err(|_| "News storage is unavailable")?;
        Ok(Self(db))
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.0
            .pragma_update(None, "secure_delete", true)
            .map_err(|_| "News storage is unavailable")?;
        let transaction = self
            .0
            .transaction()
            .map_err(|_| "News storage is unavailable")?;
        for table in [
            "news",
            "requests",
            "cooldown",
            "earnings",
            "sec_tickers",
            "sec_meta",
            "filings",
            "fundamentals",
            "portfolio_news",
            "insider_trades",
            "market_news",
        ] {
            transaction
                .execute(&format!("DELETE FROM {table}"), [])
                .map_err(|_| "Could not clear news storage")?;
        }
        transaction
            .commit()
            .map_err(|_| "Could not clear news storage")?;
        self.0
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(|_| "Could not compact cleared news storage".into())
    }
    fn attempted(&self, symbol: &str) -> Result<Option<i64>, String> {
        self.0
            .query_row(
                "SELECT attempted FROM news WHERE symbol=?1",
                [symbol],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "News storage is unavailable".into())
    }

    /// Records a news refresh before it is sent, at most once per ticker per interval.
    pub fn begin_refresh(&mut self, symbol: &str, now: i64) -> Result<(), String> {
        if self
            .attempted(symbol)?
            .is_some_and(|at| at <= now && now - at < NEWS_REFRESH_INTERVAL)
        {
            return Err("News was just refreshed; try again in a minute".into());
        }
        let tx = self
            .0
            .transaction()
            .map_err(|_| "Could not save news attempt")?;
        tx.execute("INSERT INTO news (symbol, attempted, warning) VALUES (?1, ?2, ?3) ON CONFLICT(symbol) DO UPDATE SET attempted=excluded.attempted, warning=excluded.warning",
            params![symbol, now, "Last news refresh failed"]).map_err(|_| "Could not save news attempt")?;
        tx.execute("DELETE FROM news WHERE symbol IN (SELECT symbol FROM news ORDER BY attempted DESC, symbol LIMIT -1 OFFSET 128)", []).map_err(|_| "Could not bound news cache")?;
        tx.commit()
            .map_err(|_| "Could not save news attempt".into())
    }

    /// When any holding's stories were last saved, in Unix seconds.
    pub fn newest_saved(&self) -> Result<Option<i64>, String> {
        self.0
            .query_row("SELECT MAX(saved) FROM news", [], |row| row.get(0))
            .map_err(|_| "News storage is unavailable".to_string())
    }

    /// Records a portfolio news refresh before it is sent; `false` when one ran within the
    /// interval.
    pub fn begin_portfolio_refresh(&self, now: i64) -> Result<bool, String> {
        let last: Option<i64> = self
            .0
            .query_row("SELECT attempted FROM portfolio_news WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|_| "News storage is unavailable")?;
        if last.is_some_and(|at| at <= now && now - at < PORTFOLIO_NEWS_INTERVAL) {
            return Ok(false);
        }
        self.0
            .execute(
                "INSERT OR REPLACE INTO portfolio_news VALUES (1, ?1)",
                [now],
            )
            .map_err(|_| "Could not save news attempt")?;
        Ok(true)
    }

    fn saved(&self, symbol: &str) -> Result<SavedRow, String> {
        let row: Option<(Option<String>, Option<i64>, Option<String>)> = self
            .0
            .query_row(
                "SELECT payload, saved, warning FROM news WHERE symbol=?1",
                [symbol],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|_| "News storage is unavailable")?;
        let (payload, saved, warning) = row.unwrap_or_default();
        let key = self
            .company(symbol)?
            .0
            .and_then(|company| company_key(&company.name));
        // Stories saved in another format (such as Alpha Vantage's) read as missing.
        let articles = payload
            .as_deref()
            .filter(|s| s.len() <= 2_000_000)
            .and_then(|s| serde_json::from_str::<SavedNews>(s).ok())
            .map(|saved| {
                saved
                    .articles
                    .into_iter()
                    .filter_map(|article| classify_story(article, symbol, key.as_deref()))
                    .collect::<Vec<_>>()
            });
        let saved = saved.filter(|_| articles.is_some());
        Ok((articles.unwrap_or_default(), saved, warning))
    }

    /// Saved stories for a ticker, classified, newest first.
    pub fn articles(&self, symbol: &str) -> Result<Vec<MarketNewsArticle>, String> {
        let mut articles = self.saved(symbol)?.0;
        articles.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.url.cmp(&b.url)));
        Ok(articles)
    }

    pub fn result(
        &self,
        symbol: &str,
        now: i64,
        warning: Option<String>,
    ) -> Result<NewsResult, String> {
        let (_, saved, prior_warning) = self.saved(symbol)?;
        let saved_at = saved
            .filter(|at| *at <= now)
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            .map(|at| at.to_rfc3339());
        let articles = if saved_at.is_some() {
            self.articles(symbol)?
        } else {
            Vec::new()
        };
        let can_refresh = self
            .attempted(symbol)?
            .is_none_or(|at| at > now || now - at >= NEWS_REFRESH_INTERVAL);
        let (company, filings_warning) = self.company(symbol)?;
        let cutoff = chrono::DateTime::from_timestamp(now - FILING_DAYS * DAY, 0)
            .map(|at| at.to_rfc3339())
            .unwrap_or_default();
        Ok(NewsResult {
            articles,
            saved_at,
            warning: warning.or(prior_warning),
            can_refresh,
            filings: company
                .as_ref()
                .map(|company| {
                    company
                        .filings
                        .iter()
                        .filter(|filing| filing.filed_at >= cutoff)
                        .map(|filing| SecFiling {
                            title: filing_title(filing),
                            ..filing.clone()
                        })
                        .collect()
                })
                .unwrap_or_default(),
            company: company
                .map(|company| company.name)
                .filter(|name| !name.is_empty()),
            filings_saved_at: self
                .filings_saved(symbol)?
                .filter(|at| *at <= now)
                .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
                .map(|at| at.to_rfc3339()),
            filings_warning,
        })
    }

    /// Adds fetched stories to a ticker's saved ones, keeping the newest `SAVED_STORIES`, so a
    /// portfolio refresh and the ticker's own refresh build one history.
    pub fn save(
        &self,
        symbol: &str,
        articles: &[MarketNewsArticle],
        now: i64,
    ) -> Result<(), String> {
        let existing: Option<String> = self
            .0
            .query_row("SELECT payload FROM news WHERE symbol=?1", [symbol], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|_| "News storage is unavailable")?
            .flatten();
        let mut merged: Vec<MarketNewsArticle> = articles.to_vec();
        for article in existing
            .as_deref()
            .filter(|s| s.len() <= 2_000_000)
            .and_then(|s| serde_json::from_str::<SavedNews>(s).ok())
            .map(|saved| saved.articles)
            .unwrap_or_default()
        {
            if !merged.iter().any(|kept| kept.url == article.url) {
                merged.push(article);
            }
        }
        merged.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.url.cmp(&b.url)));
        merged.truncate(SAVED_STORIES);
        let payload = serde_json::to_string(&SavedNews { articles: merged })
            .map_err(|_| "Could not save news")?;
        if payload.len() > 2_000_000 {
            return Err("News response is too large to save".into());
        }
        self.0
            .execute(
                "INSERT INTO news (symbol, payload, saved, attempted, warning) VALUES (?1, ?2, ?3, 0, NULL)
                 ON CONFLICT(symbol) DO UPDATE SET payload=excluded.payload, saved=excluded.saved, warning=NULL",
                params![symbol, payload, now],
            )
            .map_err(|_| "Could not save news")?;
        Ok(())
    }

    pub fn failure(&self, symbol: &str, message: &str) -> Result<(), String> {
        self.0
            .execute(
                "UPDATE news SET warning=?2 WHERE symbol=?1",
                params![symbol, message],
            )
            .map_err(|_| "Could not save news status")?;
        Ok(())
    }

    /// Adds stories from CNBC's market feeds, whatever tickers they name, keeping the last
    /// `MARKET_NEWS_DAYS` days.
    pub fn save_market_news(&self, stories: &[MarketNewsArticle], now: i64) -> Result<(), String> {
        for story in stories {
            let payload = serde_json::to_string(story).map_err(|_| "Could not save news")?;
            self.0
                .execute(
                    "INSERT OR REPLACE INTO market_news VALUES (?1, ?2, ?3)",
                    params![story.url, story.created_at, payload],
                )
                .map_err(|_| "Could not save news")?;
        }
        let cutoff = chrono::DateTime::from_timestamp(now - MARKET_NEWS_DAYS * DAY, 0)
            .map(|at| at.to_rfc3339())
            .unwrap_or_default();
        self.0
            .execute("DELETE FROM market_news WHERE created < ?1", [cutoff])
            .map_err(|_| "Could not bound saved news")?;
        Ok(())
    }

    /// Stories from CNBC's market feeds published at or after `since`, oldest first.
    pub fn market_news(&self, since: &str) -> Result<Vec<MarketNewsArticle>, String> {
        let mut statement = self
            .0
            .prepare("SELECT payload FROM market_news WHERE created >= ?1 ORDER BY created, url")
            .map_err(|_| "News storage is unavailable")?;
        let rows = statement
            .query_map([since], |row| row.get::<_, String>(0))
            .map_err(|_| "News storage is unavailable")?;
        Ok(rows
            .filter_map(Result::ok)
            .filter_map(|payload| serde_json::from_str(&payload).ok())
            .collect())
    }
}
