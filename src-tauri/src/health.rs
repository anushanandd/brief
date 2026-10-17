use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use ts_rs::TS;

use crate::{
    database::SyncRun,
    finance_contract::{Account, Holding, Snapshot},
    insights::Insights,
    storage::change::{LogCheck, Outcome},
    transaction_policy::Kind,
};

mod ledger_checks;

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HealthReport {
    pub computed_at: String,
    pub revision: u64,
    pub overall_status: String,
    pub counts: HealthCounts,
    pub issues: Vec<HealthIssue>,
}

#[derive(Clone, Debug, Default, Serialize, TS)]
pub struct HealthCounts {
    pub critical: usize,
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

/// Most severe first, as the report lists issues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "HealthSeverity")]
pub enum Severity {
    Critical,
    Error,
    Warning,
    Info,
}

impl Severity {
    fn name(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Classification,
    Connection,
    Consistency,
    Freshness,
    History,
    Storage,
    Valuation,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub id: String,
    pub severity: Severity,
    pub category: Category,
    pub title: String,
    pub explanation: String,
    pub evidence: Vec<String>,
    pub affected_items: Vec<HealthItem>,
    pub action: HealthAction,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct HealthItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct HealthAction {
    pub label: String,
    pub route: String,
}

#[cfg(test)]
pub fn report(snapshot: &Snapshot, sync_runs: &[SyncRun], now: DateTime<Utc>) -> HealthReport {
    report_with(
        snapshot,
        sync_runs,
        None,
        &SummaryHealth::default(),
        None,
        now,
    )
}

/// What Data health knows about the investment summary: whether news is configured, when a
/// story was last saved, this launch's latest summary run, and the saved sessions' rates.
#[derive(Clone, Debug, Default)]
pub struct SummaryHealth {
    /// Alpaca is connected and stocks are held, so news is expected.
    pub news_expected: bool,
    pub newest_story: Option<DateTime<Utc>>,
    pub latest_run: Option<crate::summary::SummaryRun>,
    pub calibration: Option<crate::summary::CalibrationSummary>,
}

/// Rates are judged only after a month of saved sessions.
const CALIBRATION_SESSIONS: usize = 21;
/// The research asked for about one group flag a month per fund; three is a threshold too loose.
const FUND_FLAGS_PER_MONTH: f64 = 3.0;

/// The calibration rates that are out of range, each as a line of evidence.
fn calibration_evidence(calibration: &crate::summary::CalibrationSummary) -> Vec<String> {
    let mut evidence = Vec::new();
    if calibration.sessions >= CALIBRATION_SESSIONS {
        for rate in calibration
            .per_fund
            .iter()
            .filter(|rate| rate.per_month > FUND_FLAGS_PER_MONTH)
        {
            evidence.push(format!(
                "{} moved as a group {} times in {} sessions",
                rate.fund, rate.flags, rate.sessions
            ));
        }
    }
    evidence
}

/// Saved stories older than this leave the summary explaining moves from old news.
const NEWS_STALE_DAYS: i64 = 2;

fn summary_issues(summary: &SummaryHealth, now: DateTime<Utc>) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    if summary.news_expected
        && summary
            .newest_story
            .is_none_or(|saved| now.signed_duration_since(saved) > Duration::days(NEWS_STALE_DAYS))
    {
        issues.push(
            issue(
                "news-stale",
                Severity::Warning,
                Category::Freshness,
                "Portfolio news is out of date",
                "Summaries can only explain moves from saved stories",
            )
            .evidence(vec![summary.newest_story.map_or_else(
                || "No stories saved".to_string(),
                |saved| format!("Last saved {}", saved.to_rfc3339()),
            )])
            .action("Open investments", "/holdings"),
        );
    }
    if let Some(run) = &summary.latest_run {
        let failed_stage = run
            .stages
            .iter()
            .filter(|stage| matches!(stage.outcome, "failed" | "partial"))
            .map(|stage| match &stage.detail {
                Some(detail) => format!("{}: {detail}", stage.name),
                None => stage.name.to_owned(),
            });
        let providers = run.providers.iter().map(|failure| {
            let status = failure
                .http_status
                .map_or_else(|| failure.kind.clone(), |status| status.to_string());
            format!("{} {} ({status})", failure.provider, failure.endpoint)
        });
        let evidence: Vec<String> = run
            .error
            .iter()
            .cloned()
            .chain(failed_stage)
            .chain(providers)
            .take(6)
            .collect();
        match run.outcome {
            "failed" => issues.push(
                issue(
                    format!("summary-failed:{}", run.id),
                    Severity::Warning,
                    Category::Freshness,
                    "The investment summary failed",
                    "Its latest run stopped before finishing; see Diagnostics",
                )
                .evidence(evidence)
                .action("Open diagnostics", "/logs"),
            ),
            "partial" => issues.push(
                issue(
                    format!("summary-partial:{}", run.id),
                    Severity::Info,
                    Category::Freshness,
                    "Part of the investment summary is unavailable",
                    "Its latest run left a part out; see Diagnostics",
                )
                .evidence(evidence)
                .action("Open diagnostics", "/logs"),
            ),
            _ => {}
        }
    }
    if let Some(evidence) = summary
        .calibration
        .as_ref()
        .map(calibration_evidence)
        .filter(|evidence| !evidence.is_empty())
    {
        issues.push(
            issue(
                "summary-calibration",
                Severity::Info,
                Category::Freshness,
                "A summary threshold fires out of range",
                "Saved sessions show a rate the summary's thresholds weren't set for; \
                 see Diagnostics",
            )
            .evidence(evidence)
            .action("Open diagnostics", "/logs"),
        );
    }
    issues
}

/// The report, with checks that read the insights attached to the snapshot and the summary.
pub fn report_with(
    snapshot: &Snapshot,
    sync_runs: &[SyncRun],
    insights: Option<&Insights>,
    summary: &SummaryHealth,
    log: Option<&LogCheck>,
    now: DateTime<Utc>,
) -> HealthReport {
    let mut issues = summary_issues(summary, now);
    issues.extend(log_issues(log));
    issues.extend(recurring_issues(insights));
    issues.extend(valuation_issues(snapshot));
    issues.extend(freshness_issues(snapshot, now));
    issues.extend(coverage_issues(snapshot));
    issues.extend(ledger_checks::ledger_issues(snapshot, now));
    issues.extend(sync_issues(sync_runs));
    issues.sort_by_key(|item| item.severity);
    let mut counts = HealthCounts::default();
    for issue in &issues {
        match issue.severity {
            Severity::Critical => counts.critical += 1,
            Severity::Error => counts.error += 1,
            Severity::Warning => counts.warning += 1,
            Severity::Info => counts.info += 1,
        }
    }
    // The most severe issue sets the status.
    let overall_status = issues
        .first()
        .map_or("healthy", |issue| issue.severity.name())
        .to_owned();
    HealthReport {
        computed_at: now.to_rfc3339(),
        revision: snapshot.revision.unwrap_or_default(),
        overall_status,
        counts,
        issues,
    }
}

/// Plaid recurring streams Brief does not forecast.
fn recurring_issues(insights: Option<&Insights>) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    let unforecast = insights.map_or(&[][..], |insights| &insights.recurring_check.unforecast);
    if !unforecast.is_empty() {
        let affected = unforecast
            .iter()
            .map(|stream| HealthItem {
                id: stream.stream_id.clone(),
                name: stream.name.clone(),
            })
            .collect::<Vec<_>>();
        issues.push(
            issue(
                "recurring-unforecast",
                Severity::Info,
                Category::Classification,
                "Plaid sees unexpected recurring activity",
                "Brief forecasts once history shows the pattern",
            )
            .evidence(vec![count_of(affected.len(), "stream")])
            .affecting(affected)
            .action("Review activity", "/activities"),
        );
    }
    issues
}

/// Recovery, totals that leave values out, and accounts that may be counted twice.
fn valuation_issues(snapshot: &Snapshot) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    if snapshot.recovery.is_some() {
        issues.push(
            issue(
                "storage-recovery",
                Severity::Critical,
                Category::Storage,
                "Local data needs recovery",
                "Read-only until recovered",
            )
            .action("Open recovery", "/"),
        );
    }
    if snapshot.net_worth_incomplete {
        let affected = snapshot
            .accounts
            .iter()
            .filter(|account| account.value.is_none())
            .map(account_item)
            .collect::<Vec<_>>();
        issues.push(
            issue(
                "incomplete-net-worth",
                Severity::Error,
                Category::Valuation,
                "Net worth is incomplete",
                "Totals exclude balances without a USD value",
            )
            .evidence(vec![format!(
                "{} unavailable",
                count_of(affected.len(), "account")
            )])
            .affecting(affected)
            .action("Review accounts", "/accounts"),
        );
    }
    let unpriced = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.quote_eligible == Some(true) && holding.value.is_none())
        .map(holding_item)
        .collect::<Vec<_>>();
    if !unpriced.is_empty() {
        issues.push(
            issue(
                "missing-holding-prices",
                Severity::Error,
                Category::Valuation,
                "Held positions are unpriced",
                "Excluded from totals until a quote is available",
            )
            .evidence(vec![count_of(unpriced.len(), "position")])
            .affecting(unpriced)
            .action("Review holdings", "/holdings"),
        );
    }
    if let Some(check) = snapshot
        .history_check
        .as_ref()
        .filter(|check| check.divergent_days > 0)
    {
        let mut evidence = vec![format!(
            "{} of {} days differ",
            check.divergent_days, check.compared_days
        )];
        if let (Some(gap), Some(date)) = (check.largest_gap, check.largest_gap_date.as_deref()) {
            evidence.push(format!("Largest gap {gap:+.2} on {date}"));
        }
        issues.push(
            issue(
                "history-divergence",
                Severity::Info,
                Category::Consistency,
                "Net worth history is partly estimated",
                "Some estimated days differ from refreshes; refreshes win",
            )
            .evidence(evidence)
            .action("Review accounts", "/accounts"),
        );
    }
    if !snapshot.possible_duplicate_accounts.is_empty() {
        issues.push(
            issue(
                "possible-duplicate-accounts",
                Severity::Error,
                Category::Consistency,
                "Possible duplicate accounts",
                "May be counted twice across providers",
            )
            .evidence(vec![format!(
                "{} possible",
                count_of(snapshot.possible_duplicate_accounts.len(), "match")
            )])
            .action("Review account links", "/settings/accounts"),
        );
    }
    issues
}

