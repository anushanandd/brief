import NumberFlow from '@number-flow/react'
import { listen } from '@tauri-apps/api/event'
import {
  ChartNoAxesCombined,
  ChevronLeft,
  ChevronRight,
  CreditCard,
  Landmark,
  type LucideIcon,
  WalletCards,
} from 'lucide-react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useEffect, useMemo, useState } from 'react'

import { BrandMark } from '../components/brand-mark'
import { AllocationChart, PerformanceChart } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, Change, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { buildNetWorthAllocation } from '../lib/allocation'
import { isTauri, openExternalUrl } from '../lib/api'
import { formatCurrency, formatPercent, formatUpdatedAt } from '../lib/format'
import { getDefaultGraphWindow, graphWindows } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import { stockLogoUrl, transactionLogoUrl, transactionMarkKind } from '../lib/logos'
import { spendingCategoryColor } from '../lib/normalize'
import { annotateKeyMoments } from '../lib/performance'
import type { FinanceSnapshot } from '../lib/schema'

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

function totalNetWorthPoints(data: FinanceSnapshot) {
  const history = data.netWorthHistory.length
    ? data.netWorthHistory
    : [{ date: data.updatedAt.slice(0, 10), value: data.netWorth }]
  const brokerageTotal = data.brokeragePerformance.find((item) => item.accountId === 'total')
  const usesIsoDates = history.every((point) => /^\d{4}-\d{2}-\d{2}$/.test(point.date))
  const benchmarkSource =
    usesIsoDates && data.benchmarkHistory.length
      ? data.benchmarkHistory
      : (brokerageTotal?.points.flatMap((point) =>
          typeof point.sp500 === 'number' ? [{ date: point.date, value: point.sp500 }] : [],
        ) ?? [])
  const benchmarkByDate = new Map(benchmarkSource.map((point) => [point.date, point.value]))
  const sortedBenchmark = usesIsoDates
    ? benchmarkSource.toSorted((left, right) => left.date.localeCompare(right.date))
    : []
  let benchmarkCursor = 0
  let lastBenchmark: number | undefined
  let benchmarkBaseline: number | undefined
  let benchmarkStartingValue: number | undefined

  return history.map((point) => {
    if (usesIsoDates) {
      while (benchmarkCursor < sortedBenchmark.length) {
        const benchmark = sortedBenchmark[benchmarkCursor]
        if (!benchmark || benchmark.date > point.date) break
        lastBenchmark = benchmark.value
        benchmarkCursor += 1
      }
    } else {
      lastBenchmark = benchmarkByDate.get(point.date) ?? lastBenchmark
    }
    if (benchmarkBaseline === undefined && lastBenchmark !== undefined) {
      benchmarkBaseline = lastBenchmark
      benchmarkStartingValue = point.value
    }

    return {
      date: point.date,
      value: point.value,
      // Total net worth has no meaningful brokerage-deposit line. This placeholder is excluded
      // from the chart; account views calculate and render their own external cash flows.
      netDeposits: point.value,
      sp500:
        lastBenchmark && benchmarkBaseline && benchmarkStartingValue !== undefined
          ? Math.round(benchmarkStartingValue * (lastBenchmark / benchmarkBaseline) * 100) / 100
          : null,
    }
  })
}

