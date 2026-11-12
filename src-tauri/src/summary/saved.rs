//! Saved summaries, so a page never waits on market data it already summarized. A summary is
//! saved with the hash of its inputs (`inputs_key`); a request with the same inputs gets it
//! back without a network call, across launches. When the inputs have changed, the older
//! summary is returned whole, marked `refreshing`, and the caller makes the new one in the
//! background: the old one is never shown as current, and never mixed with newer figures.
use std::sync::Mutex;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::Value;

use super::InvestmentSummary;
use crate::database::SavedSummary;
use crate::storage::Storage;

/// What a summary is and how it's made. A summary saved under another version is never shown,
/// not even while a new one is made, since its shape may not be today's: change this with the
/// summary's contract or the rules that write it.
pub const VERSION: u32 = 1;

/// A summary missing a part (prices that didn't load, say) is made again after this long even
/// when its inputs haven't changed.
const PARTIAL_RETRY_MINUTES: i64 = 15;

/// The portfolio's scope; a holding's is its ticker.
const PORTFOLIO: &str = "*";

pub fn scope(ticker: Option<&str>) -> &str {
    ticker.unwrap_or(PORTFOLIO)
}

/// A request's saved summary, read against its inputs.
#[derive(Debug, PartialEq)]
pub enum Lookup {
    /// Made from these inputs: shown as it is.
    Current(Value),
    /// Made from inputs that have since changed: shown marked `refreshing` while a new one is
    /// made.
    Stale(Value),
    /// None saved, or one saved by another version.
    Missing,
}

pub fn lookup(
    saved: Option<SavedSummary>,
    inputs: &str,
    version: u32,
    now: DateTime<Utc>,
) -> Lookup {
    let Some(saved) = saved.filter(|saved| saved.version == version) else {
        return Lookup::Missing;
    };
    let Ok(mut summary) = serde_json::from_str::<Value>(&saved.json) else {
        return Lookup::Missing;
    };
    let current = saved.inputs == inputs
        && (saved.complete || now - saved.saved_at < Duration::minutes(PARTIAL_RETRY_MINUTES));
    summary["refreshing"] = Value::Bool(!current);
    if current {
        Lookup::Current(summary)
    } else {
        Lookup::Stale(summary)
    }
}

fn locked(storage: &Mutex<Storage>) -> Result<std::sync::MutexGuard<'_, Storage>, String> {
    storage
        .lock()
        .map_err(|_| "The local data store is unavailable".to_string())
}

/// What a request was answered with.
#[derive(Debug, PartialEq)]
pub enum Served {
    /// The saved summary, current.
    Saved(Value),
    /// The saved summary, marked `refreshing`: the caller makes the new one in the background.
    Refreshing(Value),
    /// Made now, since none was saved.
    Made(Value),
}

impl Served {
    pub fn into_value(self) -> Value {
        match self {
            Self::Saved(value) | Self::Refreshing(value) | Self::Made(value) => value,
        }
    }
}

/// The summary of `ticker` (or the portfolio) from `start`: the saved one when it was made from
/// `inputs`, an older saved one marked refreshing, or, with none saved, one `make` makes now
/// and this saves. The storage lock is held only to read and to save, never while `make` runs.
pub async fn serve(
    storage: &Mutex<Storage>,
    ticker: Option<&str>,
    start: NaiveDate,
    inputs: &str,
    now: DateTime<Utc>,
    make: impl AsyncFnOnce() -> Result<InvestmentSummary, String>,
) -> Result<Served, String> {
    let saved = locked(storage)?.saved_summary(ticker, start)?;
    match lookup(saved, inputs, VERSION, now) {
        Lookup::Current(summary) => Ok(Served::Saved(summary)),
        Lookup::Stale(summary) => Ok(Served::Refreshing(summary)),
        Lookup::Missing => {
            let summary = make().await?;
            Ok(Served::Made(save(
                storage, ticker, start, inputs, now, &summary,
            )?))
        }
    }
}

/// Makes and saves the summary of `ticker` from `start` unless the one saved is already current
/// for `inputs`. Returns whether it made one.
pub async fn refresh(
    storage: &Mutex<Storage>,
    ticker: Option<&str>,
    start: NaiveDate,
    inputs: &str,
    now: DateTime<Utc>,
    make: impl AsyncFnOnce() -> Result<InvestmentSummary, String>,
) -> Result<bool, String> {
    let saved = locked(storage)?.saved_summary(ticker, start)?;
    if matches!(lookup(saved, inputs, VERSION, now), Lookup::Current(_)) {
        return Ok(false);
    }
    let summary = make().await?;
    save(storage, ticker, start, inputs, now, &summary)?;
    Ok(true)
}

