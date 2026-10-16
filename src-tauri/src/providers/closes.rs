//! Daily closes for the summary: one paged Alpaca request, adjusted for splits and dividends,
//! dated by New York session, with a live price standing in for the session's close only while
//! it trades, and the symbols a page limit cut short named rather than dropped. Returns and their
//! robust typical size feed the layered model (`summary::layers`).
use super::*;
use crate::numeric::median;
use crate::session::{self, Session};

/// A symbol's daily closes, oldest first.
pub type CloseSeries = [(NaiveDate, f64)];
/// Each symbol's daily closes, oldest first.
pub type DailyCloses = BTreeMap<String, Vec<(NaiveDate, f64)>>;

/// A floor on the yardstick, so a nearly flat fund doesn't make every wobble look unusual.
const MIN_SIGMA_PCT: f64 = 0.25;

/// Daily percent returns from consecutive closes. A missing day joins the next return.
pub(crate) fn returns(closes: &[(NaiveDate, f64)]) -> BTreeMap<NaiveDate, f64> {
    closes
        .windows(2)
        .filter(|pair| pair[0].1 > 0.0 && pair[1].1 > 0.0 && pair[0].0 < pair[1].0)
        .map(|pair| (pair[1].0, (pair[1].1 / pair[0].1 - 1.0) * 100.0))
        .collect()
}

/// A standard deviation that the very events being measured cannot inflate: 1.4826 times the
/// median absolute deviation, which equals the standard deviation for normal returns.
pub(crate) fn robust_sigma(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    let Some(center) = median(&mut values) else {
        return MIN_SIGMA_PCT;
    };
    let mut deviations: Vec<f64> = values.iter().map(|value| (value - center).abs()).collect();
    median(&mut deviations).map_or(MIN_SIGMA_PCT, |deviation| {
        (1.4826 * deviation).max(MIN_SIGMA_PCT)
    })
}

/// Puts a price observed during a session's regular hours in as that session's close, so a
/// period's model counts today the way its headline change does: a newer session is added,
/// and its partial bar is replaced. A pre-market or after-hours price is never a close.
pub(crate) fn overlay_latest(series: &mut Vec<(NaiveDate, f64)>, price: f64, as_of: &str) {
    let Some(at) = session::instant(as_of) else {
        return;
    };
    let Some(trading) = Session::on(session::new_york_date(at)) else {
        return;
    };
    if at < trading.open() || at >= trading.close() || !price.is_finite() || price <= 0.0 {
        return;
    }
    let day = trading.date;
    match series.last_mut() {
        Some(last) if last.0 == day => last.1 = price,
        Some(last) if last.0 > day => {}
        _ => series.push((day, price)),
    }
}

/// Daily closes by symbol, and the symbols whose history the page limit cut short.
#[derive(Debug, Default)]
pub(crate) struct Closes {
    pub series: DailyCloses,
    /// Shares traded each session, on the request's feed: comparable with itself, not a
    /// consolidated count.
    pub volumes: BTreeMap<String, BTreeMap<NaiveDate, f64>>,
    pub truncated: Vec<String>,
}

/// When the pages ran out before the answer did: Alpaca returns symbols in order, each oldest
/// first, so the symbols never reached and the one cut off mid-history end before the others.
fn cut_short(requested: &[String], series: &DailyCloses) -> Vec<String> {
    let latest = series
        .values()
        .filter_map(|series| series.last().map(|(day, _)| *day))
        .max();
    requested
        .iter()
        .filter(|symbol| {
            series
                .get(*symbol)
                .and_then(|series| series.last())
                .is_none_or(|(day, _)| Some(*day) < latest)
        })
        .cloned()
        .collect()
}

