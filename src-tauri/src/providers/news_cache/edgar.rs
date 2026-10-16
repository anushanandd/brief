//! EDGAR's ticker map, companies with their filings, Form 4 trades, and reported quarters.
use super::*;

/// EDGAR's ticker list changes rarely.
pub(super) const SEC_TICKERS_MAX_AGE: i64 = 7 * DAY;
/// A company reports quarterly; a weekly check finds a new quarter soon after it is filed.
pub(super) const FUNDAMENTALS_MAX_AGE: i64 = 7 * DAY;

impl NewsCache {
    pub fn sec_tickers_fresh(&self, now: i64) -> Result<bool, String> {
        self.0
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sec_meta WHERE id=1 AND saved <= ?1 AND saved > ?2)",
                params![now, now - SEC_TICKERS_MAX_AGE],
                |r| r.get(0),
            )
            .map_err(|_| "SEC ticker list is unavailable".into())
    }

    pub fn save_sec_tickers(&mut self, tickers: &[(String, u64)], now: i64) -> Result<(), String> {
        let tx = self
            .0
            .transaction()
            .map_err(|_| "Could not save SEC ticker list")?;
        tx.execute("DELETE FROM sec_tickers", [])
            .map_err(|_| "Could not save SEC ticker list")?;
        {
            let mut insert = tx
                .prepare("INSERT OR IGNORE INTO sec_tickers VALUES (?1, ?2)")
                .map_err(|_| "Could not save SEC ticker list")?;
            for (ticker, cik) in tickers {
                insert
                    .execute(params![ticker, cik])
                    .map_err(|_| "Could not save SEC ticker list")?;
            }
        }
        tx.execute("INSERT OR REPLACE INTO sec_meta VALUES (1, ?1)", [now])
            .map_err(|_| "Could not save SEC ticker list")?;
        tx.commit()
            .map_err(|_| "Could not save SEC ticker list".into())
    }

    pub fn sec_cik(&self, symbol: &str) -> Result<Option<u64>, String> {
        self.0
            .query_row(
                "SELECT cik FROM sec_tickers WHERE ticker=?1",
                [sec_ticker(symbol)],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "SEC ticker list is unavailable".into())
    }

    /// Saves a company's filings, or that EDGAR lists no company for the ticker.
    pub fn save_filings(
        &self,
        symbol: &str,
        company: Option<&SecCompany>,
        now: i64,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(&company).map_err(|_| "Could not save filings")?;
        self.0
            .execute(
                "INSERT OR REPLACE INTO filings VALUES (?1, ?2, ?3, NULL)",
                params![symbol, payload, now],
            )
            .map_err(|_| "Could not save filings")?;
        self.0
            .execute(
                "DELETE FROM filings WHERE symbol IN (SELECT symbol FROM filings ORDER BY saved DESC, symbol LIMIT -1 OFFSET 128)",
                [],
            )
            .map_err(|_| "Could not bound saved filings")?;
        Ok(())
    }

    /// Keeps saved filings when a refresh fails.
    pub fn filings_failure(&self, symbol: &str, message: &str) -> Result<(), String> {
        self.0
            .execute(
                "INSERT INTO filings (symbol, warning) VALUES (?1, ?2) ON CONFLICT(symbol) DO UPDATE SET warning=excluded.warning",
                params![symbol, message],
            )
            .map_err(|_| "Could not save filings status")?;
        Ok(())
    }

    /// Whether EDGAR has answered for this ticker, even with no company.
    pub fn filings_checked(&self, symbol: &str) -> Result<bool, String> {
        Ok(self.filings_saved(symbol)?.is_some())
    }

    pub fn filings_saved(&self, symbol: &str) -> Result<Option<i64>, String> {
        self.0
            .query_row("SELECT saved FROM filings WHERE symbol=?1", [symbol], |r| {
                r.get(0)
            })
            .optional()
            .map(Option::flatten)
            .map_err(|_| "Saved filings are unavailable".into())
    }

    /// The saved EDGAR company for a ticker and the last filings warning.
    pub fn company(&self, symbol: &str) -> Result<(Option<SecCompany>, Option<String>), String> {
        let row: Option<(Option<String>, Option<String>)> = self
            .0
            .query_row(
                "SELECT payload, warning FROM filings WHERE symbol=?1",
                [symbol],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| "Saved filings are unavailable")?;
        let (payload, warning) = row.unwrap_or_default();
        Ok((
            payload
                .as_deref()
                .filter(|s| s.len() <= 2_000_000)
                .and_then(|s| serde_json::from_str::<Option<SecCompany>>(s).ok())
                .flatten(),
            warning,
        ))
    }

    /// A Form 4's parsed trades, when it was read before; filings never change.
    pub fn insider_trades(&self, url: &str) -> Result<Option<Vec<InsiderTrade>>, String> {
        let payload: Option<String> = self
            .0
            .query_row(
                "SELECT payload FROM insider_trades WHERE url=?1",
                [url],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "Saved insider trades are unavailable")?;
        Ok(payload.and_then(|payload| serde_json::from_str(&payload).ok()))
    }

    /// Saves a Form 4's trades, keeping the most recently read 2,000 filings.
    pub fn save_insider_trades(&self, url: &str, trades: &[InsiderTrade]) -> Result<(), String> {
        let payload = serde_json::to_string(trades).map_err(|_| "Could not save insider trades")?;
        self.0
            .execute(
                "INSERT OR REPLACE INTO insider_trades VALUES (?1, ?2, strftime('%s','now'))",
                params![url, payload],
            )
            .map_err(|_| "Could not save insider trades")?;
        self.0
            .execute(
                "DELETE FROM insider_trades WHERE url IN (SELECT url FROM insider_trades ORDER BY saved DESC, url LIMIT -1 OFFSET 2000)",
                [],
            )
            .map_err(|_| "Could not bound saved insider trades")?;
        Ok(())
    }

    /// A ticker's saved quarter when it is still usable: checked within a week, or a failed
    /// check within a day (which keeps any earlier answer). `None` means EDGAR should be asked.
    pub fn fundamentals(
        &self,
        symbol: &str,
        now: i64,
    ) -> Result<Option<Option<Fundamentals>>, String> {
        let row: Option<(Option<String>, i64, Option<i64>)> = self
            .0
            .query_row(
                "SELECT payload, saved, failed FROM fundamentals WHERE symbol=?1",
                [symbol],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|_| "Saved company figures are unavailable")?;
        let Some((payload, saved, failed)) = row else {
            return Ok(None);
        };
        let recent = |at: i64, within: i64| at > 0 && at <= now && now - at < within;
        let fresh = recent(saved, FUNDAMENTALS_MAX_AGE);
        if !fresh && !failed.is_some_and(|at| recent(at, DAY)) {
            return Ok(None);
        }
        let saved = (saved > 0)
            .then(|| {
                payload
                    .as_deref()
                    .and_then(|payload| serde_json::from_str::<Option<Fundamentals>>(payload).ok())
                    .flatten()
            })
            .flatten();
        Ok(Some(saved))
    }

    pub fn save_fundamentals(
        &self,
        symbol: &str,
        fundamentals: Option<&Fundamentals>,
        now: i64,
    ) -> Result<(), String> {
        let payload =
            serde_json::to_string(&fundamentals).map_err(|_| "Could not save company figures")?;
        self.0
            .execute(
                "INSERT OR REPLACE INTO fundamentals VALUES (?1, ?2, ?3, NULL)",
                params![symbol, payload, now],
            )
            .map_err(|_| "Could not save company figures")?;
        Ok(())
    }

    pub fn fundamentals_failure(&self, symbol: &str, now: i64) -> Result<(), String> {
        self.0
            .execute(
                "INSERT INTO fundamentals VALUES (?1, NULL, 0, ?2)
                 ON CONFLICT(symbol) DO UPDATE SET failed=excluded.failed",
                params![symbol, now],
            )
            .map_err(|_| "Could not save company figures")?;
        Ok(())
    }
}
