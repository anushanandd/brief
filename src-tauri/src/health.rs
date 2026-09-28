use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::{
    database::SyncRun,
    finance_contract::{Account, Holding, Snapshot},
    transaction_policy::Kind,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub computed_at: String,
    pub revision: u64,
    pub overall_status: String,
    pub counts: HealthCounts,
    pub issues: Vec<HealthIssue>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct HealthCounts {
    pub critical: usize,
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub id: String,
    pub severity: String,
    pub category: String,
    pub title: String,
    pub explanation: String,
    pub evidence: Vec<String>,
    pub affected_items: Vec<HealthItem>,
    pub action: HealthAction,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthAction {
    pub label: String,
    pub route: String,
}

pub fn report(snapshot: &Snapshot, sync_runs: &[SyncRun], now: DateTime<Utc>) -> HealthReport {
    let mut issues = Vec::new();
    if snapshot.recovery.is_some() {
        issues.push(issue(
            "storage-recovery",
            "critical",
            "storage",
            "Local data needs recovery",
            "Brief is showing retained data read-only until recovery is completed.",
            vec![],
            vec![],
            "Open recovery",
            "/",
        ));
    }
    if snapshot.net_worth_incomplete {
        let affected = snapshot
            .accounts
            .iter()
            .filter(|account| account.value.is_none())
            .map(account_item)
            .collect::<Vec<_>>();
        issues.push(issue(
            "incomplete-net-worth",
            "error",
            "valuation",
            "Net worth is incomplete",
            "One or more account balances cannot be valued in USD, so totals exclude unknown values.",
            vec![format!("{} accounts unavailable", affected.len())],
            affected,
            "Review accounts",
            "/accounts",
        ));
    }
    let unpriced = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.quote_eligible == Some(true) && holding.value.is_none())
        .map(holding_item)
        .collect::<Vec<_>>();
    if !unpriced.is_empty() {
        issues.push(issue(
            "missing-holding-prices",
            "error",
            "valuation",
            "Held positions are unpriced",
            "These positions are excluded from complete account valuation until a supported quote is available.",
            vec![format!("{} positions", unpriced.len())],
            unpriced,
            "Review holdings",
            "/holdings",
        ));
    }
    if !snapshot.possible_duplicate_accounts.is_empty() {
        issues.push(issue(
            "possible-duplicate-accounts",
            "error",
            "consistency",
            "Possible duplicate accounts",
            "The same brokerage account may be connected through two providers and counted twice.",
            vec![format!(
                "{} possible matches",
                snapshot.possible_duplicate_accounts.len()
            )],
            vec![],
            "Review account links",
            "/settings",
        ));
    }

    for (provider, status) in snapshot.provider_status.as_ref().into_iter().flatten() {
        if let Some(error) = status.error.as_deref() {
            issues.push(issue(
                format!("provider-error:{provider}"),
                "warning",
                "connection",
                format!("{} is using saved data", provider_label(provider)),
                "The latest provider request failed. The previous committed snapshot remains available.",
                vec![error.to_string()],
                vec![],
                "Review data sources",
                "/settings#data-sources",
            ));
        } else if status
            .updated_at
            .as_deref()
            .and_then(parse_time)
            .is_some_and(|updated| now.signed_duration_since(updated) > Duration::days(3))
        {
            issues.push(issue(
                format!("provider-stale:{provider}"),
                "warning",
                "freshness",
                format!("{} data is stale", provider_label(provider)),
                "The provider has not supplied a recent successful update.",
                status.updated_at.clone().into_iter().collect(),
                vec![],
                "Review data sources",
                "/settings#data-sources",
            ));
        }
    }

    let (critical_quotes, stale_quotes): (Vec<_>, Vec<_>) = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.quote_eligible == Some(true) && holding.value.is_some())
        .filter_map(|holding| {
            let timestamp = holding.market_as_of.as_deref().and_then(parse_time)?;
            let age = now.signed_duration_since(timestamp);
            (age > Duration::days(3)).then_some((holding, age > Duration::days(7)))
        })
        .partition(|(_, critical)| *critical);
    for (severity, values) in [("critical", critical_quotes), ("warning", stale_quotes)] {
        if !values.is_empty() {
            issues.push(issue(
                format!("stale-holding-prices:{severity}"),
                severity,
                "freshness",
                if severity == "critical" {
                    "Held prices are materially stale"
                } else {
                    "Held prices need an update"
                },
                "Saved quotes are old; Brief cannot determine whether their prices remain accurate.",
                vec![format!("{} positions", values.len())],
                values.into_iter().map(|(holding, _)| holding_item(holding)).collect(),
                "Review holdings",
                "/holdings",
            ));
        }
    }

    let limited_performance = snapshot
        .brokerage_performance
        .iter()
        .filter(|account| {
            account.account_id != "total"
                && !matches!(
                    account.performance_method.as_deref(),
                    Some("time-weighted" | "modified-dietz")
                )
        })
        .map(|account| HealthItem {
            id: account.account_id.clone(),
            name: account.name.clone(),
        })
        .collect::<Vec<_>>();
    if !limited_performance.is_empty() {
        issues.push(issue(
            "limited-performance-history",
            "warning",
            "history",
            "Investment returns are limited",
            "Complete dated valuations and external flows are unavailable for these accounts. Brief shows balance history without claiming an investment return.",
            vec![format!("{} accounts", limited_performance.len())],
            limited_performance,
            "Review accounts",
            "/accounts",
        ));
    }

    let missing_basis = snapshot
        .accounts
        .iter()
        .filter(|account| {
            matches!(account.r#type.as_str(), "brokerage" | "retirement")
                && matches!(
                    account.cost_basis_coverage.as_deref(),
                    Some("partial" | "unavailable")
                )
        })
        .map(account_item)
        .collect::<Vec<_>>();
    if !missing_basis.is_empty() {
        issues.push(issue(
            "incomplete-cost-basis",
            "info",
            "classification",
            "Cost basis coverage is incomplete",
            "Current values remain usable, but gain and tax-lot metrics may be partial.",
            vec![format!("{} accounts", missing_basis.len())],
            missing_basis,
            "Review holdings",
            "/holdings",
        ));
    }

    let other_count = snapshot
        .transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction
                    .classification
                    .as_ref()
                    .is_some_and(|classification| classification.kind == Kind::Other)
        })
        .count();
    if other_count > 0 {
        issues.push(issue(
            "unresolved-transaction-classification",
            "info",
            "classification",
            "Some activity remains unclassified",
            "Reviewing these entries improves spending and income analysis without changing provider records.",
            vec![format!("{other_count} posted activities")],
            vec![],
            "Review activity",
            "/activities?category=Other",
        ));
    }

    if let Some(run) = sync_runs.first().filter(|run| run.outcome == "failed") {
        issues.push(issue(
            format!("latest-sync-failed:{}", run.id),
            "warning",
            "connection",
            "Latest refresh did not commit",
            "The previous snapshot was preserved. Diagnostics contains the safe failure record.",
            vec![run.finished_at.clone()],
            vec![],
            "Open diagnostics",
            "/logs",
        ));
    }

    issues.sort_by_key(|item| severity_rank(&item.severity));
    let mut counts = HealthCounts::default();
    for issue in &issues {
        match issue.severity.as_str() {
            "critical" => counts.critical += 1,
            "error" => counts.error += 1,
            "warning" => counts.warning += 1,
            _ => counts.info += 1,
        }
    }
    let overall_status = if counts.critical > 0 {
        "critical"
    } else if counts.error > 0 {
        "error"
    } else if counts.warning > 0 {
        "warning"
    } else if counts.info > 0 {
        "info"
    } else {
        "healthy"
    };
    HealthReport {
        computed_at: now.to_rfc3339(),
        revision: snapshot.revision.unwrap_or_default(),
        overall_status: overall_status.into(),
        counts,
        issues,
    }
}

