//! Investment summaries made off the UI path: a page's saved summary brought up to date after
//! it was shown marked `refreshing`, and the warm-up that makes the portfolio's and then each
//! holding's summary, largest first, before a page asks. One job at a time, so market requests
//! never pile up; a job already queued isn't queued twice; a summary whose saved copy is
//! current is skipped; and nothing runs while the window is hidden, though what was queued
//! waits for its return. Each summary made is
//! announced with `investment-summary-updated`, and the renderer asks again.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration as StdDuration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    providers,
    session::{Period, SessionClock},
    summary, AppState,
};

/// Between warm-up jobs, leaving Alpaca's per-minute limit to the market stream and pages.
const WARM_PAUSE: StdDuration = StdDuration::from_secs(2);

/// A summary to bring up to date: its period start as the page asks for it, and its holding.
#[derive(Clone, Debug, PartialEq)]
struct Job {
    start: String,
    ticker: Option<String>,
    /// Queued by the warm-up, so a newer warm-up replaces it.
    warm: bool,
}

#[derive(Debug, Default)]
struct Queue {
    jobs: VecDeque<Job>,
    running: bool,
}

impl Queue {
    const fn new() -> Self {
        Self {
            jobs: VecDeque::new(),
            running: false,
        }
    }

    /// A page's job goes first, once, even when the warm-up had queued it.
    fn page(&mut self, start: String, ticker: Option<String>) {
        self.jobs
            .retain(|job| !(job.start == start && job.ticker == ticker));
        self.jobs.push_front(Job {
            start,
            ticker,
            warm: false,
        });
    }

    /// Replaces the warm-up's queued jobs with these, after the pages' own; one a page already
    /// queued isn't queued again.
    fn warm(&mut self, start: &str, tickers: Vec<Option<String>>) {
        self.jobs.retain(|job| !job.warm);
        for ticker in tickers {
            if !self
                .jobs
                .iter()
                .any(|job| job.start == start && job.ticker == ticker)
            {
                self.jobs.push_back(Job {
                    start: start.to_owned(),
                    ticker,
                    warm: true,
                });
            }
        }
    }

    /// Whether the caller should start the runner: only when none is running.
    fn claim(&mut self) -> bool {
        !std::mem::replace(&mut self.running, true)
    }

    /// The next job, or none, which ends the run.
    fn next(&mut self) -> Option<Job> {
        let job = self.jobs.pop_front();
        self.running = job.is_some();
        job
    }

    /// Ends the run and keeps its jobs for `resume`, as a hidden window does.
    fn pause(&mut self) {
        self.running = false;
    }
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());

fn queue() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Brings a page's saved summary up to date in the background.
pub fn refresh(app: &AppHandle, start: String, ticker: Option<String>) {
    queue().page(start, ticker);
    run_if_idle(app);
}

/// Makes the portfolio's summary and each holding's (`tickers`, largest first) for the period
/// from `start`, replacing an earlier warm-up's jobs.
pub fn warm(app: &AppHandle, start: &str, tickers: Vec<Option<String>>) {
    queue().warm(start, tickers);
    run_if_idle(app);
}

/// Picks up jobs a hidden window paused, once it is back.
pub fn resume(app: &AppHandle) {
    if !queue().jobs.is_empty() {
        run_if_idle(app);
    }
}

fn run_if_idle(app: &AppHandle) {
    if queue().claim() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { run(&app).await });
    }
}

/// What the renderer hears when a summary was made: the period start it asked for, and the
/// holding (none for the portfolio).
#[derive(Clone, Serialize)]
struct Updated {
    start: String,
    ticker: Option<String>,
}

