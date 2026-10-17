import { Link, useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { AccountGroups, AccountRows } from '../components/account-groups'
import { ActivityList } from '../components/activity-list'
import { ChartRangeSelector, PerformanceChart } from '../components/charts'
import { PageError, HomeLoading, ValueHistoryEmptyState } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { ChevronRight } from '../components/icons'
import { NeedsList } from '../components/needs-list'
import { PeriodTotals } from '../components/period-totals'
import { PinnedQueries } from '../components/pinned-queries'
import { PositionTable } from '../components/position-table'
import { ShareLine, type ShareSegment } from '../components/share-line'
import { SinceLastRefresh, hasRefreshChanges } from '../components/since-last-refresh'
import { SnapTradeReference } from '../components/snaptrade-reference'
import { AnimatedCurrency, Card, ChartChange, EmptyState, SectionHeading } from '../components/ui'
import { MarketStatus, WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountGroups, groupLabel } from '../lib/account-groups'
import type { AccountDisplayNames } from '../lib/account-names'
import { accountStartDate } from '../lib/account-start-dates'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { buildChartEventGroups } from '../lib/chart-events'
import {
  accountValueIncomplete,
  hasValueHistory,
  dashboardAccountViews,
  withoutSpendingCard,
} from '../lib/dashboard-account-views'
import { financeSettings } from '../lib/finance-settings'
import { countOf } from '../lib/format'
import { chartPeriod, useGraphWindow } from '../lib/graph-preferences'
import {
  accountGroupsModes,
  saveAccountGroupsMode,
  useAccountGroupsMode,
} from '../lib/home-preferences'
import { buildLiveChartData, chartPointsFromStartDate, chartRangeChange } from '../lib/live-chart'
import { getExternalLogosEnabled } from '../lib/logos'
import type { FinanceSnapshot, Rollup } from '../lib/schema'

// Home previews the largest holdings; Investments lists them all.
const holdingsPreviewRows = 8

export function OverviewPage() {
  const live = useLiveFinance()
  // Spending presents the spending card; Home's totals leave it out, like Accounts.
  const query = useMemo(() => {
    // Rust's live total without the cards, whose balances are not market-driven.
    const marketSeries: typeof live.marketSeries = {
      ...live.marketSeries,
      'net-worth': live.marketSeries.accounts ?? [],
    }
    return { ...live, data: live.data && withoutSpendingCard(live.data), marketSeries }
  }, [live])
  const finance = useFinance()
  const navigate = useNavigate()
  const committed = finance.data
  const [graphWindow, setGraphWindow] = useGraphWindow()
  const accountGroupsMode = useAccountGroupsMode()
  const changeWindow = chartPeriod(graphWindow)
  const settings = financeSettings(committed)
  const accountDisplayNames = settings.accountNames
  const accountStartDates = settings.accountStartDates
  const [externalLogosEnabled] = useState(getExternalLogosEnabled)
  useGraphWindowShortcuts(setGraphWindow)
  const accountGroupsActions = useMemo<AppAction[]>(
    () =>
      accountGroupsModes.map(({ value, label }) => ({
        id: `home.accounts.groups.${value}`,
        title: `Switch to ${label.toLowerCase()} account categories`,
        search: 'home accounts categories groups collapsed expanded condensed',
        group: 'View',
        scope: 'page' as const,
        disabled: accountGroupsMode === value,
        disabledReason:
          accountGroupsMode === value ? `Account categories are already ${value}` : undefined,
        run: () => saveAccountGroupsMode(value),
      })),
    [accountGroupsMode],
  )
  useRegisterActions(accountGroupsActions)
  // Home is about everything you have: its hero is net worth, never one brokerage. The
  // brokerages have their own chart on Investments.
  const netWorthView = useMemo(() => {
    if (!query.data) return undefined
    const view = dashboardAccountViews(query.data).find(
      ({ accountId }) => accountId === 'net-worth',
    )
    return view ? { ...view, name: 'Net worth' } : undefined
  }, [query.data])
  const resolvedAccountId = netWorthView?.accountId
  const livePoints = resolvedAccountId ? query.marketSeries[resolvedAccountId] : undefined

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
  if (query.isError || !query.data || !committed || !details || !netWorthView) return <PageError />

  const data = query.data
  const account = netWorthView
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
  const rangeChange = chartRangeChange(query.chartChanges, account.accountId, graphWindow)
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
  const periodChanges = committed.insights?.changes.find(({ window }) => window === changeWindow)
  const accounts = data.accounts
    .filter(({ id }) => id !== 'all')
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const rollup = data.insights?.rollup
  const groups = accountGroups(accounts, rollup?.groups ?? [], periodChanges?.groups)
  const holdings = data.holdings
    .filter(
      ({ accountId }) =>
        account.accountId === 'total' ||
        account.accountId === 'net-worth' ||
        accountId === account.accountId,
    )
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const { latestActivities } = details
  // Recent activity runs as long as the longer of the two previews beside it.
  const recentActivityRows = Math.max(
    Math.min(holdings.length, holdingsPreviewRows),
    accountGroupsMode === 'expanded' ? accounts.length : groups.length,
  )
  const allocation = committed.insights?.allocation
  const holdingShares = allocation ? securityShares(allocation) : undefined
  const accountShares = assetShares(rollup)

  return (
    <div className="page overview-page">
      <WorkspaceHeader title="Home" status={<MarketStatus />} />

      {/* Two rows split by one rule. Above it, the chart sets the height and what's ahead fills
          the column beside it, Coming up scrolling to the rule. Below it, holdings and accounts
          beside what changed and recent activity. */}
      <div className="home-layout">
        <div className="home-top">
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
              <div className="chart-summary-row">
                <ChartChange
                  amount={historyIsAvailable ? rangeChange.change : null}
                  percent={historyIsAvailable ? rangeChange.percent : null}
                />
              </div>
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
                    />
                  ) : (
                    <ValueHistoryEmptyState incomplete={incomplete} valueAvailable={!incomplete} />
                  )}
                </div>
              </div>
            </div>
          </Card>
          <div className="home-companion home-ahead">
            <Card className="home-overview-summary home-ahead-card home-period-card">
              <PeriodTotals data={committed} window={changeWindow} />
            </Card>
            {committed.insights?.brief.needs.length ? (
              <Card className="home-overview-summary home-ahead-card home-needs-card">
                <NeedsList data={committed} />
              </Card>
            ) : null}
            <ExpectedActivity data={data} className="home-expected-card" />
          </div>
        </div>
        <div className="home-bottom">
          <div className="home-previews">
            <Card className="brokerage-holdings-card">
              <PositionTable
                title={
                  <Link
                    to="/holdings"
                    className="section-heading-link"
                    data-command="navigation.open./holdings"
                  >
                    Holdings <ChevronRight size={14} aria-hidden="true" />
                  </Link>
                }
                positions={holdings.slice(0, holdingsPreviewRows)}
                externalLogosEnabled={externalLogosEnabled}
                view="market"
                emptyMessage="No holdings yet."
              />
              {holdings.length > holdingsPreviewRows ? (
                <Link
                  to="/holdings"
                  className="ranked-bars-details-more home-preview-more"
                  data-command="navigation.open./holdings"
                >
                  All {countOf(holdings.length, 'holding')}{' '}
                  <ChevronRight size={14} aria-hidden="true" />
                </Link>
              ) : null}
              {holdingShares ? <ShareLine label="Holdings by ticker" {...holdingShares} /> : null}
            </Card>
            <Card className="accounts-overview-card home-overview-card">
              <SectionHeading
                title={
                  <Link
                    to="/accounts"
                    className="section-heading-link"
                    data-command="navigation.open./accounts"
                  >
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
                {accountGroupsMode === 'expanded' ? (
                  <AccountRows
                    accounts={accounts}
                    changes={periodChanges?.accounts}
                    accountNames={accountDisplayNames}
                  />
                ) : (
                  <AccountGroups
                    groups={groups}
                    changes={periodChanges?.accounts}
                    accountNames={accountDisplayNames}
                  />
                )}
                {!accounts.length ? <EmptyState>No accounts yet.</EmptyState> : null}
              </div>
              <ShareLine label="Assets by type" segments={accountShares} />
            </Card>
          </div>
          <div className="home-companion">
            <SinceLastRefreshCard data={committed} accountNames={accountDisplayNames} />
            <Card className="home-overview-card home-activity-card">
              <SectionHeading
                title={
                  <Link
                    to="/activities"
                    className="section-heading-link"
                    data-command="navigation.open./activities"
                  >
                    Recent activity <ChevronRight size={14} aria-hidden="true" />
                  </Link>
                }
              />
              <ActivityList
                activities={latestActivities.slice(0, recentActivityRows)}
                referenceIso={data.updatedAt}
                onOpen={(activity) =>
                  void navigate({
                    to: '/activities',
                    search: { open: activity.id.replace(/^spending:/, '') },
                  })
                }
              />
            </Card>
          </div>
        </div>
      </div>
      <PinnedQueries />
    </div>
  )
}

/** The card for what the last refresh changed, left out when it changed nothing. */
function SinceLastRefreshCard({
  data,
  accountNames,
}: {
  data: FinanceSnapshot
  accountNames: AccountDisplayNames
}) {
  return hasRefreshChanges(data) ? (
    <Card className="home-overview-summary home-ahead-card">
      <SinceLastRefresh data={data} accountNames={accountNames} />
    </Card>
  ) : null
}

type Allocation = NonNullable<NonNullable<FinanceSnapshot['insights']>['allocation']>

/** Positions by ticker from the native allocation, with uninvested cash closing the line. */
function securityShares(allocation: Allocation) {
  const segments = allocation.securities.map(({ key, value, percent }) => ({
    key,
    label: key,
    value,
    percent,
  }))
  const rest =
    allocation.cash > 0
      ? { key: 'cash', label: 'Cash', value: allocation.cash, percent: allocation.cashPercent }
      : undefined
  return { segments, rest }
}

/** What accounts with a positive balance hold, by type, as Rust shared them out. */
function assetShares(rollup: Rollup | undefined): ShareSegment[] {
  return (rollup?.shares ?? []).map(({ type, value, percent }) => ({
    key: type,
    label: groupLabel(type),
    value,
    percent,
  }))
}