fn issue(
    id: impl Into<String>,
    severity: &str,
    category: &str,
    title: impl Into<String>,
    explanation: impl Into<String>,
    evidence: Vec<String>,
    affected_items: Vec<HealthItem>,
    label: &str,
    route: &str,
) -> HealthIssue {
    HealthIssue {
        id: id.into(),
        severity: severity.into(),
        category: category.into(),
        title: title.into(),
        explanation: explanation.into(),
        evidence,
        affected_items,
        action: HealthAction {
            label: label.into(),
            route: route.into(),
        },
    }
}

fn account_item(account: &Account) -> HealthItem {
    HealthItem {
        id: account.id.clone(),
        name: account.name.clone(),
    }
}

fn holding_item(holding: &Holding) -> HealthItem {
    HealthItem {
        id: format!("{}:{}", holding.account_id, holding.ticker),
        name: holding.ticker.clone(),
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "plaid" => "Plaid",
        "snaptrade" => "SnapTrade",
        "alpaca" => "Alpaca",
        "alphavantage" => "Alpha Vantage",
        other => other,
    }
}

fn severity_rank(value: &str) -> u8 {
    match value {
        "critical" => 0,
        "error" => 1,
        "warning" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_values_and_duplicates_are_actionable() {
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../src/data/fixtures/native-finance.json"))
                .unwrap();
        snapshot.net_worth_incomplete = true;
        snapshot.accounts[0].value = None;
        snapshot
            .possible_duplicate_accounts
            .push(crate::finance_contract::PossibleDuplicate {
                plaid_account_id: "plaid:one".into(),
                snaptrade_account_id: "snaptrade:one".into(),
                description: "Synthetic duplicate".into(),
            });
        let report = report(
            &snapshot,
            &[],
            DateTime::parse_from_rfc3339("2026-09-11T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.id == "incomplete-net-worth"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.id == "possible-duplicate-accounts"));
        assert_eq!(report.overall_status, "error");
    }
}