/// Providers using saved data, and held prices that have gone stale.
fn freshness_issues(snapshot: &Snapshot, now: DateTime<Utc>) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    for (provider, status) in snapshot.provider_status.as_ref().into_iter().flatten() {
        if let Some(error) = status.error.as_deref() {
            issues.push(
                issue(
                    format!("provider-error:{provider}"),
                    Severity::Warning,
                    Category::Connection,
                    format!("{} is using saved data", provider_label(provider)),
                    "Last request failed; showing saved data",
                )
                .evidence(vec![error.to_string()])
                .action("Review data sources", "/settings/connections"),
            );
        } else if status
            .updated_at
            .as_deref()
            .and_then(parse_time)
            .is_some_and(|updated| now.signed_duration_since(updated) > Duration::days(3))
        {
            issues.push(
                issue(
                    format!("provider-stale:{provider}"),
                    Severity::Warning,
                    Category::Freshness,
                    format!("{} data is stale", provider_label(provider)),
                    "No recent successful update",
                )
                .evidence(status.updated_at.clone().into_iter().collect())
                .action("Review data sources", "/settings/connections"),
            );
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
    for (severity, level, title, values) in [
        (
            Severity::Critical,
            "critical",
            "Held prices are materially stale",
            critical_quotes,
        ),
        (
            Severity::Warning,
            "warning",
            "Held prices need an update",
            stale_quotes,
        ),
    ] {
        if !values.is_empty() {
            issues.push(
                issue(
                    format!("stale-holding-prices:{level}"),
                    severity,
                    Category::Freshness,
                    title,
                    "Quotes may be out of date",
                )
                .evidence(vec![count_of(values.len(), "position")])
                .affecting(
                    values
                        .into_iter()
                        .map(|(holding, _)| holding_item(holding))
                        .collect(),
                )
                .action("Review holdings", "/holdings"),
            );
        }
    }
    issues
}

/// History, cost basis, and classification that leave figures incomplete.
fn coverage_issues(snapshot: &Snapshot) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
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
        issues.push(
            issue(
                "limited-performance-history",
                Severity::Warning,
                Category::History,
                "Investment returns are limited",
                "Missing flow data; showing balance history only",
            )
            .evidence(vec![count_of(limited_performance.len(), "account")])
            .affecting(limited_performance)
            .action("Review accounts", "/accounts"),
        );
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
        issues.push(
            issue(
                "incomplete-cost-basis",
                Severity::Info,
                Category::Classification,
                "Cost basis coverage is incomplete",
                "Gains and tax lots may be partial",
            )
            .evidence(vec![count_of(missing_basis.len(), "account")])
            .affecting(missing_basis)
            .action("Review holdings", "/holdings"),
        );
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
        issues.push(
            issue(
                "unresolved-transaction-classification",
                Severity::Info,
                Category::Classification,
                "Some activity remains unclassified",
                "Categorize to improve spending analysis",
            )
            .evidence(vec![format!(
                "{} posted",
                count_of(other_count, "activity")
            )])
            .action("Review activity", "/activities?category=Other"),
        );
    }
    issues
}

