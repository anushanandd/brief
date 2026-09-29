import { Link } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'

import { AccountRow } from '../components/account-row'
import { ActivityList } from '../components/activity-list'
import { ChartRangeSelector, PerformanceChart } from '../components/charts'
import { PageError, HomeLoading, ValueHistoryEmptyState } from '../components/data-state'
import { HomeOverview } from '../components/home-overview'
import { HomeSummary } from '../components/home-summary'
import { ChevronRight } from '../components/icons'
import { PositionTable } from '../components/position-table'
import { SnapTradeReference } from '../components/snaptrade-reference'
import { AnimatedCurrency, Card, ChartChange, EmptyState, SectionHeading } from '../components/ui'
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
} from '../lib/dashboard-account-views'
import { formatPercent, valueTone } from '../lib/format'
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
import { resolveSpendingAccount } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

export function DashboardPage() {
  return <DashboardContent />
}

export function MiddayHomePrototypePage() {
  return <DashboardContent prototype />
}

function DashboardContent({ prototype = false }: { prototype?: boolean }) {
  const query = useLiveFinance()
  const finance = useFinance()
  const committed = finance.data
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
    return {
      latestActivities: buildActivities(committed, accountDisplayNames, externalLogosEnabled),
    }
  }, [committed, accountDisplayNames, externalLogosEnabled])
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
  const referenceAccount = committed.accounts.find(({ id }) => id === account.accountId)

  const points = account.points
  const latestValue = account.currentValue
  const incomplete = accountValueIncomplete(data, account.accountId)
  const startDate = accountStartDate(data, account.accountId, accountStartDates)
  const chartNow = Date.now() / 1_000
  const provisionalNetWorth = account.accountId === 'net-worth' && data.netWorthProvisional
  const chartData = buildLiveChartData(
    points,
    provisionalNetWorth ? [] : (livePoints ?? []),
    provisionalNetWorth ? (points.at(-1)?.value ?? latestValue) : latestValue,
    chartNow,
  )
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
  const { latestActivities } = details

  return (
    <div className="page dashboard-page">
      <WorkspaceHeader
        title="Home"
        status={<MarketStatus />}
        actions={
          prototype ? (
            <Link to="/settings/design" className="midday-prototype-back">
              Design
            </Link>
          ) : undefined
        }
      />

      <div className="home-primary-grid">
        <Card className="net-worth-card brokerage-performance-card">
          <header className="home-balance-header">
            <div className="home-balance-main">
              <h2 className="balance-label">
                {account.name}
                {incomplete ? ' · known USD balances only' : ''}
                {provisionalNetWorth ? ' · provisional balance' : ''}
              </h2>
              <div className="home-balance-value">
                <AnimatedCurrency className="hero-number" value={latestValue} />
              </div>
            </div>
            {referenceAccount?.id.startsWith('snaptrade:') || historyIsAvailable ? (
              <div className="chart-header-aside">
                {historyIsAvailable ? (
                  <ChartRangeSelector value={graphWindow} onValueChange={setGraphWindow} />
                ) : null}
                <SnapTradeReference account={referenceAccount} />
                {referenceAccount?.id.startsWith('snaptrade:') && livePoints?.length ? (
                  <small className="snaptrade-reference">
                    SnapTrade daily values · Alpaca intraday projection
                  </small>
                ) : null}
              </div>
            ) : null}
            {historyIsAvailable ? (
              <div className="chart-summary-row">
                <ChartChange amount={rangeChange.change} percent={rangeChange.percent} />
              </div>
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
                    value={
                      provisionalNetWorth ? (chartData.at(-1)?.value ?? latestValue) : latestValue
                    }
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
            data.netWorthProvisional ? [] : (query.marketSeries['net-worth'] ?? []),
            data.netWorthProvisional
              ? (data.netWorthHistory.at(-1)?.value ?? data.netWorth)
              : data.netWorth,
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
          <div
            className="overview-list"
            role="region"
            aria-label="Accounts"
            tabIndex={0}
            data-keyboard-region
          >
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
          <ActivityList
            activities={latestActivities.slice(0, accounts.length)}
            referenceIso={data.updatedAt}
            compact
          />
        </Card>
      </div>
    </div>
  )
}
