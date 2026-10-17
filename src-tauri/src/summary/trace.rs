//! What each summary run did, stage by stage: how long each took, what it found, and which
//! providers failed. Runs are kept in memory for the Logs page and Data health; they hold
//! counts and symbols, never amounts, credentials, or story text.
use std::collections::VecDeque;
use std::time::Instant;

use super::*;
use crate::providers::http::ProviderDiagnostic;

/// Runs kept for the Logs page.
const KEPT_RUNS: usize = 40;

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryStage {
    pub name: &'static str,
    pub millis: u64,
    /// `ok`, `partial` (some of it failed or was left out), `failed`, or `skipped`.
    pub outcome: &'static str,
    /// What the stage found, in counts and symbols.
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SummaryRun {
    pub id: String,
    /// `summary`, `explanation` when the on-device model was asked, or `close` for the close
    /// job.
    pub kind: &'static str,
    /// A ticker, or `portfolio`.
    pub scope: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub started_at: String,
    pub finished_at: String,
    /// `ok`, `partial` when a part was unavailable, or `failed`.
    pub outcome: &'static str,
    pub error: Option<String>,
    pub stages: Vec<SummaryStage>,
    pub providers: Vec<ProviderDiagnostic>,
}

/// Times one run's stages as it goes.
pub(super) struct Trace {
    run: SummaryRun,
    mark: Instant,
}

impl Trace {
    pub fn start(kind: &'static str, period: Period, ticker: Option<&str>) -> Self {
        Self {
            run: SummaryRun {
                id: uuid::Uuid::new_v4().to_string(),
                kind,
                scope: ticker.unwrap_or("portfolio").to_owned(),
                start: period.start,
                end: period.end,
                started_at: Utc::now().to_rfc3339(),
                finished_at: String::new(),
                outcome: "ok",
                error: None,
                stages: Vec::new(),
                providers: Vec::new(),
            },
            mark: Instant::now(),
        }
    }

    /// Records the stage that ran since the last mark.
    pub fn stage(&mut self, name: &'static str, outcome: &'static str, detail: Option<String>) {
        let now = Instant::now();
        self.run.stages.push(SummaryStage {
            name,
            millis: u64::try_from((now - self.mark).as_millis()).unwrap_or(u64::MAX),
            outcome,
            detail,
        });
        self.mark = now;
    }

    pub fn finish(
        mut self,
        error: Option<String>,
        partial: bool,
        providers: Vec<ProviderDiagnostic>,
    ) -> SummaryRun {
        self.run.finished_at = Utc::now().to_rfc3339();
        self.run.outcome = match (&error, partial) {
            (Some(_), _) => "failed",
            (None, true) => "partial",
            (None, false) => "ok",
        };
        self.run.error = error;
        self.run.providers = providers;
        self.run
    }
}

#[cfg(test)]
impl SummaryRun {
    /// A run that ended with `error`, or partly when `partial`.
    pub fn synthetic(period: Period, error: Option<&str>, partial: bool) -> Self {
        let mut trace = Trace::start("summary", period, None);
        trace.stage("history", if partial { "partial" } else { "ok" }, None);
        trace.finish(error.map(str::to_owned), partial, Vec::new())
    }
}

/// The latest runs, newest first.
#[derive(Default)]
pub struct SummaryRuns(std::sync::Mutex<VecDeque<SummaryRun>>);

impl SummaryRuns {
    pub fn record(&self, run: SummaryRun) {
        if let Ok(mut runs) = self.0.lock() {
            runs.push_front(run);
            runs.truncate(KEPT_RUNS);
        }
    }

    pub fn list(&self) -> Vec<SummaryRun> {
        self.0
            .lock()
            .map(|runs| runs.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period() -> Period {
        Period::through_latest("2026-09-28", "2026-10-05T18:00:00Z".parse().unwrap()).unwrap()
    }

    #[test]
    fn a_run_records_its_stages_and_outcome_and_the_log_keeps_the_newest() {
        let mut trace = Trace::start("summary", period(), Some("NVDA"));
        trace.stage("ledger", "ok", Some("3 holdings".into()));
        trace.stage("prices", "partial", Some("ABC left out".into()));
        let run = trace.finish(None, true, Vec::new());
        assert_eq!(run.scope, "NVDA");
        assert_eq!(run.outcome, "partial");
        assert_eq!(
            run.stages
                .iter()
                .map(|stage| stage.name)
                .collect::<Vec<_>>(),
            ["ledger", "prices"]
        );
        let failed = Trace::start("explanation", period(), None).finish(
            Some("Apple Intelligence took too long to summarize".into()),
            false,
            Vec::new(),
        );
        assert_eq!(failed.outcome, "failed");
        assert_eq!(failed.scope, "portfolio");

        let runs = SummaryRuns::default();
        for _ in 0..KEPT_RUNS + 5 {
            runs.record(run.clone());
        }
        runs.record(failed.clone());
        assert_eq!(runs.list().len(), KEPT_RUNS);
        assert_eq!(runs.list()[0], failed);
    }
}
