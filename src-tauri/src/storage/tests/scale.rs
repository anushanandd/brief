//! How long refresh, derive, a past day, and reports take on five synthetic years. Ignored, so
//! `cargo test` stays fast; run `cargo test --release derived_paths_at_scale -- --ignored
//! --nocapture` to print the timings and hold them to their budgets.
use std::time::{Duration, Instant};

use crate::{
    insights::{
        as_of,
        benefits::{self, Catalog},
        calendar::NamedZone,
        reports,
    },
    storage::change::{finish_refresh, project_refresh},
    synthetic::Life,
};

const RUNS: usize = 5;

/// The median of `RUNS` timed calls.
fn median(mut run: impl FnMut()) -> Duration {
    let mut times = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed()
        })
        .collect::<Vec<_>>();
    times.sort();
    times[RUNS / 2]
}

/// Prints a stage's time and, in an optimized build, holds it to a budget.
fn report(stage: &str, time: Duration, budget_ms: u64) {
    println!("{stage:<28} {:>9.1} ms", time.as_secs_f64() * 1000.0);
    assert!(
        cfg!(debug_assertions) || time <= Duration::from_millis(budget_ms),
        "{stage} took longer than {budget_ms} ms"
    );
}

#[test]
#[ignore]
fn derived_paths_at_scale() {
    let life = Life::sized(11, 5 * 365, 11, 4);
    let days = life.refresh_days();
    let (last, earlier) = days.split_last().unwrap();
    let mut state = crate::storage::empty_state().unwrap();
    let cards = Catalog::from_settings(&state.settings);
    // About one refresh a month builds the saved histories.
    for day in earlier.iter().step_by(10) {
        let refresh = life.refresh(*day);
        let snapshot =
            project_refresh(&state.snapshot, &[], &state.account_links, &cards, &refresh).unwrap();
        state = finish_refresh(&state, &refresh, snapshot).unwrap();
    }
    let refresh = life.refresh(*last);
    let mut next = None;
    let time = median(|| {
        let snapshot =
            project_refresh(&state.snapshot, &[], &state.account_links, &cards, &refresh).unwrap();
        next = Some(finish_refresh(&state, &refresh, snapshot).unwrap());
    });
    let state = next.unwrap();
    let snapshot = &state.snapshot;
    println!(
        "{} transactions, {} accounts, {} holdings, {} trades, {} net worth points",
        snapshot.transactions.len(),
        snapshot.accounts.len(),
        snapshot.holdings.len(),
        snapshot.trades.len(),
        snapshot.net_worth_history.len(),
    );
    assert!(snapshot.transactions.len() > 9_000);
    report("refresh projection", time, 1000);

    let now = chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at)
        .unwrap()
        .to_utc();
    let zone = NamedZone::Utc;
    let derive = || {
        crate::storage::derive_view(&state, None, &Default::default(), now, &zone)
            .unwrap()
            .0
    };
    report("derive_view", median(|| drop(derive())), 1000);
    let view = derive();
    let today = view.insights.as_ref().unwrap().today;

    let past = today - chrono::Duration::days(180);
    report(
        "period_as_of (180 days back)",
        median(|| assert!(as_of::period_as_of(&view, &state.settings, past).is_some())),
        100,
    );
    use reports::Range;
    for range in [
        Range::Day,
        Range::Week,
        Range::Month,
        Range::Quarter,
        Range::Year,
        Range::All,
    ] {
        let request = reports::ReportRequest {
            range: Some(range),
            ..Default::default()
        };
        report(
            &format!("reports {range:?}"),
            median(|| assert!(reports::reports(&view, &request, today, &zone).is_some())),
            100,
        );
    }

    let mut settings = state.settings.clone();
    settings.card_rules = vec![benefits::example_rules(); 10];
    report(
        "card catalog, 10 products",
        median(|| assert_eq!(Catalog::from_settings(&settings).products().len(), 10)),
        50,
    );
}
