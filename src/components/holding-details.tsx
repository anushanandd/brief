import { Link } from '@tanstack/react-router'

import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import { formatCurrency, formatShares, formatPercent, valueTone } from '../lib/format'
import { holdingDetail } from '../lib/holding-detail'
import type { FinanceSnapshot } from '../lib/schema'
import { AccountMark } from './account-mark'
import { ActivityList } from './activity-list'
import { ChevronRight } from './icons'
import { Card, EmptyState, Metric, ScrollCueCard, SectionHeading } from './ui'

export function HoldingPortfolio({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const detail = holdingDetail(data.holdings, ticker)
  return (
    <Card className="account-overview-summary-card">
      <SectionHeading title="Overview" />
      <div className="account-summary-metrics">
        <Metric
          label="% of known investments"
          value={detail.weight == null ? '—' : `${detail.weight.toFixed(1)}%`}
        />
        <Metric
          label="Total change"
          value={formatPercent(detail.totalChange)}
          tone={valueTone(detail.totalChange)}
        />
        <Metric label="Market value" value={formatCurrency(detail.value)} />
        <Metric label="Shares" value={formatShares(detail.shares)} />
        <Metric label="Cost basis" value={formatCurrency(detail.basis)} />
        <Metric label="Average basis" value={formatCurrency(detail.averageBasis)} />
      </div>
    </Card>
  )
}

export function HoldingAccounts({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const detail = holdingDetail(data.holdings, ticker)
  const names = getAccountDisplayNames()
  const accountIds = [...new Set(detail.positions.map(({ accountId }) => accountId))]
  return (
    <Card className="holding-detail-card">
      <SectionHeading title="Accounts" />
      {accountIds.length ? (
        accountIds.map((id) => {
          const account = data.accounts.find((candidate) => candidate.id === id)
          const position = holdingDetail(
            detail.positions.filter(({ accountId }) => accountId === id),
            ticker,
          )
          return (
            <Link key={id} className="account-row" to="/accounts" search={{ account: id }}>
              <AccountMark type={account?.type ?? 'brokerage'} />
              <span className="account-row-name">
                <strong>{accountDisplayName(id, account?.name ?? 'Account', names)}</strong>
                <small>{account?.institution}</small>
              </span>
              <span className="account-row-values">
                <strong>{formatCurrency(position.value)}</strong>
                <small>{formatShares(position.shares)} shares</small>
              </span>
              <ChevronRight size={15} aria-hidden="true" />
            </Link>
          )
        })
      ) : (
        <EmptyState>No positions available.</EmptyState>
      )}
    </Card>
  )
}

export function HoldingActivity({ data, ticker }: { data: FinanceSnapshot; ticker: string }) {
  const activityScrollRef = useViewportScroll(48)
  const activities = buildActivities(
    {
      transactions: [],
      trades: data.trades.filter((trade) => trade.ticker === ticker),
      updatedAt: data.updatedAt,
    },
    getAccountDisplayNames(),
  )
  return (
    <ScrollCueCard className="holding-detail-card" scrollSelector=".holding-activity-scroll">
      <SectionHeading title="Activity" />
      <div
        ref={activityScrollRef}
        className="holding-activity-scroll"
        role="region"
        aria-label={`${ticker} activity`}
        tabIndex={0}
      >
        <ActivityList
          activities={activities}
          referenceIso={data.updatedAt}
          emptyMessage={`No imported activity linked to ${ticker || 'this holding'}.`}
        />
      </div>
    </ScrollCueCard>
  )
}
