import NumberFlow from '@number-flow/react'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useEffect, useState } from 'react'

import { HoldingsPieChart, PerformanceChart } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, Change, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { formatCurrency, formatUpdatedAt } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'

function totalNetWorthPoints(data: FinanceSnapshot) {
  const history = data.netWorthHistory.length
    ? data.netWorthHistory
    : [{ date: data.updatedAt.slice(0, 10), value: data.netWorth }]
  const brokerageTotal = data.brokeragePerformance.find((item) => item.accountId === 'total')
  const benchmarkSource = data.benchmarkHistory.length
    ? data.benchmarkHistory
    : (brokerageTotal?.points.flatMap((point) =>
        typeof point.sp500 === 'number' ? [{ date: point.date, value: point.sp500 }] : [],
      ) ?? [])
  const benchmarkByDate = new Map(benchmarkSource.map((point) => [point.date, point.value]))
  const usesIsoDates = history.every((point) => /^\d{4}-\d{2}-\d{2}$/.test(point.date))
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
  const query = useFinance()
  const reduceMotion = useReducedMotion()
  const [accountIndex, setAccountIndex] = useState(0)
  const [direction, setDirection] = useState(0)
  const brokerageAccounts =
    query.data?.brokeragePerformance.filter((item) => item.accountId !== 'total') ?? []
  const accountCount = brokerageAccounts.length + 1

  useEffect(() => {
    if (accountIndex >= accountCount) setAccountIndex(0)
  }, [accountIndex, accountCount])

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const accountViews = [
    {
      accountId: 'net-worth',
      name: 'Total net worth',
      institution: 'All accounts',
      currentValue: data.netWorth,
      points: totalNetWorthPoints(data),
    },
    ...brokerageAccounts,
  ]
  const account = accountViews[accountIndex] ?? accountViews[0]

  const changeAccount = (nextDirection: number) => {
    if (accountCount < 2) return
    setDirection(nextDirection)
    setAccountIndex((current) => (current + nextDirection + accountCount) % accountCount)
  }

  const points = account.points
  const latestValue = points.at(-1)?.value ?? account.currentValue
  const priorValue = points.at(-2)?.value ?? latestValue
  const dailyChange = latestValue - priorValue
  const dailyChangePercent = priorValue ? (dailyChange / Math.abs(priorValue)) * 100 : 0
  const changePeriod = /^\d{4}-\d{2}-\d{2}$/.test(points.at(-1)?.date ?? '')
    ? 'today'
    : 'this period'
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
      <header className="page-header overview-header">
        <div className="header-actions">
          <span className="freshness">
            <StatusDot /> Updated {formatUpdatedAt(data.updatedAt)}
          </span>
          <RefreshButton />
        </div>
      </header>

      <section className="hero-balance brokerage-hero" aria-live="polite">
        <p>
          {account.name} <span>· {account.institution}</span>
        </p>
        <NumberFlow
          value={latestValue}
          format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
          className="hero-number"
        />
        <div className="hero-change">
          <Change value={dailyChangePercent} />
          <span>
            {formatCurrency(dailyChange)} {changePeriod}
          </span>
        </div>
      </section>

      <Card className="net-worth-card brokerage-performance-card">
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
                  data={points}
                  valueLabel={account.accountId === 'net-worth' ? 'Net worth' : 'Value'}
                  showNetDeposits={account.accountId !== 'net-worth'}
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

      <Card className="holdings-overview-card">
        <SectionHeading title="Holdings" />
        <HoldingsPieChart data={data.holdings} />
      </Card>
    </div>
  )
}
