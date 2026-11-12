//! Background reading of saved text: after stories and filings are saved, the model reads
//! what the summary quotes (each "why it moved" story's cause) and what earnings releases say.
//! One run at a time; each piece is read once and saved in the news cache, so the summary
//! itself never waits on a model.
use std::sync::Mutex;
use std::time::Duration as StdDuration;

use tauri::{AppHandle, Manager};

use crate::AppState;

/// How often saved text is checked for something to read, besides after each refresh.
const CHECK_INTERVAL: StdDuration = StdDuration::from_secs(10 * 60);
/// The wait before the first check, so startup's reads go first.
const FIRST_CHECK: StdDuration = StdDuration::from_secs(45);

/// One run at a time; a request during a run asks for one more pass after it, so text saved
/// mid-run is read without waiting for the next check.
struct Runs(Mutex<Gate>);

#[derive(Default)]
struct Gate {
    running: bool,
    again: bool,
}

impl Runs {
    const fn new() -> Self {
        Self(Mutex::new(Gate {
            running: false,
            again: false,
        }))
    }

    fn gate(&self) -> std::sync::MutexGuard<'_, Gate> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether the caller should run: true unless a run is already going.
    fn request(&self) -> bool {
        let mut gate = self.gate();
        if gate.running {
            gate.again = true;
            false
        } else {
            gate.running = true;
            true
        }
    }

    /// At the end of a pass: whether another was asked for; otherwise the run ends.
    fn again(&self) -> bool {
        let mut gate = self.gate();
        if gate.again {
            gate.again = false;
            true
        } else {
            gate.running = false;
            false
        }
    }

    /// Ends a run that didn't finish, as a panic does.
    fn abandon(&self) {
        self.gate().running = false;
    }
}

static RUNS: Runs = Runs::new();

/// Ends the run if its pass unwinds.
struct Abandon;

impl Drop for Abandon {
    fn drop(&mut self) {
        RUNS.abandon();
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            run(&app).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Reads what was just saved, after the run already reading if there is one.
pub fn nudge(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run(&app).await });
}

async fn run(app: &AppHandle) {
    if !RUNS.request() {
        return;
    }
    let abandon = Abandon;
    loop {
        read(app).await;
        if !RUNS.again() {
            break;
        }
    }
    std::mem::forget(abandon);
}

async fn read(app: &AppHandle) {
    let state = app.state::<AppState>();
    let saved = state.storage().and_then(|storage| {
        Ok((
            storage.data.settings.local_model.clone(),
            storage.read_view()?,
        ))
    });
    if let Ok((local_model, view)) = saved {
        let local = crate::providers::reading::local_model(local_model.as_deref());
        let stocks = crate::providers::portfolio::modeled(
            &crate::providers::portfolio::held_securities(&view.snapshot.holdings),
            None,
        );
        let _ = state.providers.read_causes(local).await;
        let _ = state.providers.read_earnings(&stocks, local).await;
        let _ = state.providers.read_expectations(&stocks, local).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_during_a_run_gets_one_more_pass_and_a_panic_frees_the_next() {
        let runs = Runs::new();
        assert!(runs.request());
        assert!(!runs.request(), "one run at a time");
        assert!(!runs.request());
        assert!(runs.again(), "the requests mid-run ask for one more pass");
        assert!(!runs.again(), "then the run ends");
        assert!(runs.request());
        runs.abandon();
        assert!(runs.request(), "an abandoned run doesn't block the next");
    }
}
