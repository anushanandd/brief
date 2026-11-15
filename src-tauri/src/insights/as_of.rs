//! The chart periods' figures as they stood at an earlier day's close, for a page scrolled back
//! in time: each chart's change, the combined brokerages' statements, each account's flows, the
//! balance changes, and the income and spending recap. Every history, transaction, and trade
//! after that day is set aside, and the same windows the snapshot carries are measured to it.
use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveTime};
use serde::Serialize;
use ts_rs::TS;

use super::{calendar, chart_change, flows, period_changes, recap, reconcile, rollup, View};
use crate::finance_contract::{Performance, Snapshot};
use crate::settings::FinanceSettings;

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeriodAsOf {
    pub end: NaiveDate,
    pub chart_changes: Vec<chart_change::ChartChanges>,
    pub periods: Vec<reconcile::Statement>,
    pub flows: Vec<flows::AccountFlows>,
    pub changes: Vec<period_changes::PeriodChange>,
    pub recap: Vec<recap::PeriodRecap>,
}

/// The figures at `end`'s close. None without insights, or for a day not before the snapshot's.
pub fn period_as_of(view: &View, settings: &FinanceSettings, end: NaiveDate) -> Option<PeriodAsOf> {
    let insights = view.insights.as_ref()?;
    if end >= insights.today {
        return None;
    }
    let snapshot = through(&view.snapshot, end);
    let series = chart_change::series(
        &view.snapshot,
        settings,
        insights
            .accounts_total
            .as_ref()
            .map_or(&view.snapshot.net_worth_history, |total| &total.history),
        insights.rollup.net_worth,
        &insights.group_history,
    )
    .into_iter()
    .filter_map(|mut series| {
        series.history.retain(|(date, _)| *date <= end);
        series.current = series.history.last()?.1;
        Some(series)
    })
    .collect::<Vec<_>>();
    let noon = end
        .and_time(NaiveTime::from_hms_opt(12, 0, 0)?)
        .and_utc()
        .timestamp();
    let cards = rollup::spending_cards(&snapshot.accounts, settings);
    let card_ids = cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    Some(PeriodAsOf {
        end,
        chart_changes: chart_change::changes(&series, &BTreeMap::new(), noon),
        periods: reconcile::statements(
            &snapshot.brokerage_performance,
            &snapshot.trades,
            &snapshot.transactions,
        ),
        flows: flows::flows(&snapshot, end),
        changes: period_changes::changes(
            &snapshot.account_balance_history,
            &snapshot.brokerage_performance,
            &snapshot.transactions,
            &snapshot.accounts,
            &card_ids,
            end,
        ),
        // From every transaction, since a purchase made by `end` counts even if it posted later.
        recap: recap::recap(&view.snapshot.transactions, end),
    })
}