/// The latest refresh, when it failed.
/// The change log should replay to the saved state; saved data is used either way.
fn log_issues(log: Option<&LogCheck>) -> Vec<HealthIssue> {
    let Some(log) = log.filter(|log| log.outcome != Outcome::Matches) else {
        return Vec::new();
    };
    let (severity, title) = match log.outcome {
        Outcome::Differs => (
            Severity::Error,
            "The change log doesn't replay to your saved data",
        ),
        _ => (Severity::Warning, "The change log couldn't be checked"),
    };
    vec![issue(
        "change-log",
        severity,
        Category::Storage,
        title,
        "Saved data is unchanged; see Diagnostics",
    )
    .evidence(log.detail.iter().cloned().collect())
    .action("Open diagnostics", "/logs")]
}

fn sync_issues(sync_runs: &[SyncRun]) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    if let Some(run) = sync_runs.first().filter(|run| run.outcome == "failed") {
        issues.push(
            issue(
                format!("latest-sync-failed:{}", run.id),
                Severity::Warning,
                Category::Connection,
                "Latest refresh did not commit",
                "Previous snapshot kept; see Diagnostics",
            )
            .evidence(vec![run.finished_at.clone()])
            .action("Open diagnostics", "/logs"),
        );
    }
    issues
}

/// An issue with its id, severity, category, title, and explanation; evidence, affected items,
/// and the action that resolves it follow by name.
fn issue(
    id: impl Into<String>,
    severity: Severity,
    category: Category,
    title: impl Into<String>,
    explanation: impl Into<String>,
) -> HealthIssue {
    HealthIssue {
        id: id.into(),
        severity,
        category,
        title: title.into(),
        explanation: explanation.into(),
        evidence: Vec::new(),
        affected_items: Vec::new(),
        action: HealthAction {
            label: String::new(),
            route: String::new(),
        },
    }
}

