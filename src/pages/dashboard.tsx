import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import {
  ChartNoAxesCombined,
  ChevronRight,
  CreditCard,
  Landmark,
  type LucideIcon,
  WalletCards,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { BrandMark } from '../components/brand-mark'
import { DonutChart, PerformanceChart, type DonutSegment } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, Change, SectionHeading, StatusDot } from '../components/ui'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { graphAccountShortcut, useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import {
  generateFoundationExplanation,
  getFoundationModelStatus,
  getMarketNews,
  isTauri,
  openExternalUrl,
} from '../lib/api'
import { buildChartEventGroups } from '../lib/chart-events'
import {
  currentMonthIncome,
  dashboardAccountViews,
  dashboardAssetBreakdown,
  monthlyPortfolioChange,
} from '../lib/dashboard-account-views'
import {
  formatCompactCurrency,
  formatCurrency,
  formatPercent,
  formatSecurityName,
  formatUpdatedAt,
} from '../lib/format'
import {
  getChartAccountPreferences,
  getDefaultGraphWindow,
  reconcileChartAccountPreferences,
} from '../lib/graph-preferences'
import { briefingEvidence, homeInsightSections, weeklyBriefingCandidate } from '../lib/insights'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { buildSpendingView, resolveSpendingAccount, spendingCategoryColor } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

const assetAccountColors: Record<string, string> = {
  brokerage: 'var(--allocation-2)',
  retirement: 'var(--allocation-3)',
  cash: 'var(--allocation-1)',
}

const accountTypeMarks: Record<string, [LucideIcon, string]> = {
  brokerage: [ChartNoAxesCombined, 'dividend'],
  retirement: [Landmark, 'interest'],
  cash: [WalletCards, 'cash'],
  credit: [CreditCard, 'payment'],
}

function AccountTypeMark({ type }: { type: string }) {
  const [Icon, tone] = accountTypeMarks[type] ?? [Landmark, 'transfer']
  return (
    <span className={`transaction-mark transaction-mark-${tone}`} aria-hidden="true">
      <Icon size={15} />
    </span>
  )
}

const insightEmphasisPattern =
  /(Notable weekly activity|The last seven days|Today|Spending|[Ii]ncome sources|[Ii]ncome|net worth|holding|[-+]?\$[\d,.]+|[+-]?\d+(?:\.\d+)?%|\b[A-Z][A-Z0-9.-]{1,7}\b)/g

function InsightLine({ children }: { children: string }) {
  return (
    <span className="home-insight-line">
      {children
        .split(insightEmphasisPattern)
        .map((part, index) =>
          index % 2 ? <strong key={`${part}:${index}`}>{part}</strong> : part,
        )}
    </span>
  )
}

function HomeInsights({ data }: { data: FinanceSnapshot }) {
  const candidate = useMemo(() => weeklyBriefingCandidate(data), [data])
  const foundationModel = useQuery({
    queryKey: ['foundation-model-status'],
    queryFn: getFoundationModelStatus,
    enabled: isTauri(),
    staleTime: Number.POSITIVE_INFINITY,
  })
  const analysis = useQuery({
    queryKey: ['weekly-briefing', 1, candidate?.key],
    queryFn: async ({ signal }) => {
      if (!candidate) throw new Error('No meaningful changes to explain')
      const news = candidate.stock
        ? await getMarketNews(candidate.stock.ticker).catch(() => undefined)
        : undefined
      signal.throwIfAborted()
      return generateFoundationExplanation(briefingEvidence(candidate, news), signal)
    },
    enabled: isTauri() && !!candidate && foundationModel.data?.state === 'available',
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  })

  const sections = homeInsightSections(data, candidate, analysis.data)

  return (
    <Card className="home-insights" aria-label="What changed">
      <div className="home-insights-heading">
        <strong>What changed</strong>
        <span>Local overview</span>
      </div>
      {sections.map((section) => (
        <section key={section.title} aria-label={section.title}>
          <p className="home-insight-summary" aria-live="polite">
            {section.lines.map((line) => (
              <InsightLine key={line}>{line}</InsightLine>
            ))}
          </p>
        </section>
      ))}
    </Card>
  )
}

export function DashboardPage() {
  return (
    <LiveMarketProvider>
      <DashboardContent />
    </LiveMarketProvider>
  )
}

function DashboardContent() {
  const query = useLiveFinance()
  const [activeAccountId, setActiveAccountId] = useState('total')
  const [graphWindow, setGraphWindow] = useState(getDefaultGraphWindow)
  const [chartAccountPreferences] = useState(getChartAccountPreferences)
  const [accountDisplayNames] = useState(getAccountDisplayNames)
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

  useEffect(() => {
    const runShortcut = (shortcut: string) => {
      if ((shortcut !== 'graph-previous' && shortcut !== 'graph-next') || accountCount < 2) return
      const nextDirection = shortcut === 'graph-previous' ? -1 : 1
      setActiveAccountId((currentId) => {
        const currentIndex = Math.max(
          0,
          accountViews.findIndex((item) => item.accountId === currentId),
        )
        return (
          accountViews[(currentIndex + nextDirection + accountCount) % accountCount]?.accountId ??
          currentId
        )
      })
    }

    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      const shortcut = graphAccountShortcut(event)
      if (
        !shortcut ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }
      event.preventDefault()
      runShortcut(shortcut)
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen<string>('graph-shortcut', (event) => runShortcut(event.payload)).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }

    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [accountCount, accountViews])

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const account = accountViews[accountIndex] ?? accountViews[0]
  const marketIsActive = query.marketPriceState === 'active'
  const marketIsLoading = query.marketPriceState === 'loading'

  const points = account.points
  const latestValue = account.currentValue
  const incomplete =
    account.accountId === 'net-worth'
      ? data.netWorthIncomplete
      : account.accountId === 'total' &&
        data.accounts.some(
          ({ type, value }) => (type === 'brokerage' || type === 'retirement') && value == null,
        )
  const latestPoint = points.at(-1)
  const latestDate = latestPoint?.date ?? ''
  const currentDate = data.updatedAt.slice(0, 10)
  const hasLiveDelta = Math.abs(latestValue - (latestPoint?.value ?? latestValue)) >= 0.01
  const priorValue =
    hasLiveDelta && /^\d{4}-\d{2}-\d{2}$/.test(latestDate) && latestDate < currentDate
      ? (latestPoint?.value ?? latestValue)
      : (points.at(-2)?.value ?? latestPoint?.value ?? latestValue)
  const dailyChange = latestValue - priorValue
  const dailyChangePercent = priorValue ? (dailyChange / Math.abs(priorValue)) * 100 : 0
  const changePeriod = /^\d{4}-\d{2}-\d{2}$/.test(points.at(-1)?.date ?? '')
    ? 'today'
    : 'this period'
  const chartNow = Date.now() / 1_000
  const chartData = buildLiveChartData(points, livePoints ?? [], latestValue, chartNow)
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
  const chartEvents = buildChartEventGroups(data, account.accountId, points, graphWindow)
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
  const latestActivities = buildActivities(data, accountDisplayNames, externalLogosEnabled).slice(
    0,
    6,
  )
  const portfolioChange = monthlyPortfolioChange(data)
  const income = currentMonthIncome(data)
  const spendingAccount = resolveSpendingAccount(data.accounts, spendingAccountId)
  const spending = buildSpendingView(
    spendingAccount
      ? data.transactions.filter(({ accountId }) => accountId === spendingAccount.id)
      : [],
    data.updatedAt,
    1,
  )
  const assetGroups = dashboardAssetBreakdown(data).map((group) => {
    const children = group.children.map((child) => ({
      ...child,
      color: child.name === 'Cash & other' ? 'var(--chart-secondary)' : stockMarkColor(child.name),
    }))
    return {
      ...group,
      name: accountDisplayName(group.id, group.name, accountDisplayNames),
      color: children[0]?.color ?? assetAccountColors[group.type] ?? 'var(--allocation-4)',
      children,
    }
  })
  const assetAccounts: DonutSegment[] = assetGroups.map(({ name, value, color, children }) => ({
    name,
    value,
    color,
    details: children,
  }))
  const totalAssets = assetAccounts.reduce((total, { value }) => total + value, 0)
  const spendingSegments: DonutSegment[] = spending.categories.map(({ name, value }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
  }))
  return (
    <div className="page dashboard-page">
      <header className="page-header home-header">
        <h1>Home</h1>
        <div className="dashboard-actions">
          <span className="freshness" title={query.marketPriceMessage} aria-live="polite">
            <StatusDot tone={marketIsActive ? 'positive' : 'neutral'} />
            {marketIsActive
              ? `${query.marketSession} · ${query.marketPriceMessage}`
              : marketIsLoading
                ? 'Checking market prices'
                : query.marketPriceState === 'error'
                  ? 'Prices unavailable'
                  : query.marketSession === 'Market closed'
                    ? 'Market closed · prices paused'
                    : `Updated ${formatUpdatedAt(data.updatedAt)}`}
          </span>
          <RefreshButton />
        </div>
      </header>

      <div className="home-primary-grid">
        <Card className="net-worth-card brokerage-performance-card">
          <header className="home-balance-header">
            <div>
              <h2 className="balance-label">
                {account.name}
                {incomplete ? ' · known USD balances only' : ''}
              </h2>
              <div className="home-balance-value">
                <span className="hero-number">{formatCurrency(latestValue)}</span>
              </div>
              {!incomplete ? (
                <div className="hero-change">
                  <span
                    className={
                      dailyChange > 0 ? 'positive' : dailyChange < 0 ? 'negative' : 'muted'
                    }
                  >
                    {formatCurrency(dailyChange)} {changePeriod}
                  </span>
                  <Change value={dailyChangePercent} />
                </div>
              ) : (
                <p className="spending-detail-note">
                  A complete balance and change are unavailable.
                </p>
              )}
            </div>
            <span className="balance-institution">
              {account.institution}
              <br />
              {account.accountId === 'net-worth'
                ? data.netWorthHistoryEstimated
                  ? 'Estimated history'
                  : 'Observed history'
                : account.historySource === 'provider-estimated'
                  ? 'Provider-estimated value history'
                  : account.historySource === 'reported'
                    ? 'Observed provider history'
                    : account.historySource === 'estimated'
                      ? 'Estimated history'
                      : 'History unavailable'}
            </span>
          </header>

          <div className="brokerage-chart-viewport">
            <div key={account.accountId} className="brokerage-chart-slide">
              <div className="brokerage-chart-content">
                {!incomplete ? (
                  <PerformanceChart
                    data={chartData}
                    netDeposits={netDepositsData}
                    benchmark={benchmarkData}
                    value={latestValue}
                    events={chartEvents}
                    referenceIso={data.updatedAt}
                    selectedWindow={graphWindow}
                    onWindowChange={setGraphWindow}
                  />
                ) : (
                  <p className="spending-detail-note">
                    The total chart is paused until all included account balances are available in
                    USD.
                  </p>
                )}
              </div>
            </div>
          </div>
        </Card>
        <HomeInsights data={data} />
      </div>

      <div className="home-secondary-grid home-holdings-row">
        <Card className="brokerage-holdings-card">
          <section className="brokerage-holdings" aria-label={`${account.name} holdings`}>
            <div className="holding-column-headings">
              <h2>Holdings</h2>
              <span aria-hidden="true">Price</span>
              <span aria-hidden="true">Today %</span>
              <span aria-hidden="true">Week %</span>
              <span aria-hidden="true">Total %</span>
              <span aria-hidden="true">Value</span>
            </div>
            <div className="overview-list holdings-list">
              {holdings.map((holding) => {
                const yahooUrl = `https://finance.yahoo.com/quote/${encodeURIComponent(holding.ticker)}/`
                return (
                  <a
                    className="overview-holding-row compact-holding-row"
                    href={yahooUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    key={`${holding.accountId}:${holding.ticker}`}
                    onClick={(event) => {
                      if (!isTauri()) return
                      event.preventDefault()
                      void openExternalUrl(yahooUrl)
                    }}
                  >
                    <span className="table-asset">
                      <BrandMark
                        className="asset-mark"
                        fallback={stockMarkLabel(holding.ticker)}
                        label={`${holding.name} logo`}
                        src={stockLogoUrl(holding.ticker, externalLogosEnabled)}
                        style={{ backgroundColor: stockMarkColor(holding.ticker) }}
                      />
                      <span className="overview-holding-copy">
                        <strong className="overview-holding-ticker">{holding.ticker}</strong>
                        <small>{holding.valuationNote ?? formatSecurityName(holding.name)}</small>
                      </span>
                    </span>
                    <span className="holding-metric">
                      <span className="sr-only">Price: </span>
                      <span>{formatCurrency(holding.price)}</span>
                    </span>
                    <span className="holding-metric">
                      <span className="sr-only">Today: </span>
                      <span
                        className={
                          holding.dailyChangePct == null
                            ? 'muted'
                            : holding.dailyChangePct >= 0
                              ? 'positive'
                              : 'negative'
                        }
                      >
                        {formatPercent(holding.dailyChangePct)}
                      </span>
                    </span>
                    <span
                      className="holding-metric"
                      title={
                        holding.weeklyReferencePrice != null && holding.weeklyReferenceDate
                          ? `Since ${holding.weeklyReferenceDate} close ${formatCurrency(holding.weeklyReferencePrice)}`
                          : 'Seven-day reference price unavailable'
                      }
                    >
                      <span className="sr-only">Seven-day change: </span>
                      <span
                        className={
                          holding.weeklyChangePct == null
                            ? 'muted'
                            : holding.weeklyChangePct >= 0
                              ? 'positive'
                              : 'negative'
                        }
                      >
                        {formatPercent(holding.weeklyChangePct)}
                      </span>
                    </span>
                    <span className="holding-metric">
                      <span className="sr-only">Total gain: </span>
                      <span
                        className={
                          holding.totalChangePct == null
                            ? 'muted'
                            : holding.totalChangePct >= 0
                              ? 'positive'
                              : 'negative'
                        }
                      >
                        {formatPercent(holding.totalChangePct)}
                      </span>
                    </span>
                    <span className="holding-metric">
                      <span className="sr-only">Value: </span>
                      <span>{formatCurrency(holding.value)}</span>
                    </span>
                  </a>
                )
              })}
              {!holdings.length ? (
                <p className="overview-recent-empty">No holdings in this account.</p>
              ) : null}
            </div>
          </section>
        </Card>

        <Card className="accounts-overview-card home-overview-card">
          <SectionHeading title="Accounts" />
          <div className="overview-list">
            {accounts.map((item) => (
              <Link
                className="overview-account-row"
                key={item.id}
                to="/accounts/$accountId"
                params={{ accountId: item.id }}
              >
                <AccountTypeMark type={item.type} />
                <span className="account-name">
                  <span>{accountDisplayName(item.id, item.name, accountDisplayNames)}</span>
                  <small>{item.institution}</small>
                </span>
                <span>{formatCurrency(item.value)}</span>
              </Link>
            ))}
            {!accounts.length ? <p className="overview-recent-empty">No accounts yet.</p> : null}
          </div>
        </Card>

        <Card className="home-overview-card home-activity-card">
          <SectionHeading
            title={
              <Link to="/activities" className="section-heading-link">
                Latest activities <ChevronRight size={14} aria-hidden="true" />
              </Link>
            }
          />
          <ActivityList activities={latestActivities} referenceIso={data.updatedAt} compact />
        </Card>
      </div>

      <div className="home-finance-grid">
        <Card className="home-monthly-card">
          <SectionHeading title="Monthly overview" />
          <div className="home-monthly-metrics">
            <div className="home-monthly-metric" title="Value change, not investment return">
              <span>Portfolio value</span>
              <strong
                className={
                  portfolioChange == null ? 'muted' : portfolioChange >= 0 ? 'positive' : 'negative'
                }
              >
                {formatPercent(portfolioChange)}
              </strong>
            </div>
            <div className="home-monthly-metric">
              <span>Income</span>
              <strong>{formatCurrency(income)}</strong>
            </div>
            <div
              className="home-monthly-metric"
              title={
                spendingAccount
                  ? accountDisplayName(
                      spendingAccount.id,
                      spendingAccount.name,
                      accountDisplayNames,
                    )
                  : 'Choose a spending account in Settings'
              }
            >
              <span>Spending</span>
              <strong>{formatCurrency(spending.total)}</strong>
            </div>
          </div>
        </Card>

        <Card className="home-breakdown-card">
          <SectionHeading title="Spending" />
          <div className="home-donut-content">
            <DonutChart
              segments={spendingSegments}
              label="Spending by category"
              centerValue={formatCompactCurrency(spending.total)}
              centerLabel="spent"
            />
            <div className="home-donut-legend">
              {spendingSegments.length ? (
                spendingSegments.map((segment) => (
                  <div className="home-donut-legend-row" key={segment.name}>
                    <span className="home-donut-swatch" style={{ background: segment.color }} />
                    <span>{segment.name}</span>
                    <strong>{formatCompactCurrency(segment.value)}</strong>
                  </div>
                ))
              ) : (
                <small className="home-donut-empty">No spending activity</small>
              )}
            </div>
          </div>
        </Card>

        <Card className="home-breakdown-card home-assets-card">
          <SectionHeading title="Assets" />
          <div className="home-donut-content">
            <DonutChart
              segments={assetAccounts}
              label="Assets by account"
              centerValue={formatCompactCurrency(totalAssets)}
              centerLabel="assets"
            />
            <div className="home-asset-legend">
              {assetGroups.map((group) => (
                <div className="home-asset-group" key={group.id}>
                  <div className="home-donut-legend-row">
                    <span className="home-donut-swatch" style={{ background: group.color }} />
                    <span>{group.name}</span>
                    <strong>{formatCompactCurrency(group.value)}</strong>
                  </div>
                </div>
              ))}
              {!assetGroups.length ? (
                <small className="home-donut-empty">No valued assets</small>
              ) : null}
            </div>
          </div>
        </Card>
      </div>
    </div>
  )
}
