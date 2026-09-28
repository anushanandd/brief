import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'

import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { AlertCircle, Info, ShieldCheck } from '../components/icons'
import { Card, EmptyState, SectionHeading, StatusDot } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { getDataHealth } from '../lib/api'
import type { HealthReport } from '../lib/schema'

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
      <Link
        className="button-base button-secondary button-compact"
        to="/settings"
        hash={route.split('#')[1]}
      >
        {issue.action.label}
      </Link>
    )
  if (route.startsWith('/activities'))
    return (
      <Link
        className="button-base button-secondary button-compact"
        to="/activities"
        search={{ category: 'Other' }}
      >
        {issue.action.label}
      </Link>
    )
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

  return (
    <div className="page health-page">
      <WorkspaceHeader title="Data health" actions={<RefreshButton />} />
      <Card className={`health-summary health-${report.overallStatus}`}>
        <div className="health-summary-mark" aria-hidden="true">
          <StatusIcon size={24} />
        </div>
        <div>
          <span>Committed data</span>
          <strong>
            {report.overallStatus === 'healthy'
              ? 'All checks passed'
              : `${report.issues.length} ${report.issues.length === 1 ? 'issue' : 'issues'} found`}
          </strong>
          <small>Revision {report.revision} · checked locally</small>
        </div>
        <div className="health-counts" aria-label="Issue counts">
          {(['critical', 'error', 'warning', 'info'] as const).map((severity) => (
            <span key={severity}>
              <StatusDot
                tone={
                  severity === 'info' ? 'neutral' : severity === 'warning' ? 'warning' : 'negative'
                }
              />
              {report.counts[severity]} {severity}
            </span>
          ))}
        </div>
      </Card>

      {!sections.length ? (
        <Card>
          <EmptyState title="No data issues found">
            Brief checked committed values, freshness, account links, classifications, and return
            coverage.
          </EmptyState>
        </Card>
      ) : null}

      {sections.map(({ severity, issues }) => (
        <section className="health-section" key={severity}>
          <h2>{labels[severity]}</h2>
          <div className="health-issue-grid">
            {issues.map((issue) => {
              const Icon = severity === 'info' ? Info : AlertCircle
              return (
                <Card className="health-issue" key={issue.id}>
                  <SectionHeading
                    title={issue.title}
                    action={
                      <span className={`health-severity health-severity-${severity}`}>
                        <Icon size={14} aria-hidden="true" />
                        {labels[severity]}
                      </span>
                    }
                  />
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
                      <summary>{issue.affectedItems.length} affected records</summary>
                      <ul>
                        {issue.affectedItems.map((item) => (
                          <li key={item.id}>{item.name}</li>
                        ))}
                      </ul>
                    </details>
                  ) : null}
                  <div className="health-action">
                    <IssueLink issue={issue} />
                  </div>
                </Card>
              )
            })}
          </div>
        </section>
      ))}
    </div>
  )
}
