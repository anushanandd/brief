//! Daily closes kept between summaries: settled sessions never change, so each symbol's history
//! is fetched once and then only from its last saved day (`providers::closes`). What a symbol's
//! saved history covers is kept beside it, so a gap is never mistaken for a quiet day.
use super::*;
use chrono::NaiveDate;

/// One saved session: its close and, when Alpaca gave one, its volume.
pub(crate) type SavedClose = (NaiveDate, f64, Option<f64>);
/// The first and last sessions a symbol's saved history covers, and its closes.
type SavedHistory = (Option<(NaiveDate, NaiveDate)>, Vec<SavedClose>);

impl NewsCache {
    /// A symbol's saved closes from `from`, and the first and last sessions its saved history
    /// covers; `None` when nothing is saved.
    pub(crate) fn closes(&self, symbol: &str, from: NaiveDate) -> Result<SavedHistory, String> {
        const ERROR: &str = "Saved prices are unavailable";
        let covered: Option<(String, String)> = self
            .0
            .query_row(
                "SELECT first, last FROM close_history WHERE symbol=?1",
                [symbol],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| ERROR)?;
        let covered = covered.and_then(|(first, last)| {
            Some((
                NaiveDate::parse_from_str(&first, "%Y-%m-%d").ok()?,
                NaiveDate::parse_from_str(&last, "%Y-%m-%d").ok()?,
            ))
        });
        let mut statement = self
            .0
            .prepare(
                "SELECT day, close, volume FROM closes WHERE symbol=?1 AND day>=?2 ORDER BY day",
            )
            .map_err(|_| ERROR)?;
        let rows = statement
            .query_map(params![symbol, from.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                ))
            })
            .map_err(|_| ERROR)?
            .filter_map(Result::ok)
            .filter_map(|(day, close, volume)| {
                Some((
                    NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok()?,
                    close,
                    volume,
                ))
            })
            .collect();
        Ok((covered, rows))
    }

    /// Saves settled closes for `symbol` covering `first` through `last`, widening what its
    /// saved history covers. `replace` drops what was saved first, when a split or dividend
    /// has adjusted the whole history.
    pub(crate) fn save_closes(
        &self,
        symbol: &str,
        rows: &[SavedClose],
        (first, last): (NaiveDate, NaiveDate),
        replace: bool,
    ) -> Result<(), String> {
        const ERROR: &str = "Could not save prices";
        let transaction = self.0.unchecked_transaction().map_err(|_| ERROR)?;
        if replace {
            transaction
                .execute("DELETE FROM closes WHERE symbol=?1", [symbol])
                .map_err(|_| ERROR)?;
            transaction
                .execute("DELETE FROM close_history WHERE symbol=?1", [symbol])
                .map_err(|_| ERROR)?;
        }
        for (day, close, volume) in rows {
            transaction
                .execute(
                    "INSERT OR REPLACE INTO closes VALUES (?1, ?2, ?3, ?4)",
                    params![symbol, day.to_string(), close, volume],
                )
                .map_err(|_| ERROR)?;
        }
        transaction
            .execute(
                "INSERT INTO close_history VALUES (?1, ?2, ?3)
                 ON CONFLICT(symbol) DO UPDATE SET first=min(first, excluded.first), last=max(last, excluded.last)",
                params![symbol, first.to_string(), last.to_string()],
            )
            .map_err(|_| ERROR)?;
        transaction.commit().map_err(|_| ERROR.to_owned())
    }
}
