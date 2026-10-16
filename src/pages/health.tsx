import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'

import { PageError, PageLoading } from '../components/data-state'
import { AlertCircle, ChevronRight, Info, ShieldCheck } from '../components/icons'
import { SettingsLink } from '../components/settings-dialog'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { getDataHealth } from '../lib/api'
import { countOf } from '../lib/format'
import type { HealthReport } from '../lib/schema'
import { settingsHashForPath } from '../lib/settings-search'

const labels = {
  critical: 'Critical',
  error: 'Needs attention',
  warning: 'Limited data',
  info: 'Information',
} as const

function IssueLink({ issue }: { issue: HealthReport['issues'][number] }) {
  const route = issue.action.route
  if (route.startsWith('/settings'))
    return (
      <SettingsLink
        className="button-base button-secondary button-compact"
        target={settingsHashForPath(route)}
      >
        {issue.action.label}
      </SettingsLink>
    )
  if (route.startsWith('/activities')) {
    // The route carries the filter Rust chose, such as `?evidence=conflict`.
    const query = new URLSearchParams(route.split('?')[1] ?? '')
    return (
      <Link
        className="button-base button-secondary button-compact"
        to="/activities"
        search={Object.fromEntries(query)}
      >
        {issue.action.label}
      </Link>
    )
  }
  if (route === '/accounts' || route === '/holdings' || route === '/logs')
    return (
      <Link className="button-base button-secondary button-compact" to={route}>
        {issue.action.label}
      </Link>
    )
  return (
    <Link className="button-base button-secondary button-compact" to="/">
      {issue.action.label}
    </Link>
  )
}

export function HealthPage() {
  const finance = useFinance()
  const health = useQuery({
    queryKey: ['data-health', finance.data?.revision],
    queryFn: getDataHealth,
    enabled: Boolean(finance.data),
  })
  if (finance.isLoading || health.isPending) return <PageLoading />
  if (finance.isError || health.isError || !health.data) return <PageError />

  const report = health.data
  const StatusIcon = report.overallStatus === 'healthy' ? ShieldCheck : AlertCircle
  const sections = (['critical', 'error', 'warning', 'info'] as const).flatMap((severity) => {
    const issues = report.issues.filter((issue) => issue.severity === severity)
    return issues.length ? [{ severity, issues }] : []
  })

  const severityTone = {
    critical: 'negative',
    error: 'negative',
    warning: 'warning',
    info: 'neutral',
  } as const

  return (
    <div className="page health-page">
      <WorkspaceHeader title="Data health" />
      <Card className={`health-summary health-${report.overallStatus}`}>
        <div className="health-summary-status">
          <div className="health-summary-mark" aria-hidden="true">
            <StatusIcon size={22} />
          </div>
          <div>
            <strong>
              {report.overallStatus === 'healthy'
                ? 'All checks passed'
                : `${countOf(report.issues.length, 'issue')} found`}
            </strong>
            <small>Snapshot {report.revision} · checked locally</small>
          </div>
        </div>
        <dl className="health-counts" aria-label="Issue counts">
          {(['critical', 'error', 'warning', 'info'] as const).map((severity) => (
            <div key={severity} data-empty={report.counts[severity] === 0 || undefined}>
              <dt>
                <StatusDot
                  tone={report.counts[severity] === 0 ? 'neutral' : severityTone[severity]}
                />
                {labels[severity]}
              </dt>
              <dd>{report.counts[severity]}</dd>
            </div>
          ))}
        </dl>
      </Card>

      {sections.map(({ severity, issues }) => {
        const Icon = severity === 'info' ? Info : AlertCircle
        return (
          <Card className="health-section" key={severity}>
            <SectionHeading
              title={labels[severity]}
              action={<span className="health-section-count">{issues.length}</span>}
            />
            <div className="health-issue-list">
              {issues.map((issue) => (
                <article className={`health-issue health-issue-${severity}`} key={issue.id}>
                  <span className="health-issue-mark" aria-hidden="true">
                    <Icon size={14} />
                  </span>
                  <div className="health-issue-body">
                    <h3>{issue.title}</h3>
                    <p>{issue.explanation}</p>
                    {issue.evidence.length ? (
                      <ul className="health-evidence" aria-label="Evidence">
                        {issue.evidence.map((evidence) => (
                          <li key={evidence}>{evidence}</li>
                        ))}
                      </ul>
                    ) : null}
                    {issue.affectedItems.length ? (
                      <details className="health-affected">
                        <summary>
                          <ChevronRight size={12} aria-hidden="true" />
                          {countOf(issue.affectedItems.length, 'affected record')}
                        </summary>
                        <ul>
                          {issue.affectedItems.map((item) => (
                            <li key={item.id}>{item.name}</li>
                          ))}
                        </ul>
                      </details>
                    ) : null}
                  </div>
                  <div className="health-action">
                    <IssueLink issue={issue} />
                  </div>
                </article>
              ))}
            </div>
          </Card>
        )
      })}
    </div>
  )
}
