import { Link } from '@tanstack/react-router'
import type { LivelinePoint } from 'liveline'
import type { ReactNode } from 'react'

import { localDateKey } from '../lib/analytics'
import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import { homeOverviewMetrics, type OverviewRange } from '../lib/home-overview'
import type { FinanceSnapshot } from '../lib/schema'
import { Card, Metric, SectionHeading } from './ui'

export function HomeOverview({
  data,
  spendingAccountId,
  range,
  allAccountsPoints,
  summary,
}: {
  data: FinanceSnapshot
  spendingAccountId?: string
  range: OverviewRange
  allAccountsPoints: LivelinePoint[]
  summary?: ReactNode
}) {
  const metrics = homeOverviewMetrics(data, spendingAccountId, range, allAccountsPoints)
  return (
    <Card className="account-overview-summary-card">
      <SectionHeading title="Overview" />
      <div className="account-summary-metrics">
        <Metric
          label="Portfolio"
          value={formatPercent(metrics.portfolio)}
          tone={valueTone(metrics.portfolio)}
          detail={`vs ${formatPercent(metrics.benchmark)} S&P 500`}
        />
        <Metric
          label="All accounts"
          value={formatPercent(metrics.allAccounts)}
          tone={valueTone(metrics.allAccounts)}
        />
        <Metric label="Spending" value={formatCurrency(metrics.spending)} />
        <Metric
          label="Income"
          value={
            metrics.income == null ? (
              '—'
            ) : (
              <Link
                className="metric-link"
                to="/analytics"
                search={{
                  chart: 'income',
                  from: localDateKey(range.start),
                  to: localDateKey(Math.min(range.end, Date.parse(data.updatedAt) / 1000)),
                }}
              >
                {formatCurrency(metrics.income)}
              </Link>
            )
          }
        />
      </div>
      {summary}
    </Card>
  )
}