export function DashboardPage() {
  const query = useFinance({ liveMarket: true })
  const reduceMotion = useReducedMotion()
  const [direction, setDirection] = useState(0)
  const [activeAccountId, setActiveAccountId] = useState('all')
  const [graphWindow, setGraphWindow] = useState(getDefaultGraphWindow)
  const accountViews = useMemo(
    () =>
      query.data
        ? [
            {
              accountId: 'net-worth',
              name: 'Total net worth',
              institution: 'All accounts',
              currentValue: query.data.netWorth,
              points: totalNetWorthPoints(query.data),
            },
            ...query.data.brokeragePerformance
              .filter((item) => item.accountId !== 'total')
              .toSorted((left, right) => right.currentValue - left.currentValue)
              .slice(0, 2),
          ].toSorted((left, right) => right.currentValue - left.currentValue)
        : [],
    [query.data],
  )
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
      if (shortcut === 'graph-previous' || shortcut === 'graph-next') {
        if (accountCount < 2) return
        const nextDirection = shortcut === 'graph-previous' ? -1 : 1
        setDirection(nextDirection)
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
        return
      }

      const windowLabels: Record<string, string> = {
        'graph-week': '1W',
        'graph-month': '1M',
        'graph-three-months': '3M',
        'graph-all': 'All',
      }
      const nextWindow = graphWindows.find((window) => window.label === windowLabels[shortcut])
      if (nextWindow) setGraphWindow(nextWindow.secs)
    }

    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      if (
        !(event.metaKey || event.ctrlKey) ||
        event.altKey ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }

      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault()
        runShortcut(event.key === 'ArrowLeft' ? 'graph-previous' : 'graph-next')
        return
      }

      const shortcuts: Record<string, string> = {
        w: 'graph-week',
        m: 'graph-month',
        '3': 'graph-three-months',
        a: 'graph-all',
      }
      const shortcut = shortcuts[event.key.toLowerCase()]
      if (shortcut) {
        event.preventDefault()
        runShortcut(shortcut)
      }
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
  const currentMonth = data.updatedAt.slice(0, 7)
  const currentMonthShort = new Date(data.updatedAt).toLocaleDateString('en-US', {
    month: 'short',
    timeZone: 'UTC',
  })
  const monthlyTransactions = data.transactions.filter(
    (transaction) =>
      transaction.date.startsWith(currentMonth) || transaction.date.startsWith(currentMonthShort),
  )
  const monthlyIncome = monthlyTransactions
    .filter((transaction) => transaction.amount > 0)
    .reduce((sum, transaction) => sum + transaction.amount, 0)
  const monthlyNetIncome = monthlyIncome - data.spending.monthTotal
  const biggestTransaction = monthlyTransactions.toSorted(
    (left, right) => Math.abs(right.amount) - Math.abs(left.amount),
  )[0]
  const netWorthAllocation = buildNetWorthAllocation(data)
  const spendingCategories = data.spending.categories.map((category, index) => ({
    ...category,
    color: spendingCategoryColor(category.name, index),
  }))
  const recentTransactions = data.transactions.slice(0, 3)
  const marketIsLive = query.marketStreamState === 'live'
  const marketHasActivePrices = marketIsLive && query.marketStreamMessage !== 'Market closed'
  const marketIsConnecting = ['connecting', 'reconnecting'].includes(query.marketStreamState)
  const marketIsPolling = query.marketStreamState === 'error'

  const changeAccount = (nextDirection: number) => {
    if (accountCount < 2) return
    setDirection(nextDirection)
    const nextIndex = (accountIndex + nextDirection + accountCount) % accountCount
    const nextAccount = accountViews[nextIndex]
    if (nextAccount) setActiveAccountId(nextAccount.accountId)
  }

  const points = annotateKeyMoments(account.points, {
    transactions: data.transactions,
    investmentActivities: data.investmentActivities,
    currentValue: account.currentValue,
    accountId: account.accountId,
  })
  const latestValue = points.at(-1)?.value ?? account.currentValue
  const priorValue = points.at(-2)?.value ?? latestValue
  const dailyChange = latestValue - priorValue
  const dailyChangePercent = priorValue ? (dailyChange / Math.abs(priorValue)) * 100 : 0
  const changePeriod = /^\d{4}-\d{2}-\d{2}$/.test(points.at(-1)?.date ?? '')
    ? 'today'
    : 'this period'
  const chartNow = Date.now() / 1_000
  const chartData = buildLiveChartData(points, livePoints ?? [], latestValue, chartNow)
  const netDepositsData =
    account.accountId === 'net-worth'
      ? undefined
      : buildLiveChartData(
          points.map((point) => ({ date: point.date, value: point.netDeposits })),
          [],
          points.at(-1)?.netDeposits ?? 0,
          chartNow,
        )
  const benchmarkHistory = points.flatMap((point) =>
    typeof point.sp500 === 'number' ? [{ date: point.date, value: point.sp500 }] : [],
  )
  const benchmarkData = benchmarkHistory.length
    ? buildLiveChartData(benchmarkHistory, [], benchmarkHistory.at(-1)?.value ?? 0, chartNow)
    : undefined
  const slideVariants = {
    enter: (slideDirection: number) => ({
      opacity: 0,
      transform: reduceMotion ? 'translateX(0%)' : `translateX(${slideDirection >= 0 ? 18 : -18}%)`,
    }),
    center: { opacity: 1, transform: 'translateX(0%)' },
    exit: (slideDirection: number) => ({
      opacity: 0,
      transform: reduceMotion ? 'translateX(0%)' : `translateX(${slideDirection >= 0 ? -18 : 18}%)`,
    }),
  }

  return (
    <div className="page dashboard-page dashboard-focus">
      <header className="dashboard-topbar">
        <div>
          <NumberFlow
            value={latestValue}
            format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
            className="hero-number"
          />
          <div className="hero-change">
            <Change value={dailyChangePercent} />
            <span className={dailyChange > 0 ? 'positive' : dailyChange < 0 ? 'negative' : 'muted'}>
              {formatCurrency(dailyChange)} {changePeriod}
            </span>
          </div>
        </div>
        <div className="dashboard-actions">
          <span className="freshness" title={query.marketStreamMessage} aria-live="polite">
            <StatusDot tone={marketHasActivePrices ? 'positive' : 'neutral'} />
            {marketIsLive
              ? (query.marketStreamMessage ?? 'Live prices')
              : marketIsConnecting
                ? 'Connecting prices'
                : marketIsPolling
                  ? 'Polling prices'
                  : `Updated ${formatUpdatedAt(data.updatedAt)}`}
          </span>
          <RefreshButton />
        </div>
      </header>

      <Card className="net-worth-card brokerage-performance-card">
        <header className="brokerage-card-header">
          <h1 className="overview-account-title">
            {account.name} <span>· {account.institution}</span>
          </h1>
        </header>

        <div className="brokerage-chart-viewport">
          <AnimatePresence initial={false} custom={direction} mode="popLayout">
            <motion.div
              key={account.accountId}
              className="brokerage-chart-slide"
              custom={direction}
              variants={slideVariants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={
                reduceMotion
                  ? { duration: 0.15, ease: [0.23, 1, 0.32, 1] }
                  : { type: 'spring', duration: 0.5, bounce: 0.2 }
              }
            >
              <div className="brokerage-chart-content">
                <PerformanceChart
                  data={chartData}
                  netDeposits={netDepositsData}
                  benchmark={benchmarkData}
                  value={latestValue}
                  isLive={marketHasActivePrices}
                  selectedWindow={graphWindow}
                  onWindowChange={setGraphWindow}
                />
              </div>
            </motion.div>
          </AnimatePresence>
        </div>

        {accountCount > 1 ? (
          <nav className="brokerage-chart-nav" aria-label="Investment accounts">
            <button type="button" onClick={() => changeAccount(-1)} aria-label="Previous account">
              <ChevronLeft size={15} />
            </button>
            <div className="brokerage-page-dots">
              {accountViews.map((item, index) => (
                <span
                  key={item.accountId}
                  className={index === accountIndex ? 'active' : undefined}
                  aria-hidden="true"
                />
              ))}
            </div>
            <button type="button" onClick={() => changeAccount(1)} aria-label="Next account">
              <ChevronRight size={15} />
            </button>
          </nav>
        ) : null}
      </Card>

      <div className="overview-grid">
        <Card className="holdings-overview-card">
          <SectionHeading title="Holdings" />
          <div className="overview-list holdings-list">
            {data.holdings
              .toSorted((left, right) => right.value - left.value)
              .map((holding) => {
                const yahooUrl = `https://finance.yahoo.com/quote/${encodeURIComponent(holding.ticker)}`
                return (
                  <a
                    className="overview-holding-row"
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
                        fallback={holding.ticker.slice(0, 1)}
                        label={`${holding.name} logo`}
                        src={stockLogoUrl(holding.ticker)}
                        style={{ backgroundColor: holding.color }}
                      />
                      <span>
                        <strong className="overview-holding-ticker">{holding.ticker}</strong>
                        <small>{holding.name.split(/\s+/).slice(0, 3).join(' ')}</small>
                      </span>
                    </span>
                    <span>{formatCurrency(holding.price)}</span>
                    <span className={holding.dailyChangePct >= 0 ? 'positive' : 'negative'}>
                      {formatPercent(holding.dailyChangePct)}
                    </span>
                    <span className={holding.totalChangePct >= 0 ? 'positive' : 'negative'}>
                      {formatPercent(holding.totalChangePct)}
                    </span>
                    <span>{formatCurrency(holding.value)}</span>
                  </a>
                )
              })}
          </div>
        </Card>

        <Card className="accounts-overview-card">
          <SectionHeading title="Accounts" />
          <div className="overview-list">
            {data.accounts
              .slice(1)
              .toSorted((left, right) => right.value - left.value)
              .map((item) => (
                <div className="overview-account-row" key={item.id}>
                  <AccountTypeMark type={item.type} />
                  <span className="account-name">
                    <span>{item.name}</span>
                    <small>{item.type}</small>
                  </span>
                  <span>{formatCurrency(item.value)}</span>
                </div>
              ))}
          </div>
        </Card>
      </div>

      <div className="overview-insights-grid">
        <Card className="overview-income-card">
          <SectionHeading title="Monthly net income" />
          <div className="overview-income-summary">
            <NumberFlow
              value={monthlyNetIncome}
              format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
              className={monthlyNetIncome >= 0 ? 'positive' : 'negative'}
            />
          </div>
          <div className="overview-income-breakdown">
            <span>
              <small>Income</small>
              <strong>{formatCurrency(monthlyIncome)}</strong>
            </span>
            <span>
              <small>Spending</small>
              <strong>{formatCurrency(data.spending.monthTotal)}</strong>
            </span>
            {biggestTransaction ? (
              <span className="overview-income-largest">
                <span>
                  <small>Biggest transaction</small>
                  <strong>{biggestTransaction.merchant}</strong>
                </span>
                <strong className={biggestTransaction.amount > 0 ? 'positive' : undefined}>
                  {formatCurrency(biggestTransaction.amount)}
                </strong>
              </span>
            ) : null}
          </div>
        </Card>

        <Card className="overview-donut-card">
          <SectionHeading title="Monthly spending" />
          <AllocationChart
            data={spendingCategories}
            centerValue={data.spending.monthTotal}
            centerLabel="Spent"
          />
        </Card>

        <Card className="overview-donut-card">
          <SectionHeading title="Net worth allocation" />
          <AllocationChart
            data={netWorthAllocation}
            centerValue={data.netWorth}
            centerLabel="Net worth"
          />
        </Card>

        <Card className="overview-transactions-card">
          <SectionHeading title="Recent transactions" />
          <div className="overview-recent-list">
            {recentTransactions.length ? (
              recentTransactions.map((transaction) => {
                const markKind = transactionMarkKind(transaction)
                return (
                  <div className="overview-recent-row" key={transaction.id}>
                    <BrandMark
                      className={`transaction-mark transaction-mark-${markKind}`}
                      fallback={transaction.merchant.slice(0, 1)}
                      label={`${transaction.merchant} transaction icon`}
                      src={transactionLogoUrl(transaction)}
                    />
                    <span className="transaction-name">
                      <strong>{transaction.merchant}</strong>
                      <small>
                        {transaction.pending ? 'Pending · ' : ''}
                        {transaction.category} · {transaction.date}
                      </small>
                    </span>
                    <strong className={transaction.amount > 0 ? 'positive' : undefined}>
                      {formatCurrency(transaction.amount)}
                    </strong>
                  </div>
                )
              })
            ) : (
              <p className="overview-recent-empty">No recent transactions.</p>
            )}
          </div>
        </Card>
      </div>
    </div>
  )
}
