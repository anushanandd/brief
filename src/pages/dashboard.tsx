import { Link, useNavigate } from '@tanstack/react-router'
import { ChevronRight } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { AccountRow } from '../components/account-row'
import { ActivityList } from '../components/activity-list'
import { PerformanceChart, PerformanceChartControls } from '../components/charts'
import { PageError, HomeLoading, ValueHistoryEmptyState } from '../components/data-state'
import { HomeOverview } from '../components/home-overview'
import { HomeSummary } from '../components/home-summary'
import { PositionTable } from '../components/position-table'
import { Treemap } from '../components/treemap'
import { AnimatedCurrency, Card, Change, EmptyState, SectionHeading } from '../components/ui'
import { MarketStatus, WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { accountStartDate, getAccountStartDates } from '../lib/account-start-date-preferences'
import { buildActivities } from '../lib/activity'
import { buildChartEventGroups } from '../lib/chart-events'
import {
  accountWeeklyChangePct,
  accountValueIncomplete,
  hasValueHistory,
  dashboardAccountViews,
  dashboardAssetBreakdown,
} from '../lib/dashboard-account-views'
import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import {
  getChartAccountPreferences,
  getDefaultGraphWindow,
  reconcileChartAccountPreferences,
} from '../lib/graph-preferences'
import {
  buildLiveChartData,
  chartPointsFromStartDate,
  chartRangeChange,
  marketClosePoint,
} from '../lib/live-chart'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildSpendingView,
  resolveSpendingAccount,
  weeklySpendingCategoryPerformance,
} from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

function weeklyPerformanceColor(value: number | null | undefined) {
  if (value == null) return 'var(--surface-hover)'
  if (value === 0) return 'color-mix(in oklch, var(--text-muted) 42%, black)'
  const strength = Math.round(48 + Math.min(Math.abs(value), 10) * 2.4)
  return `color-mix(in oklch, var(--${value > 0 ? 'positive' : 'negative'}) ${strength}%, black)`
}

function assetColor(type: string) {
  if (type === 'brokerage') return 'var(--asset-brokerage)'
  if (type === 'retirement') return 'var(--asset-retirement)'
  if (type === 'cash') return 'var(--asset-cash)'
  if (type === 'credit') return 'var(--asset-credit)'
  if (type === 'loan') return 'var(--asset-loan)'
  return 'var(--asset-other)'
}

export function DashboardPage() {
  return <DashboardContent />
}

