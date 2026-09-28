//! Local news and a conservative rolling budget, separate from financial state.
use super::{parse_alpha_vantage_news, MarketNewsArticle};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

const DAY: i64 = 86_400;
const MINUTE: i64 = 60;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsResult {
    pub articles: Vec<MarketNewsArticle>,
    pub saved_at: Option<String>,
    pub warning: Option<String>,
    pub requests_remaining: usize,
    pub can_refresh: bool,
}

pub struct NewsCache(Connection);

impl NewsCache {
    pub fn new(path: Option<&Path>) -> Result<Self, String> {
        let mut db = match path {
            Some(path) => Connection::open(path),
            None => Connection::open_in_memory(),
        }
        .map_err(|_| "News storage is unavailable")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS news (symbol TEXT PRIMARY KEY, payload TEXT, saved INTEGER, attempted INTEGER NOT NULL, warning TEXT);
            CREATE TABLE IF NOT EXISTS requests (at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL, period TEXT NOT NULL DEFAULT 'minute');
            CREATE TABLE IF NOT EXISTS earnings (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL, saved INTEGER NOT NULL);")
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

    fn budget(&self, now: i64, news: bool) -> Result<(usize, Option<String>), String> {
        let used: usize = self
            .0
            .query_row(
                "SELECT count(*) FROM requests WHERE at > ?1",
                [now - DAY],
                |r| r.get(0),
            )
            .map_err(|_| "Request budget is unavailable")?;
        let cooldown: Option<(i64, String)> = self
            .0
            .query_row("SELECT until, period FROM cooldown WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(|_| "Request budget is unavailable")?;
        Ok((
            (if news { 20usize } else { 25usize }).saturating_sub(used),
            cooldown
                .filter(|(until, _)| *until > now)
                .map(|(_, period)| period),
        ))
    }

    // Persist before sending: failed requests count too, including across restarts.
    pub fn reserve(&mut self, symbol: Option<&str>, now: i64) -> Result<(), String> {
        let (remaining, cooldown) = self.budget(now, symbol.is_some())?;
        if let Some(period) = cooldown {
            return Err(match period.as_str() {
                "daily" | "daily-confirmed" => "Alpha Vantage still reports a daily limit for this API key. Brief’s local budget does not reflect the provider’s allowance. Saved news remains available; try again after the provider resets the key.",
                "minute" | "daily-probe" => "Alpha Vantage temporarily rate-limited requests. Refresh again in about a minute; saved news remains available.",
                _ => "Alpha Vantage temporarily rate-limited requests. Try again after the provider cooldown; saved news remains available.",
            }.into());
        }
        if remaining == 0 {
            return Err(if symbol.is_some() {
                "Brief’s news budget is used up for the last 24 hours. Five of the 25 free requests are reserved for earnings and credential checks. Saved news remains available."
            } else {
                "Brief’s 25-request budget is used up for the last 24 hours. Try again later."
            }.into());
        }
        let tx = self
            .0
            .transaction()
            .map_err(|_| "Could not save request budget")?;
        tx.execute("DELETE FROM requests WHERE at <= ?1", [now - DAY])
            .map_err(|_| "Could not save request budget")?;
        tx.execute("INSERT INTO requests VALUES (?1)", [now])
            .map_err(|_| "Could not save request budget")?;
        if let Some(symbol) = symbol {
            tx.execute("INSERT INTO news (symbol, attempted, warning) VALUES (?1, ?2, ?3) ON CONFLICT(symbol) DO UPDATE SET attempted=excluded.attempted, warning=excluded.warning",
                params![symbol, now, "The last news refresh did not complete. Use Refresh news to try again."]).map_err(|_| "Could not save news attempt")?;
            tx.execute("DELETE FROM news WHERE symbol IN (SELECT symbol FROM news ORDER BY attempted DESC, symbol LIMIT -1 OFFSET 128)", []).map_err(|_| "Could not bound news cache")?;
        }
        tx.commit()
            .map_err(|_| "Could not save request budget".into())
    }

    pub fn result(
        &self,
        symbol: &str,
        now: i64,
        warning: Option<String>,
    ) -> Result<NewsResult, String> {
        let row: Option<(Option<String>, Option<i64>, Option<String>)> = self
            .0
            .query_row(
                "SELECT payload, saved, warning FROM news WHERE symbol=?1",
                [symbol],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|_| "News storage is unavailable")?;
        let (payload, saved, prior_warning) = row.unwrap_or_default();
        let articles = payload
            .as_deref()
            .filter(|s| s.len() <= 2_000_000)
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .and_then(|v| parse_alpha_vantage_news(&v, symbol).ok());
        let saved_at = saved
            .filter(|at| *at <= now && articles.is_some())
            .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
            .map(|at| at.to_rfc3339());
        let (requests_remaining, cooldown) = self.budget(now, true)?;
        Ok(NewsResult {
            articles: if saved_at.is_some() {
                articles.unwrap_or_default()
            } else {
                vec![]
            },
            saved_at,
            warning: match cooldown.as_deref() {
                Some("daily") | Some("daily-confirmed") => Some("Alpha Vantage still reports a daily limit for this API key. Brief’s local budget does not reflect the provider’s allowance. Saved stories remain available; try again after the provider resets the key.".into()),
                Some("minute") | Some("daily-probe") => Some("Alpha Vantage temporarily rate-limited requests. Refresh again in about a minute; saved stories remain available.".into()),
                Some(_) => Some("Alpha Vantage temporarily rate-limited requests. Try again after the provider cooldown; saved stories remain available.".into()),
                None => warning.or(prior_warning),
            },
            requests_remaining,
            can_refresh: requests_remaining > 0 && cooldown.is_none(),
        })
    }

    pub fn save(&self, symbol: &str, payload: &Value, now: i64) -> Result<(), String> {
        let payload = serde_json::to_string(payload).map_err(|_| "Could not save news")?;
        if payload.len() > 2_000_000 {
            return Err("News response is too large to save".into());
        }
        self.0
            .execute(
                "UPDATE news SET payload=?2, saved=?3, warning=NULL WHERE symbol=?1",
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

    pub fn provider_limit(&mut self, payload: &Value, now: i64) -> Result<(), String> {
        let message = payload
            .get("Note")
            .or_else(|| payload.get("Information"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if message.contains("rate limit")
            || message.contains("call frequency")
            || message.contains("requests per day")
        {
            let daily = message.contains("per day") || message.contains("daily");
            // A probe belongs to one rate-limit incident. An old cooldown row
            // must not turn every later daily report into a full-day pause.
            let recently_probed: bool = self
                .0
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM cooldown WHERE period='daily-probe' AND until >= ?1 - ?2)",
                    params![now, MINUTE],
                    |r| r.get(0),
                )
                .map_err(|_| "Could not read provider quota status")?;
            let first_daily_report = daily && !recently_probed && self.budget(now, false)?.0 > 0;
            self.save_cooldown(
                if daily && !first_daily_report {
                    DAY
                } else {
                    MINUTE
                },
                if first_daily_report {
                    "daily-probe"
                } else if daily {
                    "daily-confirmed"
                } else {
                    "minute"
                },
                now,
            )?;
        }
        Ok(())
    }

    pub fn provider_throttle(&mut self, retry_after: Option<i64>, now: i64) -> Result<(), String> {
        let delay = retry_after.unwrap_or(MINUTE).clamp(1, DAY);
        let period = if delay >= DAY {
            "daily-confirmed"
        } else if delay <= MINUTE {
            "minute"
        } else {
            "temporary"
        };
        self.save_cooldown(delay, period, now)
    }

    fn save_cooldown(&mut self, delay: i64, period: &str, now: i64) -> Result<(), String> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO cooldown (id, until, period) VALUES (1, ?1, ?2)",
                params![now + delay, period],
            )
            .map_err(|_| "Could not save provider quota status")?;
        Ok(())
    }

    pub fn earnings(&self, now: i64) -> Result<Option<String>, String> {
        self.0
            .query_row(
                "SELECT body FROM earnings WHERE id=1 AND saved <= ?1 AND saved > ?2",
                params![now, now - 6 * 3600],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "Earnings cache is unavailable".into())
    }

    pub fn save_earnings(&self, body: &str, now: i64) -> Result<(), String> {
        self.0
            .execute(
                "INSERT OR REPLACE INTO earnings VALUES (1, ?1, ?2)",
                params![body, now],
            )
            .map_err(|_| "Could not save earnings")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_news_contract_retains_saved_scores_after_failure() {
        let mut cache = NewsCache::new(None).unwrap();
        let now = 1789992000;
        cache.reserve(Some("TEST"), now).unwrap();
        let payload = serde_json::json!({"feed": [{
            "title": "Synthetic news", "summary": "Synthetic evidence", "source": "Example",
            "url": "https://example.com/story", "time_published": "20260921T120000",
            "ticker_sentiment": [{"ticker":"TEST", "relevance_score":"0.91", "ticker_sentiment_score":"-0.3", "ticker_sentiment_label":"Somewhat-Bearish"}]
        }]});
        cache.save("TEST", &payload, now).unwrap();
        cache.failure("TEST", "Synthetic refresh failure").unwrap();
        let result = cache.result("TEST", now, None).unwrap();
        let fixture: Value =
            serde_json::from_str(include_str!("../../../src/data/fixtures/native-news.json"))
                .unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), fixture);
        assert!(cache
            .result("OTHER", now, None)
            .unwrap()
            .articles
            .is_empty());
    }

    #[test]
    fn failed_reservation_does_not_spend_budget_or_replace_news() {
        let mut cache = NewsCache::new(None).unwrap();
        cache.0.execute_batch("CREATE TRIGGER fail_budget BEFORE INSERT ON requests BEGIN SELECT RAISE(ABORT, 'synthetic'); END;").unwrap();
        assert!(cache.reserve(Some("TEST"), 100).is_err());
        assert_eq!(
            cache.result("TEST", 101, None).unwrap().requests_remaining,
            20
        );
    }

    #[test]
    fn cache_bounds_tickers_without_expiring_saved_stories() {
        let mut cache = NewsCache::new(None).unwrap();
        for index in 0..130 {
            let now = (index + 1) * DAY;
            cache.reserve(Some(&format!("TEST{index}")), now).unwrap();
            cache
                .save(
                    &format!("TEST{index}"),
                    &serde_json::json!({"feed": []}),
                    now,
                )
                .unwrap();
        }
        let count: usize = cache
            .0
            .query_row("SELECT count(*) FROM news", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 128);
        assert!(cache
            .result("TEST0", 131 * DAY, None)
            .unwrap()
            .saved_at
            .is_none());
        assert!(cache
            .result("TEST2", 131 * DAY, None)
            .unwrap()
            .saved_at
            .is_some());
    }

    #[test]
    fn persisted_budget_retains_news_and_reserves_earnings_capacity() {
        let path =
            std::env::temp_dir().join(format!("brief-news-{}.sqlite3", uuid::Uuid::new_v4()));
        let now = 1_800_000_000;
        {
            let mut cache = NewsCache::new(Some(&path)).unwrap();
            cache.reserve(Some("TEST"), now).unwrap();
            cache
                .save("TEST", &serde_json::json!({"feed": []}), now)
                .unwrap();
        }
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        assert!(cache
            .result("TEST", now + 1, None)
            .unwrap()
            .saved_at
            .is_some());
        for _ in 1..20 {
            cache.reserve(Some("TEST"), now + 1).unwrap();
        }
        assert!(cache.reserve(Some("OTHER"), now + 1).is_err());
        for _ in 0..5 {
            cache.reserve(None, now + 1).unwrap();
        }
        assert!(cache.reserve(None, now + 1).is_err());
        cache.failure("TEST", "Synthetic failure").unwrap();
        let saved = cache.result("TEST", now + 2, None).unwrap();
        assert!(saved.saved_at.is_some());
        assert_eq!(saved.warning.as_deref(), Some("Synthetic failure"));
        assert!(!saved.can_refresh);
        cache.reserve(Some("OTHER"), now + DAY + 2).unwrap();
        drop(cache);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn provider_quota_cooldown_survives_reopens_and_failed_saves_preserve_news() {
        let path =
            std::env::temp_dir().join(format!("brief-news-{}.sqlite3", uuid::Uuid::new_v4()));
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        cache.reserve(Some("TEST"), 100).unwrap();
        cache
            .save("TEST", &serde_json::json!({"feed": []}), 100)
            .unwrap();
        cache.0.execute_batch("CREATE TRIGGER fail_news BEFORE UPDATE OF payload ON news BEGIN SELECT RAISE(ABORT, 'synthetic'); END;").unwrap();
        cache.reserve(Some("TEST"), 101).unwrap();
        assert!(cache
            .save("TEST", &serde_json::json!({"feed": []}), 101)
            .is_err());
        cache
            .provider_limit(
                &serde_json::json!({"Information": "25 requests per day rate limit"}),
                101,
            )
            .unwrap();
        drop(cache);
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        assert!(cache.reserve(None, 102).is_err());
        cache
            .provider_limit(
                &serde_json::json!({"Information": "25 requests per day rate limit"}),
                161,
            )
            .unwrap();
        assert!(cache.reserve(None, 162).is_err());
        assert!(cache
            .result("TEST", 162, None)
            .unwrap()
            .warning
            .unwrap()
            .contains("still reports a daily limit"));
        assert!(cache
            .result("TEST", 102, None)
            .unwrap()
            .saved_at
            .unwrap()
            .contains("00:01:40"));
        assert!(cache.reserve(None, 161 + DAY).is_ok());
        let next_incident = 161 + DAY + 1;
        cache
            .provider_limit(
                &serde_json::json!({"Information": "25 requests per day rate limit"}),
                next_incident,
            )
            .unwrap();
        assert!(cache.reserve(None, next_incident + 1).is_err());
        assert!(cache.reserve(None, next_incident + MINUTE).is_ok());
        cache
            .provider_limit(
                &serde_json::json!({"Information": "25 requests per day rate limit"}),
                next_incident + MINUTE,
            )
            .unwrap();
        assert!(cache.reserve(None, next_incident + MINUTE + 1).is_err());
        drop(cache);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unspecified_rate_limits_and_legacy_cooldowns_last_one_minute() {
        let mut cache = NewsCache::new(None).unwrap();
        cache
            .provider_limit(&serde_json::json!({"Note": "rate limit"}), 100)
            .unwrap();
        let paused = cache.result("TEST", 101, None).unwrap();
        assert!(!paused.can_refresh);
        assert!(paused.warning.unwrap().contains("about a minute"));
        assert!(cache.reserve(None, 159).is_err());
        assert!(cache.reserve(None, 160).is_ok());

        let path = std::env::temp_dir().join(format!(
            "brief-legacy-news-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE requests (at INTEGER NOT NULL);
             CREATE TABLE cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL);
             INSERT INTO requests VALUES (100);
             INSERT INTO cooldown VALUES (1, 86500);",
        )
        .unwrap();
        drop(db);
        let mut migrated = NewsCache::new(Some(&path)).unwrap();
        assert!(migrated.reserve(None, 159).is_err());
        assert!(migrated.reserve(None, 160).is_ok());
        drop(migrated);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_daily_pause_gets_one_short_retry() {
        let path = std::env::temp_dir().join(format!(
            "brief-old-daily-news-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = Connection::open(&path).unwrap();
        let now = 1_800_000_000;
        db.execute_batch(
            "CREATE TABLE requests (at INTEGER NOT NULL);
             CREATE TABLE cooldown (id INTEGER PRIMARY KEY CHECK(id=1), until INTEGER NOT NULL, period TEXT NOT NULL);",
        )
        .unwrap();
        db.execute("INSERT INTO requests VALUES (?1)", [now - MINUTE])
            .unwrap();
        db.execute("INSERT INTO cooldown VALUES (1, ?1, 'daily')", [now + DAY])
            .unwrap();
        drop(db);
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        assert!(cache.reserve(None, now).is_ok());
        cache
            .provider_limit(
                &serde_json::json!({"Information": "25 requests per day rate limit"}),
                now,
            )
            .unwrap();
        drop(cache);
        let mut cache = NewsCache::new(Some(&path)).unwrap();
        assert!(cache.reserve(None, now + MINUTE).is_err());
        drop(cache);
        std::fs::remove_file(path).unwrap();
    }
}
