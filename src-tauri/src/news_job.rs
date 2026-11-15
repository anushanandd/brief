//! Keeps the portfolio's stories and filings current while Brief's window is open, so no page
//! ever asks the network. A refresh runs every ten minutes from the pre-market to the end of
//! after-hours on a trading day, and hourly otherwise; each request goes through the provider's
//! pace (`providers::pace`). When a refresh saved something new, Brief says so
//! (`data-updated`), the background reader reads it, and the summaries in view are made again.
//! While the market trades, those summaries are also made again each quarter hour, when the
//! prices behind them move on.
use std::sync::Mutex;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Timelike, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::session::{self, Session};
use crate::AppState;

/// How often the job looks at the clock.
const TICK: StdDuration = StdDuration::from_secs(60);
/// Before the first refresh, so launch paints first.
const FIRST_TICK: StdDuration = StdDuration::from_secs(5);
/// A summary's prices move on in steps this long while the market trades (`summary::inputs_key`).
const STEP_SECONDS: i64 = 15 * 60;

/// What the page hears when Brief saved something new: its kind (`news` or `readings`) and the
/// holdings it concerns, empty when it's the portfolio's.
#[derive(Clone, Debug, Serialize)]
pub struct DataUpdated {
    pub kind: &'static str,
    pub symbols: Vec<String>,
}

/// Tells the page that `kind` changed for `symbols`, so it reads them again.
pub fn announce(app: &AppHandle, kind: &'static str, symbols: Vec<String>) {
    let _ = app.emit("data-updated", DataUpdated { kind, symbols });
}

/// How long a refresh is good for at `now`: ten minutes while there's news to have, on a
/// trading day from the pre-market's start to after-hours' end in New York; an hour otherwise.
pub fn refresh_interval(now: DateTime<Utc>) -> Duration {
    let local = session::new_york(now);
    if session::is_trading_day(local.date()) && (4..20).contains(&local.hour()) {
        Duration::minutes(10)
    } else {
        Duration::hours(1)
    }
}

/// Whether the market trades at `now`, or did until its prices settled half an hour ago.
fn moving(now: DateTime<Utc>) -> bool {
    Session::on(session::new_york_date(now))
        .is_some_and(|day| now >= day.open() && now < day.close() + Duration::minutes(30))
}

#[derive(Default)]
struct Clock {
    refreshed: Option<DateTime<Utc>>,
    step: i64,
}

impl Clock {
    /// Whether a refresh is due at `now`, and whether summaries' prices moved on a step. Only a
    /// refresh that worked sets `refreshed`, so a failed one is tried again next tick.
    fn due(&mut self, now: DateTime<Utc>) -> (bool, bool) {
        let refresh = self
            .refreshed
            .is_none_or(|at| now - at >= refresh_interval(now));
        let step = now.timestamp().div_euclid(STEP_SECONDS);
        let stepped = moving(now) && step != self.step;
        self.step = step;
        (refresh, stepped)
    }
}

static CLOCK: Mutex<Clock> = Mutex::new(Clock {
    refreshed: None,
    step: 0,
});

fn clock() -> std::sync::MutexGuard<'static, Clock> {
    CLOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_TICK).await;
        loop {
            if crate::background::window_visible(&app) {
                tick(&app, Utc::now()).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &AppHandle, now: DateTime<Utc>) {
    let (refresh, step) = clock().due(now);
    let mut changed = false;
    if refresh {
        let state = app.state::<AppState>();
        let view = state.storage().and_then(|storage| storage.read_view());
        if let Ok(view) = view {
            if let Ok(news) = state.providers.refresh_portfolio_news(&view.snapshot).await {
                clock().refreshed = Some(now);
                if !news.changed.is_empty() {
                    announce(app, "news", news.changed);
                    crate::reading_job::nudge(app);
                    changed = true;
                }
            }
            // Saved for six hours, so this asks Alpha Vantage a few times a day at most.
            let held = crate::providers::portfolio::held_securities(&view.snapshot.holdings);
            let symbols = held.into_iter().map(|security| security.ticker).collect();
            let _ = state.providers.earnings_calendar(symbols).await;
        }
    }
    if changed || step {
        crate::summary_jobs::rewarm(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    #[test]
    fn news_refreshes_often_while_the_market_has_news_and_hourly_otherwise() {
        // Friday, Nov 13, 2026: 9:00 in New York is 14:00 UTC; 21:00 is after after-hours.
        assert_eq!(
            refresh_interval(at("2026-11-13T14:00:00Z")),
            Duration::minutes(10)
        );
        assert_eq!(
            refresh_interval(at("2026-11-13T09:30:00Z")),
            Duration::minutes(10)
        );
        assert_eq!(
            refresh_interval(at("2026-11-14T02:00:00Z")),
            Duration::hours(1)
        );
        // Saturday.
        assert_eq!(
            refresh_interval(at("2026-11-14T16:00:00Z")),
            Duration::hours(1)
        );
        // The market moves from the open until half an hour after the close.
        assert!(moving(at("2026-11-13T15:00:00Z")));
        assert!(moving(at("2026-11-13T21:20:00Z")));
        assert!(!moving(at("2026-11-13T21:40:00Z")));
        assert!(!moving(at("2026-11-14T16:00:00Z")));
    }

    #[test]
    fn a_failed_refresh_is_tried_again_next_tick() {
        let mut clock = Clock::default();
        let now = at("2026-11-14T16:00:00Z");
        assert!(clock.due(now).0);
        assert!(clock.due(now + Duration::minutes(1)).0);
        clock.refreshed = Some(now + Duration::minutes(1));
        assert!(!clock.due(now + Duration::minutes(2)).0);
    }
}
