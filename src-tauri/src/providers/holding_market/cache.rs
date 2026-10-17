//! Verified security history saved in `market-prices.sqlite3`, a disposable cache.
use super::*;

// A replace-only, bounded cache avoids mixing adjustment epochs or retaining a
// cancelled trade. Failed/incomplete downloads never replace the prior result.
pub(super) fn cache_connection(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|_| "Price cache unavailable")?;
    crate::database::secure_file(path)?;
    db.busy_timeout(StdDuration::from_secs(2))
        .map_err(|_| "Price cache unavailable")?;
    db.execute_batch(
        "PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS price_history (
            symbol TEXT NOT NULL, range_seconds INTEGER NOT NULL, fetched_at INTEGER NOT NULL,
            payload TEXT NOT NULL CHECK(json_valid(payload)),
            PRIMARY KEY(symbol, range_seconds)
        ) STRICT;",
    )
    .map_err(|_| "Price cache unavailable")?;
    Ok(db)
}

/// Every symbol's saved history for a range, or nothing when any is missing.
pub fn cached_histories(
    path: &Path,
    symbols: &[String],
    range: u64,
) -> Option<BTreeMap<String, PriceHistory>> {
    symbols
        .iter()
        .map(|symbol| Some((symbol.clone(), cached_history(path, symbol, range)?)))
        .collect()
}

pub fn cached_history(path: &Path, symbol: &str, range: u64) -> Option<PriceHistory> {
    let db = cache_connection(path).ok()?;
    let payload: String = db
        .query_row(
            "SELECT payload FROM price_history WHERE symbol=?1 AND range_seconds=?2",
            params![symbol, range],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    let mut history: PriceHistory = serde_json::from_str(&payload).ok()?;
    if history.symbol != symbol
        || history.range != range
        || history.adjustment != "split"
        || history.resolution != resolution(range).ok()?.1
        || history.feeds != history_feeds(history.resolution)
        || history.start >= history.end
        || history.calendar.as_ref().is_some_and(|days| {
            days.windows(2).any(|pair| pair[0].date >= pair[1].date)
                || days.iter().any(|day| {
                    chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").is_err()
                        || day.open < 14400
                        || day.close <= day.open
                        || day.close > 72000
                })
        })
        || history
            .bars
            .windows(2)
            .any(|w| (w[0].time, &w[0].feed) >= (w[1].time, &w[1].feed))
        || history.bars.iter().any(|b| {
            b.time < history.start
                || b.end_time > history.end
                || b.end_time <= b.time
                || !history.feeds.contains(&b.feed)
                || !b.close.is_finite()
                || b.close <= 0.0
                || !b.open.is_finite()
                || !b.high.is_finite()
                || !b.low.is_finite()
                || b.low <= 0.0
                || b.low > b.open.min(b.close)
                || b.high < b.open.max(b.close)
                || !b.volume.is_finite()
                || b.volume < 0.0
        })
    {
        return None;
    }
    history.cached = true;
    Some(history)
}

pub fn clear_cache(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let db = cache_connection(path)?;
    db.pragma_update(None, "secure_delete", true)
        .map_err(|_| "Could not protect price-history erasure")?;
    db.execute("DELETE FROM price_history", [])
        .map_err(|_| "Could not clear price history")?;
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        .map_err(|_| "Could not compact cleared price history".into())
}

pub(super) fn save_history(path: &Path, history: &PriceHistory) -> Result<(), String> {
    let mut db = cache_connection(path)?;
    let tx = db.transaction().map_err(|_| "Price cache unavailable")?;
    let payload = serde_json::to_string(history).map_err(|_| "Invalid price history")?;
    tx.execute(
        "INSERT OR REPLACE INTO price_history VALUES (?1, ?2, ?3, ?4)",
        params![history.symbol, history.range, history.fetched_at, payload],
    )
    .map_err(|_| "Could not save price history")?;
    tx.execute(
        "DELETE FROM price_history WHERE rowid NOT IN
        (SELECT rowid FROM price_history ORDER BY fetched_at DESC LIMIT 160)",
        [],
    )
    .map_err(|_| "Could not bound price cache")?;
    tx.commit()
        .map_err(|_| "Could not commit price history".into())
}
