import { useMemo } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName } from '../lib/account-names'
import { accountStartDate } from '../lib/account-start-dates'
import { buildChartEventGroups, summaryChartEvents } from '../lib/chart-events'
import {
  accountValueIncomplete,
  dashboardAccountViews,
  hasValueHistory,
} from '../lib/dashboard-account-views'
import { financeSettings } from '../lib/finance-settings'
import { type ChartPeriod, chartPeriodSeconds } from '../lib/graph-preferences'
import { buildLiveChartData, chartPointsFromStartDate } from '../lib/live-chart'
import type { FinanceSnapshot } from '../lib/schema'
import { PerformanceChart } from './charts'
import { ValueHistoryEmptyState } from './data-state'
import { useInvestmentSummary } from './investment-summary-card'
import { InvestmentRangeSelect, PeriodChange, periodStatement } from './investments-summary'
import { AnimatedCurrency, Card } from './ui'

/**
 * The page's focal card: what the investments are worth, how the period went, and the value
 * over the period beside deposits and the S&P 500, with the page's period chosen here. Today is
 * told once, in the summary below.
 */
export function InvestmentsPortfolio({
  data,
  value,
  range,
}: {
  data: FinanceSnapshot
  value: number | null
  range: ChartPeriod
}) {
  const live = useLiveFinance()
  const committed = useFinance().data ?? data
  const settings = financeSettings(committed)
  const now = Date.now() / 1_000
  const graphWindow = chartPeriodSeconds(range)
  const views = useMemo(
    () =>
      dashboardAccountViews(data)
        .filter(({ accountId }) => accountId !== 'net-worth')
        .map((view) => ({
          ...view,
          name:
            view.accountId === 'total'
              ? 'All investments'
              : accountDisplayName(view.accountId, view.name, settings.accountNames),
        })),
    [data, settings.accountNames],
  )
  // The whole portfolio; a single brokerage's view stands in when there is no total.
  const view = views.find(({ accountId }) => accountId === 'total') ?? views[0]
  // The summary's sourced events sit on the line beside the activity.
  const { summary } = useInvestmentSummary(data, range)
  const listed = summary.data?.events
  const chartEvents = useMemo(
    () =>
      view
        ? buildChartEventGroups(
            committed,
            view.accountId,
            graphWindow,
            summaryChartEvents(listed ?? []),
          )
        : [],
    [committed, view, graphWindow, listed],
  )
  const total = !view || view.accountId === 'total'
  const accountId = view?.accountId ?? 'total'
  // The statement covers the combined brokerages; a single brokerage standing in has none.
  const span = accountId === 'total' ? periodStatement(data, range) : undefined

  const livePoints = view ? (live.marketSeries[view.accountId] ?? []) : []
  const startDate = view ? accountStartDate(data, view.accountId, settings.accountStartDates) : ''
  const incomplete = view ? accountValueIncomplete(data, view.accountId) : true
  const chartData = view ? buildLiveChartData(view.points, livePoints, view.currentValue, now) : []
  const historyIsAvailable =
    view != null &&
    !incomplete &&
    (hasValueHistory(view.points, startDate) ||
      chartPointsFromStartDate(livePoints, startDate).length > 1)
  const valueOnly = view?.performanceMethod === 'value-only'
  const netDeposits =
    !view || valueOnly
      ? undefined
      : buildLiveChartData(
          view.points.flatMap((point) =>
            typeof point.netDeposits === 'number'
              ? [{ date: point.date, value: point.netDeposits }]
              : [],
          ),
          [],
          view.points.findLast((point) => typeof point.netDeposits === 'number')?.netDeposits ?? 0,
          now,
        )
  const benchmarkHistory =
    !view || valueOnly
      ? []
      : view.points.flatMap((point) =>
          typeof point.sp500 === 'number' ? [{ date: point.date, value: point.sp500 }] : [],
        )
  const benchmark = benchmarkHistory.length
    ? buildLiveChartData(benchmarkHistory, [], benchmarkHistory.at(-1)?.value ?? 0, now)
    : undefined

  return (
    <Card className="investments-portfolio-card brokerage-performance-card">
      <header className="home-balance-header">
        <div className="home-balance-main">
          <h2 className="balance-label">{view?.name ?? 'All investments'}</h2>
          <div className="home-balance-value">
            <AnimatedCurrency className="hero-number" value={total ? value : view.currentValue} />
          </div>
          <PeriodChange span={span} />
        </div>
        <div className="chart-header-aside">
          <InvestmentRangeSelect />
        </div>
      </header>
      <div className="brokerage-chart-viewport">
        <div key={accountId} className="brokerage-chart-slide">
          <div className="brokerage-chart-content">
            {historyIsAvailable && view ? (
              <PerformanceChart
                data={chartData}
                netDeposits={netDeposits}
                benchmark={benchmark}
                value={view.currentValue}
                events={chartEvents}
                referenceIso={data.updatedAt}
                startDate={startDate}
                selectedWindow={graphWindow}
              />
            ) : (
              <ValueHistoryEmptyState
                incomplete={view ? incomplete : Boolean(data.insights?.allocation?.incomplete)}
                valueAvailable={view ? !incomplete : data.insights?.allocation != null}
              />
            )}
          </div>
        </div>
      </div>
    </Card>
  )
}