impl Providers {
    /// Every symbol's daily closes from `from` through `to`, in one paginated Alpaca request.
    pub(crate) async fn daily_closes(
        &self,
        symbols: Vec<String>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Closes, String> {
        /// Pages beyond this are left out rather than holding the summary open, and the
        /// symbols they would have held are reported.
        const MAX_PAGES: usize = 10;
        let symbols = normalize_stock_symbols(symbols);
        let mut closes: DailyCloses = BTreeMap::new();
        let mut volumes: BTreeMap<String, BTreeMap<NaiveDate, f64>> = BTreeMap::new();
        if symbols.is_empty() {
            return Ok(Closes::default());
        }
        let credentials = self.alpaca_credentials()?;
        // A bare date for `end` can cut off that day's bar (stamped at the start of its New York
        // day); an exact time includes it. Never ask past now.
        let to = Utc::now()
            .min(
                to.and_hms_opt(23, 59, 59)
                    .map(|end| end.and_utc())
                    .unwrap_or_else(Utc::now),
            )
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let (from, joined) = (from.to_string(), symbols.join(","));
        let query = [
            ("symbols", joined.as_str()),
            ("timeframe", "1Day"),
            ("start", from.as_str()),
            ("end", to.as_str()),
            ("adjustment", "all"),
            ("feed", "iex"),
            ("limit", "10000"),
        ];
        let complete = self
            .alpaca_pages(
                &credentials,
                "/v2/stocks/bars",
                &query,
                MAX_PAGES,
                |payload| {
                    for (symbol, bars) in payload["bars"].as_object().into_iter().flatten() {
                        let series = closes.entry(symbol.clone()).or_default();
                        for bar in bars.as_array().into_iter().flatten() {
                            // A daily bar is stamped at the start of its New York day.
                            let (Some(day), Some(close)) = (
                                bar["t"].as_str().and_then(session::date_of),
                                bar["c"]
                                    .as_f64()
                                    .filter(|close| close.is_finite() && *close > 0.0),
                            ) else {
                                continue;
                            };
                            series.push((day, close));
                            if let Some(volume) = bar["v"].as_f64().filter(|volume| *volume >= 0.0)
                            {
                                volumes
                                    .entry(symbol.clone())
                                    .or_default()
                                    .insert(day, volume);
                            }
                        }
                    }
                    Ok(())
                },
            )
            .await?;
        for series in closes.values_mut() {
            series.sort_by_key(|(day, _)| *day);
            series.dedup_by_key(|(day, _)| *day);
        }
        let truncated = if !complete {
            cut_short(&symbols, &closes)
        } else {
            Vec::new()
        };
        // A history cut off mid-way would misstate its symbol's year; it is left out and named.
        for symbol in &truncated {
            closes.remove(symbol);
            volumes.remove(symbol);
        }
        Ok(Closes {
            series: closes,
            volumes,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(offset: i64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Duration::days(offset)
    }

    #[test]
    fn todays_price_joins_the_daily_closes_once_and_only_during_the_session() {
        let date = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
        // Thursday and Friday, October 1 and 2, 2026, Eastern daylight time.
        let mut series = vec![(date("2026-09-30"), 100.0), (date("2026-10-01"), 101.0)];
        // A trade at 2:55 pm New York time on Friday becomes Friday's close.
        overlay_latest(&mut series, 108.0, "2026-10-02T18:55:00Z");
        assert_eq!(series.last(), Some(&(date("2026-10-02"), 108.0)));
        // A later trade the same session replaces it rather than adding a day.
        overlay_latest(&mut series, 109.0, "2026-10-02T19:55:00Z");
        assert_eq!(series.len(), 3);
        assert_eq!(series.last(), Some(&(date("2026-10-02"), 109.0)));
        // After-hours and pre-market prices are never a close, nor is a weekend quote.
        overlay_latest(&mut series, 120.0, "2026-10-02T21:30:00Z");
        overlay_latest(&mut series, 120.0, "2026-10-05T12:00:00Z");
        overlay_latest(&mut series, 120.0, "2026-10-03T15:00:00Z");
        // An older or unusable observation changes nothing.
        overlay_latest(&mut series, 90.0, "2026-10-01T18:00:00Z");
        overlay_latest(&mut series, f64::NAN, "2026-10-05T18:00:00Z");
        overlay_latest(&mut series, 90.0, "not a time");
        assert_eq!(series.len(), 3);
        assert_eq!(series.last(), Some(&(date("2026-10-02"), 109.0)));
    }

    #[test]
    fn symbols_the_page_limit_cut_short_are_named_not_dropped_silently() {
        let requested = vec!["AAA".to_string(), "BBB".into(), "CCC".into()];
        // Pages ran out partway through BBB, before CCC.
        let series = BTreeMap::from([
            ("AAA".to_string(), vec![(day(0), 1.0), (day(9), 1.0)]),
            ("BBB".to_string(), vec![(day(0), 1.0), (day(4), 1.0)]),
        ]);
        assert_eq!(cut_short(&requested, &series), ["BBB", "CCC"]);
    }

    #[test]
    fn returns_join_consecutive_closes() {
        let closes = [(day(0), 100.0), (day(1), 110.0), (day(3), 99.0)];
        let returns = returns(&closes);
        assert_eq!(returns.len(), 2);
        assert!((returns[&day(1)] - 10.0).abs() < 1e-9);
        assert!((returns[&day(3)] + 10.0).abs() < 1e-9);
    }
}
