import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import { ChevronRight } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { ActivityList } from '../components/activity-list'
import { BrandMark } from '../components/brand-mark'
import { DonutChart, PerformanceChart, type DonutSegment } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, Change, SectionHeading, StatusDot } from '../components/ui'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { graphAccountShortcut, useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { accountStartDate, getAccountStartDates } from '../lib/account-start-date-preferences'
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
  accountWeeklyChangePct,
  chartValueChange,
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
import {
  getExternalLogosEnabled,
  markColor,
  stockLogoUrl,
  stockMarkColor,
  stockMarkLabel,
} from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import {
  buildSpendingView,
  formatActivityDate,
  resolveSpendingAccount,
  spendingCategoryColor,
} from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

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
    queryKey: ['weekly-briefing', 2, candidate?.key],
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
    <Card className="home-insights">
      <div className="home-insights-heading">
        <h2>What changed</h2>
        <span>Local overview</span>
      </div>
      {sections.map((section) => (
        <section key={section.title} aria-label={section.title}>
          <ul className="home-insight-summary" aria-live="polite">
            {section.lines.map((line) => (
              <li key={line}>
                <InsightLine>{line}</InsightLine>
              </li>
            ))}
          </ul>
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
  const {
    change: dailyChange,
    percent: dailyChangePercent,
    period: changePeriod,
  } = chartValueChange(points, latestValue, data.updatedAt.slice(0, 10))
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
  const chartEvents = buildChartEventGroups(data, account.accountId, graphWindow)
  const startDate = accountStartDate(data, account.accountId, accountStartDates)
  const accounts = data.accounts
    .filter(({ id }) => id !== 'all')
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const holdingsAccountId = accounts.some(({ id }) => id === account.accountId)
    ? account.accountId
    : ''
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
    Math.max(holdings.length || 2, accounts.length || 2),
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
      color: markColor(group.id),
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
              Started {startDate ? formatActivityDate(startDate, data.updatedAt) : 'unknown'}
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
                    startDate={startDate}
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
          <div
            className="home-account-dots"
            role="group"
            aria-label="Home chart accounts"
            aria-keyshortcuts="Meta+ArrowLeft Meta+ArrowRight"
          >
            {accountViews.map((view) => {
              const active = view.accountId === account.accountId
              return (
                <button
                  key={view.accountId}
                  type="button"
                  className={active ? 'active' : undefined}
                  aria-label={`${view.name}: ${formatCurrency(view.currentValue)}`}
                  aria-pressed={active}
                  title={view.name}
                  onClick={() => setActiveAccountId(view.accountId)}
                />
              )
            })}
          </div>
        </Card>
        <HomeInsights data={data} />
      </div>

      <div className="home-secondary-grid home-holdings-row">
        <Card className="brokerage-holdings-card">
          <section className="brokerage-holdings" aria-label={`${account.name} holdings`}>
            <div className="holding-column-headings">
              <h2>
                <Link
                  to="/holdings"
                  search={holdingsAccountId ? { account: holdingsAccountId } : undefined}
                  className="section-heading-link"
                >
                  Holdings <ChevronRight size={14} aria-hidden="true" />
                </Link>
              </h2>
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
          <SectionHeading
            title={
              <Link to="/accounts" className="section-heading-link">
                Accounts <ChevronRight size={14} aria-hidden="true" />
              </Link>
            }
          />
          <div className="overview-list">
            {accounts.map((item) => {
              const weeklyChange = accountWeeklyChangePct(data, item)
              return (
                <Link
                  className="overview-account-row"
                  key={item.id}
                  to="/accounts/$accountId"
                  params={{ accountId: item.id }}
                >
                  <AccountMark type={item.type} />
                  <span className="account-name">
                    <span>{accountDisplayName(item.id, item.name, accountDisplayNames)}</span>
                    <small>{item.institution}</small>
                  </span>
                  <span className="overview-account-values">
                    <strong>{formatCurrency(item.value)}</strong>
                    <small
                      className={
                        weeklyChange == null
                          ? 'muted'
                          : weeklyChange > 0
                            ? 'positive'
                            : weeklyChange < 0
                              ? 'negative'
                              : 'muted'
                      }
                      title="Seven-day account value change, not investment return"
                    >
                      1W {formatPercent(weeklyChange)}
                    </small>
                  </span>
                </Link>
              )
            })}
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
            {!spendingSegments.length ? (
              <small className="home-donut-empty">No spending activity</small>
            ) : null}
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
            {!assetAccounts.length ? (
              <small className="home-donut-empty">No valued assets</small>
            ) : null}
          </div>
        </Card>
      </div>
    </div>
  )
}