function DashboardContent() {
  const navigate = useNavigate()
  const query = useLiveFinance()
  const committed = useFinance().data
  const [activeAccountId, setActiveAccountId] = useState('')
  const [graphWindow, setGraphWindow] = useState(getDefaultGraphWindow)
  const [chartAccountPreferences] = useState(getChartAccountPreferences)
  const [accountDisplayNames] = useState(getAccountDisplayNames)
  const [accountStartDates] = useState(getAccountStartDates)
  const [externalLogosEnabled] = useState(getExternalLogosEnabled)
  const [spendingAccountId] = useState(getSpendingAccountId)
  useGraphWindowShortcuts(setGraphWindow)
  const accountViews = useMemo(() => {
    if (!query.data) return []
    const views = dashboardAccountViews(query.data).map((view) => ({
      ...view,
      name: accountDisplayName(view.accountId, view.name, accountDisplayNames),
    }))
    const byId = new Map(views.map((view) => [view.accountId, view]))
    const visibleViews = reconcileChartAccountPreferences(
      views.map(({ accountId }) => accountId),
      chartAccountPreferences,
    ).flatMap(({ accountId, visible: isVisible }) => (isVisible ? [byId.get(accountId)!] : []))
    return visibleViews.length ? visibleViews : views.slice(0, 1)
  }, [accountDisplayNames, chartAccountPreferences, query.data])
  const accountCount = accountViews.length
  const selectedAccountIndex = accountViews.findIndex((item) => item.accountId === activeAccountId)
  const accountIndex = selectedAccountIndex >= 0 ? selectedAccountIndex : 0
  const resolvedAccountId = accountViews[accountIndex]?.accountId
  const livePoints = resolvedAccountId ? query.marketSeries[resolvedAccountId] : undefined

  useEffect(() => {
    if (resolvedAccountId && resolvedAccountId !== activeAccountId) {
      setActiveAccountId(resolvedAccountId)
    }
  }, [activeAccountId, resolvedAccountId, setActiveAccountId])

  useGraphAccountShortcuts((direction) => {
    if (accountCount < 2) return
    setActiveAccountId((currentId) => {
      const currentIndex = Math.max(
        0,
        accountViews.findIndex((item) => item.accountId === currentId),
      )
      return accountViews[(currentIndex + direction + accountCount) % accountCount].accountId
    })
  })

  const details = useMemo(() => {
    if (!committed) return undefined
    const spendingAccount = resolveSpendingAccount(committed.accounts, spendingAccountId)
    return {
      latestActivities: buildActivities(committed, accountDisplayNames, externalLogosEnabled).slice(
        0,
        10,
      ),
      spending: buildSpendingView(
        spendingAccount
          ? committed.transactions.filter(
              ({ accountId, pending }) => accountId === spendingAccount.id && !pending,
            )
          : [],
        committed.updatedAt,
        1,
      ),
    }
  }, [committed, accountDisplayNames, externalLogosEnabled, spendingAccountId])
  const chartEvents = useMemo(
    () =>
      committed && resolvedAccountId
        ? buildChartEventGroups(committed, resolvedAccountId, graphWindow)
        : [],
    [committed, resolvedAccountId, graphWindow],
  )

  if (query.isLoading || query.startupPending) return <HomeLoading />
  if (query.isError || !query.data || !committed || !details) return <PageError />

  const data = query.data
  const account = accountViews[accountIndex] ?? accountViews[0]

  const points = account.points
  const latestValue = account.currentValue
  const incomplete = accountValueIncomplete(data, account.accountId)
  const startDate = accountStartDate(data, account.accountId, accountStartDates)
  const chartNow = Date.now() / 1_000
  const chartData = buildLiveChartData(points, livePoints ?? [], latestValue, chartNow)
  const rangeChange = chartRangeChange(
    chartPointsFromStartDate(chartData, startDate),
    graphWindow,
    chartNow,
  )
  const closePoint = marketClosePoint(livePoints ?? [], query.marketSession, query.marketCloseTime)
  const netDepositsData =
    account.accountId === 'net-worth' || account.performanceMethod === 'value-only'
      ? undefined
      : buildLiveChartData(
          points.flatMap((point) =>
            typeof point.netDeposits === 'number'
              ? [{ date: point.date, value: point.netDeposits }]
              : [],
          ),
          [],
          points.findLast((point) => typeof point.netDeposits === 'number')?.netDeposits ?? 0,
          chartNow,
        )
  const benchmarkHistory =
    account.accountId === 'net-worth' || account.performanceMethod === 'value-only'
      ? []
      : points.flatMap((point) =>
          typeof point.sp500 === 'number' ? [{ date: point.date, value: point.sp500 }] : [],
        )
  const benchmarkData = benchmarkHistory.length
    ? buildLiveChartData(benchmarkHistory, [], benchmarkHistory.at(-1)?.value ?? 0, chartNow)
    : undefined
  const hasHistoricalValues = hasValueHistory(points, startDate)
  const historyIsAvailable =
    !incomplete &&
    (hasHistoricalValues || chartPointsFromStartDate(livePoints ?? [], startDate).length > 1)
  const accounts = data.accounts
    .filter(({ id }) => id !== 'all')
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const holdings = data.holdings
    .filter(
      ({ accountId }) =>
        account.accountId === 'total' ||
        account.accountId === 'net-worth' ||
        accountId === account.accountId,
    )
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const { latestActivities, spending } = details
  const accountById = new Map(data.accounts.map((item) => [item.id, item]))
  const assetAccounts = dashboardAssetBreakdown(data).map(({ id, name, type, value, children }) => {
    const source = accountById.get(id)
    const changePct = source ? accountWeeklyChangePct(data, source, query.valuationAsOf) : null
    return {
      name: accountDisplayName(id, name, accountDisplayNames),
      value,
      color: assetColor(type),
      changePct,
      href: `/accounts?account=${encodeURIComponent(id)}`,
      onSelect: () => void navigate({ to: '/accounts', search: { account: id } }),
      details: children,
    }
  })
  const spendingPerformance = weeklySpendingCategoryPerformance(
    committed.transactions.filter(({ accountId }) => accountId === spendingAccountId),
    committed.updatedAt,
  )
  const spendingSegments = spending.categories.map(({ name, value }) => ({
    name,
    value,
    color: weeklyPerformanceColor(spendingPerformance.get(name)),
    changePct: spendingPerformance.get(name),
    href: `/activities?account=${encodeURIComponent(spendingAccountId)}&category=${encodeURIComponent(name)}`,
    onSelect: () =>
      void navigate({
        to: '/activities',
        search: { account: spendingAccountId || undefined, category: name },
      }),
  }))
  const holdingValues = new Map<string, { value: number; changePct: number | null }>()
  for (const holding of data.holdings) {
    if (holding.value != null && holding.value > 0) {
      const current = holdingValues.get(holding.ticker)
      holdingValues.set(holding.ticker, {
        value: (current?.value ?? 0) + holding.value,
        changePct: current?.changePct ?? holding.weeklyChangePct,
      })
    }
  }
  const holdingSegments = [...holdingValues].map(([name, { value, changePct }]) => ({
    name,
    value,
    color: weeklyPerformanceColor(changePct),
    changePct,
    href: `/holdings?ticker=${encodeURIComponent(name)}`,
    onSelect: () => void navigate({ to: '/holdings', search: { ticker: name } }),
  }))

  return (
    <div className="page dashboard-page">
      <WorkspaceHeader title="Home" status={<MarketStatus />} />

      <div className="home-primary-grid">
        <Card className="net-worth-card brokerage-performance-card">
          <header className="home-balance-header">
            <div className="home-balance-main">
              <h2 className="balance-label">
                {account.name}
                {incomplete ? ' · known USD balances only' : ''}
              </h2>
              <div className="home-balance-value">
                <AnimatedCurrency className="hero-number" value={latestValue} />
              </div>
              {historyIsAvailable ? (
                <div className="chart-summary-row">
                  <div className="hero-change">
                    <span className={valueTone(rangeChange.change)}>
                      {formatCurrency(rangeChange.change)}
                    </span>
                    <Change value={rangeChange.percent} />
                  </div>
                </div>
              ) : null}
            </div>
            {historyIsAvailable ? (
              <PerformanceChartControls
                value={graphWindow}
                onValueChange={setGraphWindow}
                netDeposits={netDepositsData}
                benchmark={benchmarkData}
              />
            ) : null}
          </header>

          <div className="brokerage-chart-viewport">
            <div key={account.accountId} className="brokerage-chart-slide">
              <div className="brokerage-chart-content">
                {historyIsAvailable ? (
                  <PerformanceChart
                    data={chartData}
                    netDeposits={netDepositsData}
                    benchmark={benchmarkData}
                    value={latestValue}
                    events={chartEvents}
                    referenceIso={data.updatedAt}
                    startDate={startDate}
                    selectedWindow={graphWindow}
                    sessionBoundary={closePoint}
                  />
                ) : (
                  <ValueHistoryEmptyState incomplete={incomplete} valueAvailable={!incomplete} />
                )}
              </div>
            </div>
          </div>
          {accountCount > 1 ? (
            <div className="home-account-dots" role="group" aria-label="Chart account">
              {accountViews.map((view, index) => (
                <button
                  key={view.accountId}
                  aria-label={`Show ${view.name} chart`}
                  aria-pressed={index === accountIndex}
                  className="home-account-dot"
                  onClick={() => setActiveAccountId(view.accountId)}
                  type="button"
                />
              ))}
            </div>
          ) : null}
        </Card>
        <HomeOverview
          summary={
            <HomeSummary
              data={committed}
              rangeSeconds={graphWindow}
              spendingAccountId={resolveSpendingAccount(committed.accounts, spendingAccountId)?.id}
            />
          }
          data={data}
          range={{
            start: graphWindow
              ? chartNow - graphWindow
              : (chartPointsFromStartDate(chartData, startDate)[0]?.time ?? chartNow),
            end: chartNow,
          }}
          allAccountsPoints={buildLiveChartData(
            data.netWorthHistory,
            query.marketSeries['net-worth'] ?? [],
            data.netWorth,
            chartNow,
          )}
          spendingAccountId={resolveSpendingAccount(committed.accounts, spendingAccountId)?.id}
        />
      </div>

      <div className="home-secondary-grid home-holdings-row">
        <Card className="brokerage-holdings-card">
          <PositionTable
            title={
              <Link to="/holdings" className="section-heading-link">
                Holdings <ChevronRight size={14} aria-hidden="true" />
              </Link>
            }
            positions={holdings}
            externalLogosEnabled={externalLogosEnabled}
            view="market"
            emptyMessage="No holdings in this account."
          />
        </Card>

        <Card className="accounts-overview-card home-overview-card">
          <SectionHeading
            title={
              <Link to="/accounts" className="section-heading-link">
                Accounts <ChevronRight size={14} aria-hidden="true" />
              </Link>
            }
          />
          <div className="overview-list">
            {accounts.map((item) => {
              const weeklyChange = accountWeeklyChangePct(data, item, query.valuationAsOf)
              return (
                <AccountRow
                  key={item.id}
                  account={item}
                  displayName={accountDisplayName(item.id, item.name, accountDisplayNames)}
                  valueDetail={
                    <small
                      className={valueTone(weeklyChange)}
                      aria-label={`One-week balance change ${formatPercent(weeklyChange)}`}
                    >
                      {formatPercent(weeklyChange)}
                    </small>
                  }
                />
              )
            })}
            {!accounts.length ? <EmptyState>No accounts yet.</EmptyState> : null}
          </div>
        </Card>

        <Card className="home-overview-card home-activity-card">
          <SectionHeading
            title={
              <Link to="/activities" className="section-heading-link">
                Recent activity <ChevronRight size={14} aria-hidden="true" />
              </Link>
            }
          />
          <ActivityList activities={latestActivities} referenceIso={data.updatedAt} compact />
        </Card>
      </div>

      <div className="home-finance-grid">
        <Card className="home-breakdown-card home-holdings-map">
          {holdingSegments.length ? (
            <Treemap
              segments={holdingSegments}
              label={
                data.holdings.some(({ value }) => value == null)
                  ? 'Holdings by market value · known values only'
                  : 'Holdings by market value'
              }
            />
          ) : (
            <EmptyState>No valued holdings</EmptyState>
          )}
          {data.holdings.some(({ value }) => value == null) ? (
            <p className="spending-detail-note">Known values only</p>
          ) : null}
        </Card>
        <Card className="home-breakdown-card home-assets-card">
          {assetAccounts.length ? (
            <Treemap
              segments={assetAccounts}
              label={
                data.netWorthIncomplete
                  ? 'Assets by account · known USD values only'
                  : 'Assets by account'
              }
            />
          ) : (
            <EmptyState>No valued assets</EmptyState>
          )}
          {data.netWorthIncomplete ? (
            <p className="spending-detail-note">Known USD values only</p>
          ) : null}
        </Card>
        <Card className="home-breakdown-card">
          {spendingSegments.length ? (
            <Treemap segments={spendingSegments} label="This month’s posted spending by category" />
          ) : (
            <EmptyState>No spending activity</EmptyState>
          )}
        </Card>
      </div>
    </div>
  )
}