/// The snapshot as of `end`'s close: later points, transactions, and trades left out, and each
/// history's current value the last one it kept.
fn through(snapshot: &Snapshot, end: NaiveDate) -> Snapshot {
    let on_or_before = |date: &str| calendar::transaction_date(date).is_some_and(|day| day <= end);
    let histories = |records: &[Performance]| {
        records
            .iter()
            .filter_map(|record| {
                let mut record = record.clone();
                record.points.retain(|point| on_or_before(&point.date));
                record.current_value = record.points.last()?.value;
                Some(record)
            })
            .collect::<Vec<_>>()
    };
    let mut past = snapshot.clone();
    past.brokerage_performance = histories(&snapshot.brokerage_performance);
    past.account_balance_history = histories(&snapshot.account_balance_history);
    past.net_worth_history
        .retain(|point| on_or_before(&point.date));
    past.transactions
        .retain(|transaction| on_or_before(transaction.posted_date()));
    past.trades.retain(|trade| on_or_before(&trade.date));
    past.updated_at = format!("{end}T23:59:59Z");
    past
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance_contract::PerformancePoint;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn performance(points: &[(&str, f64, f64)]) -> Performance {
        Performance {
            account_id: "broker".into(),
            name: "Broker".into(),
            institution: "Bank".into(),
            current_value: points.last().unwrap().1,
            history_source: Some("estimated".into()),
            history_start: Some(points[0].0.into()),
            performance_method: Some("time-weighted".into()),
            points: points
                .iter()
                .map(|(date, value, deposits)| PerformancePoint {
                    date: (*date).into(),
                    value: *value,
                    net_deposits: Some(*deposits),
                    ..Default::default()
                })
                .collect(),
        }
    }

    #[test]
    fn a_past_day_keeps_only_what_had_happened_by_its_close() {
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../src/data/empty.json")).unwrap();
        snapshot.brokerage_performance = vec![
            performance(&[
                ("2026-09-01", 100.0, 100.0),
                ("2026-09-02", 110.0, 100.0),
                ("2026-09-03", 160.0, 140.0),
            ]),
            Performance {
                account_id: "later".into(),
                ..performance(&[("2026-09-03", 50.0, 50.0)])
            },
        ];
        let past = through(&snapshot, day("2026-09-02"));
        assert_eq!(past.brokerage_performance.len(), 1);
        let broker = &past.brokerage_performance[0];
        assert_eq!(broker.current_value, 110.0);
        assert_eq!(broker.points.last().unwrap().date, "2026-09-02");
        assert_eq!(past.updated_at, "2026-09-02T23:59:59Z");
    }

    /// A synthetic year of refreshes and its derived view.
    fn life() -> (crate::storage::FinanceState, View) {
        use crate::insights::calendar::NamedZone;
        use crate::storage::change::{finish_refresh, project_refresh};
        let life = crate::synthetic::Life::new(1);
        let mut state = crate::storage::empty_state().unwrap();
        for day in life.refresh_days() {
            let refresh = life.refresh(day);
            let snapshot = project_refresh(
                &state.snapshot,
                &[],
                &state.account_links,
                &crate::insights::benefits::Catalog::from_settings(&state.settings),
                &refresh,
            )
            .unwrap();
            state = finish_refresh(&state, &refresh, snapshot).unwrap();
        }
        let now = chrono::DateTime::parse_from_rfc3339(&state.snapshot.updated_at)
            .unwrap()
            .to_utc();
        let (view, _) =
            crate::storage::derive_view(&state, None, &BTreeMap::new(), now, &NamedZone::Utc)
                .unwrap();
        (state, view)
    }

    #[test]
    fn every_figure_counts_the_same_days_for_a_window() {
        use crate::insights::{calendar::NamedZone, reports, returns};
        let (_, view) = life();
        let insights = view.insights.as_ref().unwrap();
        let today = insights.today;
        for (window, days) in period_changes::WINDOWS {
            let Some(days) = days else { continue };
            let first = today - chrono::Duration::days(days - 1);
            let change = insights
                .changes
                .iter()
                .find(|change| change.window == window);
            assert_eq!(change.unwrap().start, first, "{window:?} balance change");
            let mut flows = insights.flows.iter().filter(|flows| flows.window == window);
            assert!(flows.clone().count() > 0);
            assert!(
                flows.all(|flows| flows.start == Some(first)),
                "{window:?} flows"
            );
            let recap = insights.recap.iter().find(|recap| recap.window == window);
            assert!(
                recap.is_none_or(|recap| recap.start == first),
                "{window:?} recap"
            );
            let request = reports::ReportRequest {
                range: serde_json::from_value(serde_json::to_value(window).unwrap()).unwrap(),
                ..Default::default()
            };
            let report = reports::reports(&view, &request, today, &NamedZone::Utc).unwrap();
            assert_eq!(report.window.start, first, "{window:?} report");
            // A return opens on the close of the day before.
            let name = serde_json::to_value(window).unwrap();
            let returns_window = returns::WINDOWS
                .into_iter()
                .find(|other| serde_json::to_value(other).unwrap() == name);
            let opening = returns::window_start(returns_window.unwrap(), today);
            assert_eq!(
                opening,
                Some(first - chrono::Duration::days(1)),
                "{window:?} return"
            );
        }
    }

    #[test]
    fn a_past_days_recap_counts_purchases_made_by_then_that_posted_later() {
        let (state, mut view) = life();
        let today = view.insights.as_ref().unwrap().today;
        let end = today - chrono::Duration::days(30);
        let month = |view: &View| {
            period_as_of(view, &state.settings, end)
                .unwrap()
                .recap
                .into_iter()
                .find(|recap| recap.window == period_changes::Window::Month)
                .unwrap()
                .spending
        };
        let before = month(&view);
        let mut late = view
            .snapshot
            .transactions
            .iter()
            .find(|transaction| {
                transaction
                    .classification
                    .as_ref()
                    .is_some_and(|c| c.spending)
                    && !transaction.pending
            })
            .unwrap()
            .clone();
        late.id = "late".into();
        late.amount = -60.0;
        late.date = (end - chrono::Duration::days(1)).to_string();
        late.occurred_on = Some(late.date.clone());
        late.posted_on = Some((end + chrono::Duration::days(1)).to_string());
        late.splits = Vec::new();
        view.snapshot.transactions.push(late);
        assert_eq!(crate::numeric::round2(month(&view) - before), 60.0);
    }

    #[test]
    fn a_months_spending_is_one_figure_on_every_page() {
        let (_, view) = life();
        let insights = view.insights.as_ref().unwrap();
        let every = insights
            .spending
            .iter()
            .find(|scope| scope.scope == "all" && scope.owner == super::super::spending::Owner::Me)
            .unwrap();
        assert!(every.months.len() > 1);
        for month in &every.months {
            let calendar = insights
                .activity_months
                .iter()
                .find(|totals| totals.period == month.month)
                .map_or(0.0, |totals| totals.spent);
            assert_eq!(month.total, calendar, "{}", month.month);
        }
    }

    #[test]
    fn homes_month_spends_and_earns_what_the_months_reports_do() {
        use crate::insights::{calendar::NamedZone, reports};
        let (_, view) = life();
        let insights = view.insights.as_ref().unwrap();
        let recap = insights
            .recap
            .iter()
            .find(|recap| recap.window == period_changes::Window::Month)
            .unwrap();
        let read = |chart| {
            let request = reports::ReportRequest {
                chart: Some(chart),
                range: Some(reports::Range::Month),
                ..Default::default()
            };
            reports::reports(&view, &request, insights.today, &NamedZone::Utc).unwrap()
        };
        let income = read(reports::Chart::Income).report.unwrap().summary.total;
        let flow = read(reports::Chart::CashFlow).flow.unwrap();
        let spent = flow
            .purchases
            .iter()
            .map(|item| crate::numeric::minor_units(item.value))
            .sum();
        let start = recap.start.to_string();
        let calendar = insights
            .activity_days
            .iter()
            .filter(|totals| totals.period >= start)
            .fold((0, 0), |(spent, income), totals| {
                (
                    spent + crate::numeric::minor_units(totals.spent),
                    income + crate::numeric::minor_units(totals.income),
                )
            });
        assert!(recap.spending > 0.0 && recap.income > 0.0);
        assert_eq!(
            (recap.spending, Some(recap.income)),
            (crate::numeric::from_minor(spent), income)
        );
        // The calendar dates each purchase by the day it was made.
        assert_eq!(
            (recap.spending, recap.income),
            (
                crate::numeric::from_minor(calendar.0),
                crate::numeric::from_minor(calendar.1)
            )
        );
    }

    #[test]
    fn a_month_back_every_window_ends_on_that_day() {
        let (state, view) = life();
        let settings = &state.settings;
        let today = view.insights.as_ref().unwrap().today;
        assert!(period_as_of(&view, settings, today).is_none());

        let end = today - chrono::Duration::days(30);
        let past = period_as_of(&view, settings, end).unwrap();
        assert!(!past.periods.is_empty());
        assert!(past.periods.iter().all(|statement| statement.end == end));
        assert!(past.changes.iter().all(|change| change.end == end));
        assert!(past.flows.iter().all(|flows| flows.end == end));
        assert!(!past.recap.is_empty());
        // The month's change runs between the net worth saved on the two days.
        let saved = |date: NaiveDate| {
            view.snapshot
                .net_worth_history
                .iter()
                .find(|point| point.date == date.to_string())
                .unwrap()
                .value
        };
        let month = past
            .chart_changes
            .iter()
            .find(|chart| chart.series == chart_change::NET_WORTH)
            .unwrap()
            .windows[&period_changes::Window::Month];
        assert_eq!(
            month.change,
            crate::numeric::round2(saved(end) - saved(end - chrono::Duration::days(30)))
        );
    }
}