async fn run(app: &AppHandle) {
    loop {
        let job = {
            let mut queue = queue();
            if !crate::background::window_visible(app) {
                queue.pause();
                return;
            }
            match queue.next() {
                Some(job) => job,
                None => return,
            }
        };
        let state = app.state::<AppState>();
        let made = match request(&state, &job.start, job.ticker.clone()).await {
            Ok(request) => {
                let (ticker, start, inputs, now) = (
                    request.ticker.clone(),
                    request.period.start,
                    request.inputs.clone(),
                    request.now,
                );
                summary::saved::refresh(
                    &state.storage,
                    ticker.as_deref(),
                    start,
                    &inputs,
                    now,
                    async || request.make(&state).await,
                )
                .await
            }
            Err(error) => Err(error),
        };
        if made.unwrap_or(false) {
            let _ = app.emit(
                "investment-summary-updated",
                Updated {
                    start: job.start.clone(),
                    ticker: job.ticker.clone(),
                },
            );
        }
        if job.warm {
            tokio::time::sleep(WARM_PAUSE).await;
        }
    }
}

/// A summary request read against what is saved: its period, holding, clock, the view and
/// saved sessions it is made from, and the hash of its inputs.
pub(crate) struct Request {
    pub period: Period,
    pub ticker: Option<String>,
    pub now: chrono::DateTime<chrono::Utc>,
    pub inputs: String,
    clock: SessionClock,
    view: std::sync::Arc<crate::insights::View>,
    kept: summary::Kept,
}

/// Reads a request from saved data alone; the storage lock is held only while reading.
pub(crate) async fn request(
    state: &AppState,
    start: &str,
    ticker: Option<String>,
) -> Result<Request, String> {
    let now = chrono::Utc::now();
    let period = Period::through_latest(start, now)?;
    let ticker = providers::one_ticker(ticker)?;
    let clock = SessionClock::at(now);
    let (view, kept) = {
        let storage = state.storage()?;
        (storage.read_view()?, storage.summary_kept(period)?)
    };
    let finances = crate::storage::finances_stamp(&view.snapshot)?;
    let inputs = summary::inputs_key(
        &state.providers,
        &view.snapshot,
        &finances,
        period,
        ticker.as_deref(),
        &clock,
        now,
        &kept,
        summary::saved::VERSION,
    )
    .await?;
    Ok(Request {
        period,
        ticker,
        now,
        inputs,
        clock,
        view,
        kept,
    })
}

impl Request {
    /// Makes the summary over the network, saving the closed sessions it computed.
    pub(crate) async fn make(self, state: &AppState) -> Result<summary::InvestmentSummary, String> {
        let (summary, closed) = summary::investment_summary(
            &state.providers,
            &state.summary_runs,
            &self.view.snapshot,
            self.period,
            self.ticker,
            self.clock,
            self.kept,
        )
        .await?;
        state.save_market_sessions(&closed);
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tickers(queue: &Queue) -> Vec<(Option<&str>, bool)> {
        queue
            .jobs
            .iter()
            .map(|job| (job.ticker.as_deref(), job.warm))
            .collect()
    }

    #[test]
    fn jobs_are_queued_once_pages_first_and_a_new_warm_up_replaces_the_old() {
        let mut queue = Queue::new();
        queue.warm(
            "2026-10-09",
            vec![None, Some("BIG".into()), Some("SMALL".into())],
        );
        // A page's request jumps ahead, and isn't left queued twice.
        queue.page("2026-10-09".into(), Some("SMALL".into()));
        queue.page("2026-10-09".into(), Some("SMALL".into()));
        assert_eq!(
            tickers(&queue),
            [(Some("SMALL"), false), (None, true), (Some("BIG"), true)]
        );
        // A newer warm-up drops the older one's jobs and skips what a page queued.
        queue.warm("2026-10-09", vec![None, Some("SMALL".into())]);
        assert_eq!(tickers(&queue), [(Some("SMALL"), false), (None, true)]);
        // One runner: the first claim starts it, the next doesn't, and an empty queue ends it.
        assert!(queue.claim());
        assert!(!queue.claim());
        assert!(queue.next().is_some() && queue.next().is_some());
        assert!(queue.next().is_none());
        assert!(queue.claim());
        // A hidden window pauses the run; its jobs wait for the window's return.
        queue.warm("2026-10-09", vec![None]);
        queue.pause();
        assert_eq!(tickers(&queue), [(None, true)]);
        assert!(queue.claim());
    }
}