impl HealthIssue {
    fn evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    fn affecting(mut self, items: Vec<HealthItem>) -> Self {
        self.affected_items = items;
        self
    }

    fn action(mut self, label: &str, route: &str) -> Self {
        self.action = HealthAction {
            label: label.into(),
            route: route.into(),
        };
        self
    }
}

/// "1 account", "2 accounts": a count with the right form of its noun.
fn count_of(count: usize, singular: &str) -> String {
    if count == 1 {
        return format!("1 {singular}");
    }
    let plural = match singular {
        "match" => "matches".to_owned(),
        "activity" => "activities".to_owned(),
        _ => format!("{singular}s"),
    };
    format!("{count} {plural}")
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
    crate::providers::vault::Integration::from_name(provider)
        .map_or(provider, |integration| integration.label())
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

    #[test]
    fn stale_news_and_a_failed_summary_are_reported_with_their_evidence() {
        let snapshot: Snapshot =
            serde_json::from_str(include_str!("../../src/data/fixtures/native-finance.json"))
                .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-10-08T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let period = crate::session::Period::through_latest("2026-10-01", now).unwrap();
        let run = |error: Option<&str>, partial: bool| {
            crate::summary::SummaryRun::synthetic(period, error, partial)
        };
        let ids = |summary: &SummaryHealth| {
            report_with(&snapshot, &[], None, summary, None, now)
                .issues
                .into_iter()
                .map(|issue| issue.id)
                .filter(|id| id.starts_with("news") || id.starts_with("summary"))
                .collect::<Vec<_>>()
        };
        let fresh = SummaryHealth {
            news_expected: true,
            newest_story: Some(now - Duration::hours(3)),
            latest_run: Some(run(None, false)),
            calibration: None,
        };
        assert!(ids(&fresh).is_empty());
        // News isn't expected without Alpaca or stocks, however old.
        let unconfigured = SummaryHealth {
            news_expected: false,
            newest_story: None,
            ..fresh.clone()
        };
        assert!(ids(&unconfigured).is_empty());
        let stale = SummaryHealth {
            newest_story: Some(now - Duration::days(3)),
            latest_run: Some(run(Some("Connect Alpaca in Settings"), false)),
            ..fresh.clone()
        };
        let found = ids(&stale);
        assert_eq!(found[0], "news-stale");
        assert!(found[1].starts_with("summary-failed:"));
        let report = report_with(&snapshot, &[], None, &stale, None, now);
        let failed = report
            .issues
            .iter()
            .find(|issue| issue.id.starts_with("summary-failed"))
            .unwrap();
        assert_eq!(failed.evidence[0], "Connect Alpaca in Settings");
        let partial = SummaryHealth {
            latest_run: Some(run(None, true)),
            ..fresh.clone()
        };
        assert!(ids(&partial)[0].starts_with("summary-partial:"));
        // A month of sessions with a fund flagging weekly.
        let calibration = crate::summary::CalibrationSummary {
            sessions: 42,
            per_fund: vec![crate::summary::FundRate {
                fund: "SMH".into(),
                flags: 9,
                sessions: 42,
                per_month: 4.5,
            }],
            ..Default::default()
        };
        let loose = SummaryHealth {
            calibration: Some(calibration.clone()),
            ..fresh.clone()
        };
        let found = summary_issues(&loose, now);
        assert_eq!(found[0].id, "summary-calibration");
        assert_eq!(
            found[0].evidence,
            ["SMH moved as a group 9 times in 42 sessions"]
        );
        // Too few sessions to judge a fund's rate.
        let early = SummaryHealth {
            calibration: Some(crate::summary::CalibrationSummary {
                sessions: 10,
                ..calibration
            }),
            ..fresh.clone()
        };
        assert!(ids(&early).is_empty());
    }
}
