import { Link } from '@tanstack/react-router'
import { Fragment, type ReactNode } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import type { AnalyticsSearch, AnalyticsChart } from '../lib/analytics'
import { formatUpdatedAt } from '../lib/format'
import { RefreshButton } from './data-state'
import { TrendingUp } from './icons'

type WorkspaceParent = {
  label: string
  to: '/' | '/accounts' | '/spending' | '/activities' | '/settings'
}

export type WorkspaceBreadcrumb = {
  label: string
  to: '/accounts' | '/activities' | '/holdings' | '/analytics' | '/settings/design/midday/activity'
  search?: AnalyticsSearch & {
    category?: string
    method?: string
    ticker?: string
    analysis?: AnalyticsChart
  }
}

export function WorkspaceHeader({
  title,
  parent,
  breadcrumbs,
  detail,
  status,
  actions,
  showSnapshot = true,
  showRefresh = true,
}: {
  title: string
  parent?: WorkspaceParent
  breadcrumbs?: WorkspaceBreadcrumb[]
  detail?: string
  status?: ReactNode
  actions?: ReactNode
  showSnapshot?: boolean
  showRefresh?: boolean
}) {
  const data = useFinance().data
  const hasWarning = Boolean(
    data?.recovery ||
    data?.syncWarnings?.length ||
    Object.values(data?.providerStatus ?? {}).some(({ error }) => error),
  )

  return (
    <header className="page-header workspace-header">
      <div className="workspace-title">
        <h1 className={parent || breadcrumbs?.length ? 'page-route' : undefined}>
          {breadcrumbs?.length ? (
            breadcrumbs.map((crumb, index) => (
              <Fragment key={`${index}-${crumb.label}`}>
                {index > 0 ? (
                  <span className="page-route-separator" aria-hidden="true">
                    /
                  </span>
                ) : null}
                {index === breadcrumbs.length - 1 ? (
                  <span aria-current="page">{crumb.label}</span>
                ) : (
                  <Link to={crumb.to} search={crumb.search ?? {}} activeOptions={{ exact: true }}>
                    {crumb.label}
                  </Link>
                )}
              </Fragment>
            ))
          ) : parent ? (
            <>
              <Link to={parent.to} activeOptions={{ exact: true }}>
                {parent.label}
              </Link>
              <span className="page-route-separator" aria-hidden="true">
                /
              </span>
              <span aria-current="page">{title}</span>
            </>
          ) : (
            title
          )}
        </h1>
        {detail ? <p>{detail}</p> : null}
      </div>
      <div className="workspace-actions">
        <div className="workspace-status">
          {status}
          {showSnapshot && data?.updatedAt ? (
            <span className={`freshness snapshot-freshness${hasWarning ? ' warning' : ''}`}>
              {status ? '· ' : null}
              <span>
                Saved <time dateTime={data.updatedAt}>{formatUpdatedAt(data.updatedAt)}</time>
                {hasWarning ? ' · Data needs attention' : ''}
              </span>
            </span>
          ) : null}
        </div>
        {actions}
        {showRefresh ? <RefreshButton /> : null}
      </div>
    </header>
  )
}

export function marketSessionLabel(session: string | undefined, now = new Date()) {
  if (session !== 'Market closed') return session
  const weekday = new Intl.DateTimeFormat('en-US', {
    weekday: 'short',
    timeZone: 'America/New_York',
  }).format(now)
  return weekday === 'Sat' || weekday === 'Sun' ? 'Weekend' : session
}

export function MarketStatus() {
  const query = useLiveFinance()
  const marketIsActive = query.marketPriceState === 'active'
  const marketIsLoading = query.marketPriceState === 'loading'
  const session = marketSessionLabel(query.marketSession)
  return (
    <span
      className="freshness market-freshness"
      data-tone={
        marketIsActive ? 'positive' : query.marketPriceState === 'error' ? 'negative' : 'neutral'
      }
      aria-live="polite"
    >
      <TrendingUp size={13} aria-hidden="true" />
      {marketIsActive
        ? `${session} · ${query.marketPriceMessage}`
        : marketIsLoading
          ? query.marketIsSaved
            ? 'Saved prices · updating'
            : 'Checking market prices'
          : query.marketPriceState === 'error'
            ? 'Prices unavailable'
            : (session ?? 'Prices saved')}
    </span>
  )
}
