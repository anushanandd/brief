//! The market close job. Half an hour after each session closes, or on the next launch when
//! Brief wasn't running then, it records that session's layers and events in the finance
//! database, once per session: a session already recorded by the current method is left
//! alone, so a missed close is filled later and a repeated one does nothing.
//!
//! It refreshes the portfolio's stories and filings (Alpaca, EDGAR, and CNBC, each bounded as
//! on any page) and reads daily closes; it never calls Plaid. It runs while the process runs,
//! with the window open or hidden for background refresh; it holds no market socket, so the
//! market stream's hidden-window shutdown is untouched.
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use tauri::{AppHandle, Manager};

use crate::session::{new_york_date, Period, Session, SessionClock};
use crate::summary::{Kept, METHOD};
use crate::AppState;

/// How often the job checks for a session to record.
const CHECK_INTERVAL: StdDuration = StdDuration::from_secs(15 * 60);
/// Before the first check, so launch's own reads and the summary warm-up go first.
const FIRST_CHECK: StdDuration = StdDuration::from_secs(30);
/// A session is recorded this long after its close, once late prints have settled.
const SETTLE_MINUTES: i64 = 30;
/// The period a close records: its last month of sessions, so a few missed closes fill too.
const LOOKBACK_DAYS: i64 = 31;

/// The session the job would record at `now`, and the period that records it.
pub fn due(now: DateTime<Utc>) -> Period {
    let session = Session::latest_settled(now, Duration::minutes(SETTLE_MINUTES));
    Period {
        start: session.date - Duration::days(LOOKBACK_DAYS),
        end: session.date,
        through: new_york_date(now).max(session.date),
    }
}

/// Whether `kept` already holds the session that ends `period`, by the current method.
pub fn recorded(kept: &Kept, period: Period) -> bool {
    kept.sessions
        .iter()
        .any(|record| record.session == period.end && record.method == METHOD)
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            run_once(&app, Utc::now()).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

async fn run_once(app: &AppHandle, now: DateTime<Utc>) {
    let state = app.state::<AppState>();
    let period = due(now);
    let Ok((view, kept, recovering)) = state.storage().and_then(|storage| {
        Ok((
            storage.read_view()?,
            storage.summary_kept(period)?,
            storage.is_recovering(),
        ))
    }) else {
        return;
    };
    let alpaca = state
        .providers
        .integration_status()
        .map(|status| status.alpaca)
        .unwrap_or(false);
    if recovering || !alpaca || recorded(&kept, period) {
        return;
    }
    // Fresh stories and filings first, so the session's news and disclosures are complete.
    let _ = state.providers.refresh_portfolio_news(&view.snapshot).await;
    if let Ok(closed) = state
        .providers
        .close_session(&view.snapshot, period, SessionClock::at(now), kept)
        .await
    {
        state.save_market_sessions(&closed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        crate::session::instant(value).unwrap()
    }

    #[test]
    fn a_close_records_the_settled_session_with_a_month_before_it() {
        // 5 p.m. Tuesday in New York: Tuesday's session has settled.
        let period = due(at("2026-10-06T21:00:00Z"));
        assert_eq!(period.end.to_string(), "2026-10-06");
        assert_eq!(period.start.to_string(), "2026-09-05");
        // A launch on Sunday records Friday's.
        assert_eq!(
            due(at("2026-10-04T15:00:00Z")).end.to_string(),
            "2026-10-02"
        );
        // A session saved by the current method is done; one from an older method isn't.
        let record = |method: u32| -> crate::summary::SessionRecord {
            serde_json::from_value(serde_json::json!({
                "session": "2026-10-06", "method": method, "computedAt": "", "marketPct": 0.0,
                "marketZ": 0.0, "holdings": [], "groups": [],
                "calibration": { "modeled": 0, "fundGroups": 0, "looseGroups": 0, "notable": 0 },
            }))
            .unwrap()
        };
        let kept = |method: u32| Kept {
            sessions: vec![record(method)],
            ..Kept::default()
        };
        assert!(recorded(&kept(METHOD), period));
        assert!(!recorded(&kept(METHOD - 1), period));
        assert!(!recorded(&Kept::default(), period));
    }
}
