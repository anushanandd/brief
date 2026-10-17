use serde_json::{json, Value};

use super::*;
use crate::insights::calendar::NamedZone;

/// Every Spending period the seed offers in full, keyed `scope|owner|kind|offset` as the
/// preview asks for them.
fn spending_periods(
    seed: &Value,
    settings: &FinanceSettings,
    now: DateTime<Utc>,
) -> serde_json::Map<String, Value> {
    let mut snapshot: Snapshot = serde_json::from_value(seed.clone()).unwrap();
    let insights = derive(&mut snapshot, settings, now, &NamedZone::Utc).unwrap();
    let scopes = insights.spending.clone();
    let view = View {
        snapshot,
        insights: Some(insights),
        account_names: BTreeMap::new(),
        spending_account_id: settings.spending_account_id.clone(),
    };
    let name = |value: Value| value.as_str().unwrap().to_owned();
    let mut periods = serde_json::Map::new();
    for scope in &scopes {
        for current in &scope.periods {
            for offset in 0.. {
                let request = spending::PeriodRequest {
                    scope: scope.scope.clone(),
                    owner: scope.owner,
                    kind: current.kind,
                    offset,
                };
                let Some(detail) = spending_period(&view, &request, &NamedZone::Utc) else {
                    break;
                };
                let key = format!(
                    "{}|{}|{}|{offset}",
                    scope.scope,
                    name(json!(scope.owner)),
                    name(json!(current.kind)),
                );
                let earlier = detail.earlier;
                periods.insert(key, json!(detail));
                if !earlier {
                    break;
                }
            }
        }
    }
    periods
}

/// The browser preview has no native runtime, so it reads the view, Spending periods, and
/// reports made here from the synthetic seed by the native code. Regenerate with
/// `BRIEF_UPDATE_CONTRACT_FIXTURE=1`.
#[test]
fn browser_seed_insights_match_native_insights() {
    let seed: Value = serde_json::from_str(include_str!("../../../src/data/seed.json")).unwrap();
    let spending_account_id = seed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["type"] == "credit")
        .and_then(|account| account["id"].as_str())
        .unwrap()
        .to_owned();
    let settings = FinanceSettings {
        spending_account_id: Some(spending_account_id.clone()),
        ..Default::default()
    };
    // The preview is pinned to the seed's own save time so it never goes stale.
    let now = DateTime::parse_from_rfc3339(seed["updatedAt"].as_str().unwrap())
        .unwrap()
        .to_utc();
    // The view as the native app would send it, so the preview reads the renderer's own type.
    let state = crate::storage::FinanceState {
        snapshot: serde_json::from_value(seed.clone()).unwrap(),
        settings: settings.clone(),
        ..crate::storage::empty_state().unwrap()
    };
    let (read, view) =
        crate::storage::derive_view(&state, None, &BTreeMap::new(), now, &NamedZone::Utc).unwrap();
    // The renderer's `Transaction` type relies on every transaction it reads being classified.
    assert!(read
        .snapshot
        .transactions
        .iter()
        .all(|transaction| transaction.classification.is_some()));
    let fixture = json!({
        "spendingAccountId": spending_account_id,
        "view": view,
        // The investment summary needs prices the preview doesn't have; this one is made
        // from synthetic closes by the same code (`crate::summary::preview`).
        "summary": crate::summary::preview::summary(),
    });
    matches_fixture("seed-view.json", &fixture, false);
    // The seed's object graph, which the preview searches and shows raw.
    let graph = crate::objects::graph(&state, &read);
    matches_fixture(
        "seed-objects.json",
        &json!({ "objects": graph.objects, "links": graph.links, "facts": graph.facts }),
        false,
    );
    // A view of no saved data, which renderer tests start from.
    let empty = crate::storage::empty_state().unwrap();
    let (_, view) =
        crate::storage::derive_view(&empty, None, &BTreeMap::new(), now, &NamedZone::Utc).unwrap();
    matches_fixture("empty-view.json", &view, false);
    // Periods are read on demand, so the preview loads them on its own; the file stays compact
    // and out of the formatter.
    let periods = Value::Object(spending_periods(&seed, &settings, now));
    matches_fixture("seed-spending.json", &periods, false);
    let reports = Value::Object(seed_reports(&seed, &settings, now));
    matches_fixture("seed-reports.json", &reports, false);
}

/// Every report over each chart period, and over each Investments period's exact days, keyed
/// `chart|range|from|to|accounts` as the preview asks for them.
fn seed_reports(
    seed: &Value,
    settings: &FinanceSettings,
    now: DateTime<Utc>,
) -> serde_json::Map<String, Value> {
    use reports::{Chart, Range, ReportRequest};
    let mut snapshot: Snapshot = serde_json::from_value(seed.clone()).unwrap();
    let insights = derive(&mut snapshot, settings, now, &NamedZone::Utc).unwrap();
    let today = insights.today;
    // Investments asks for the days after each period's opening, through today.
    let days = insights
        .periods
        .iter()
        .map(|statement| (statement.start + chrono::Duration::days(1), today))
        .collect::<Vec<_>>();
    let view = View {
        snapshot,
        insights: Some(insights),
        account_names: BTreeMap::new(),
        spending_account_id: settings.spending_account_id.clone(),
    };
    let name = |value: Value| value.as_str().unwrap_or_default().to_owned();
    let mut requests = Vec::new();
    for chart in std::iter::once(None).chain(Chart::ALL.into_iter().map(Some)) {
        for range in Range::ALL {
            requests.push(ReportRequest {
                chart,
                range: Some(range),
                ..Default::default()
            });
        }
    }
    for (from, to) in days {
        requests.push(ReportRequest {
            from: Some(from),
            to: Some(to),
            ..Default::default()
        });
    }
    requests
        .into_iter()
        .map(|request| {
            let key = format!(
                "{}|{}|{}|{}|*",
                name(json!(request.chart)),
                name(json!(request.range)),
                request.from.map(|day| day.to_string()).unwrap_or_default(),
                request.to.map(|day| day.to_string()).unwrap_or_default(),
            );
            let value = json!(reports::reports(&view, &request, today, &NamedZone::Utc));
            (key, value)
        })
        .collect()
}

/// Writes `src/data/{name}` when regenerating, and otherwise checks it still holds `fixture`.
fn matches_fixture(name: &str, fixture: &Value, pretty: bool) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/data")
        .join(name);
    if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
        let text = if pretty {
            serde_json::to_string_pretty(fixture)
        } else {
            serde_json::to_string(fixture)
        };
        std::fs::write(&path, format!("{}\n", text.unwrap())).unwrap();
    } else {
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            &saved, fixture,
            "Regenerate the browser seed insights with BRIEF_UPDATE_CONTRACT_FIXTURE=1"
        );
    }
}