/// Saves `summary` and returns it as the renderer reads it. A failed save only means it is
/// made again next time, so it never fails the request.
fn save(
    storage: &Mutex<Storage>,
    ticker: Option<&str>,
    start: NaiveDate,
    inputs: &str,
    now: DateTime<Utc>,
    summary: &InvestmentSummary,
) -> Result<Value, String> {
    let json = serde_json::to_string(summary).map_err(|error| error.to_string())?;
    let saved = SavedSummary {
        version: VERSION,
        inputs: inputs.to_owned(),
        complete: summary.unavailable.is_empty(),
        saved_at: now,
        json: json.clone(),
    };
    if let Ok(mut storage) = locked(storage) {
        let _ = storage.save_summary(ticker, start, &saved);
    }
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::session::{Period, SessionClock};

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value).unwrap().to_utc()
    }

    /// A fake summary maker that counts its calls, standing in for the market requests.
    struct Maker(AtomicUsize);

    impl Maker {
        async fn make(&self, unavailable: bool) -> Result<InvestmentSummary, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let period = Period {
                start: day("2026-10-09"),
                end: day("2026-10-16"),
                through: day("2026-10-17"),
            };
            let mut summary = InvestmentSummary::new(
                period,
                Some("TEST".into()),
                SessionClock::at(at("2026-10-17T15:00:00Z")),
            );
            if unavailable {
                summary.missing("prices");
            }
            Ok(summary)
        }

        fn calls(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("brief-summaries-{}", uuid::Uuid::new_v4()))
    }

    const START: &str = "2026-10-09";
    const INPUTS: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const CHANGED: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[tokio::test]
    async fn the_same_inputs_are_answered_from_the_saved_summary_even_after_a_restart() {
        let directory = directory();
        let maker = Maker(AtomicUsize::new(0));
        let now = at("2026-10-17T22:00:00Z");
        let storage = Mutex::new(Storage::new(directory.clone()).unwrap());
        let served = serve(
            &storage,
            Some("TEST"),
            day(START),
            INPUTS,
            now,
            async || maker.make(false).await,
        )
        .await
        .unwrap();
        assert!(matches!(served, Served::Made(_)));
        assert_eq!(maker.calls(), 1);
        drop(storage);

        // A new launch reads the same database.
        let storage = Mutex::new(Storage::new(directory.clone()).unwrap());
        let later = now + Duration::hours(3);
        let served = serve(
            &storage,
            Some("TEST"),
            day(START),
            INPUTS,
            later,
            async || maker.make(false).await,
        )
        .await
        .unwrap();
        let Served::Saved(summary) = served else {
            panic!("expected the saved summary, got {served:?}");
        };
        assert_eq!(summary["refreshing"], Value::Bool(false));
        assert_eq!(summary["ticker"], "TEST");
        assert_eq!(maker.calls(), 1, "no second call for the same inputs");
        // Nor does the background job make one.
        assert!(!refresh(
            &storage,
            Some("TEST"),
            day(START),
            INPUTS,
            later,
            async || { maker.make(false).await }
        )
        .await
        .unwrap());
        assert_eq!(maker.calls(), 1);
        // The portfolio's scope is its own.
        assert!(locked(&storage)
            .unwrap()
            .saved_summary(None, day(START))
            .unwrap()
            .is_none());
        drop(storage);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn changed_inputs_get_the_older_summary_marked_refreshing_and_one_new_one() {
        let directory = directory();
        let maker = Maker(AtomicUsize::new(0));
        let now = at("2026-10-17T22:00:00Z");
        let storage = Mutex::new(Storage::new(directory.clone()).unwrap());
        serve(
            &storage,
            Some("TEST"),
            day(START),
            INPUTS,
            now,
            async || maker.make(false).await,
        )
        .await
        .unwrap();
        // The inputs changed: the page gets the older summary at once, marked, with no call.
        for _ in 0..2 {
            let served = serve(
                &storage,
                Some("TEST"),
                day(START),
                CHANGED,
                now,
                async || maker.make(false).await,
            )
            .await
            .unwrap();
            let Served::Refreshing(summary) = served else {
                panic!("expected the older summary, got {served:?}");
            };
            assert_eq!(summary["refreshing"], Value::Bool(true));
        }
        assert_eq!(maker.calls(), 1);
        // The background job makes exactly one new summary, and then it is current.
        assert!(refresh(
            &storage,
            Some("TEST"),
            day(START),
            CHANGED,
            now,
            async || { maker.make(false).await }
        )
        .await
        .unwrap());
        assert!(!refresh(
            &storage,
            Some("TEST"),
            day(START),
            CHANGED,
            now,
            async || { maker.make(false).await }
        )
        .await
        .unwrap());
        assert_eq!(maker.calls(), 2);
        let served = serve(
            &storage,
            Some("TEST"),
            day(START),
            CHANGED,
            now,
            async || maker.make(false).await,
        )
        .await
        .unwrap();
        assert!(matches!(served, Served::Saved(ref summary) if summary["refreshing"] == false));
        assert_eq!(maker.calls(), 2);
        drop(storage);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn another_version_or_a_partial_summary_is_not_kept_as_current() {
        let directory = directory();
        let maker = Maker(AtomicUsize::new(0));
        let now = at("2026-10-17T22:00:00Z");
        let storage = Mutex::new(Storage::new(directory.clone()).unwrap());
        serve(&storage, None, day(START), INPUTS, now, async || {
            maker.make(false).await
        })
        .await
        .unwrap();
        let saved = locked(&storage)
            .unwrap()
            .saved_summary(None, day(START))
            .unwrap();
        assert!(matches!(
            lookup(saved.clone(), INPUTS, VERSION, now),
            Lookup::Current(_)
        ));
        // A build that makes summaries differently never shows this one, not even as refreshing.
        assert_eq!(lookup(saved, INPUTS, VERSION + 1, now), Lookup::Missing);

        // A summary missing a part stands only a quarter hour, then is made again.
        serve(
            &storage,
            Some("TEST"),
            day(START),
            INPUTS,
            now,
            async || maker.make(true).await,
        )
        .await
        .unwrap();
        let partial = locked(&storage)
            .unwrap()
            .saved_summary(Some("TEST"), day(START))
            .unwrap();
        assert!(!partial.as_ref().unwrap().complete);
        assert!(matches!(
            lookup(partial.clone(), INPUTS, VERSION, now + Duration::minutes(5)),
            Lookup::Current(_)
        ));
        assert!(matches!(
            lookup(partial, INPUTS, VERSION, now + Duration::minutes(20)),
            Lookup::Stale(_)
        ));
        drop(storage);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
