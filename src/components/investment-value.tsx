import type { LivelinePoint } from 'liveline'
import { useMemo } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { asOfWaiting } from '../hooks/use-as-of'
import { useFinance } from '../hooks/use-finance'
import { useGraphScrollShortcuts } from '../hooks/use-graph-window-shortcuts'
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
import {
  type ChartPeriod,
  chartPeriodSeconds,
  useGraphEnd,
  useGraphWindow,
} from '../lib/graph-preferences'
import {
  buildLiveChartData,
  chartDayTime,
  chartPointAtOrBefore,
  chartPointsFromStartDate,
  firstChartDay,
  referenceLines,
} from '../lib/live-chart'
import type { FinanceSnapshot } from '../lib/schema'
import { ChartSpan, ExpandableChart, LineValue, PerformanceChart } from './charts'
import { ValueHistoryEmptyState } from './data-state'
import { useInvestmentSummary } from './investment-summary-card'
import { InvestmentRangeSelect, PeriodChange, periodStatement } from './investments-summary'
import { Card } from './ui'

const noPoints: LivelinePoint[] = []

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
  const graphWindow = chartPeriodSeconds(range)
  // The line reads only the saved histories, which a page scrolled back keeps as they are, so
  // the past day's figures arriving mid-scroll don't rebuild it.
  const { accounts, brokeragePerformance, netWorthHistory, updatedAt, netWorth } = data
  const views = useMemo(
    () =>
      dashboardAccountViews({
        accounts,
        brokeragePerformance,
        netWorthHistory,
        updatedAt,
        netWorth,
      })
        .filter(({ accountId }) => accountId !== 'net-worth')
        .map((view) => ({
          ...view,
          name:
            view.accountId === 'total'
              ? 'All investments'
              : accountDisplayName(view.accountId, view.name, settings.accountNames),
        })),
    [accounts, brokeragePerformance, netWorthHistory, updatedAt, netWorth, settings.accountNames],
  )
  // The whole portfolio; a single brokerage's view stands in when there is no total.
  const view = views.find(({ accountId }) => accountId === 'total') ?? views[0]
  // The summary's sourced events sit on the line beside the activity; it describes today.
  const { current } = useInvestmentSummary(committed, range)
  const listed = current?.events
  const [, setGraphWindow] = useGraphWindow()
  const [graphEnd] = useGraphEnd()
  // Event labels carry formatted amounts, so they are built again when the mask changes.
  const amountsHidden = useAmountsHidden()
  const chartEvents = useMemo(
    () =>
      view
        ? buildChartEventGroups(
            committed,
            view.accountId,
            graphWindow,
            summaryChartEvents(listed ?? []),
            graphEnd,
          )
        : [],
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- the mask changes what is formatted
    [committed, view, graphWindow, listed, graphEnd, amountsHidden],
  )
  const total = !view || view.accountId === 'total'
  const accountId = view?.accountId ?? 'total'
  // The statement covers the combined brokerages; a single brokerage standing in has none.
  const span = accountId === 'total' ? periodStatement(data, range) : undefined

  const livePoints = (view && live.marketSeries[view.accountId]) || noPoints
  const startDate = view ? accountStartDate(data, view.accountId, settings.accountStartDates) : ''
  const incomplete = view ? accountValueIncomplete(data, view.accountId) : true
  // The same points give the same arrays, so the chart redraws only when a point changes.
  const { chartData, netDeposits, benchmark } = useMemo(() => {
    const now = Date.now() / 1_000
    return {
      chartData: view ? buildLiveChartData(view.points, livePoints, view.currentValue, now) : [],
      ...referenceLines(view, now),
    }
  }, [view, livePoints])
  const historyIsAvailable =
    view != null &&
    !incomplete &&
    (hasValueHistory(view.points, startDate) ||
      chartPointsFromStartDate(livePoints, startDate).length > 1)
  const today = committed.insights?.today
  const scroll = useMemo(
    () => (today ? { today, first: firstChartDay(chartData, startDate) } : undefined),
    [today, chartData, startDate],
  )
  useGraphScrollShortcuts(historyIsAvailable ? scroll : undefined)
  const currentValue = total ? value : (view?.currentValue ?? null)
  // Scrolled back, the hero is the value saved for the end day.
  const shownValue = graphEnd
    ? (chartPointAtOrBefore(chartData, chartDayTime(graphEnd))?.value ?? currentValue)
    : currentValue

  return (
    <Card className="investments-portfolio-card brokerage-performance-card">
      <ExpandableChart
        title={view?.name ?? 'All investments'}
        seconds={graphWindow}
        setSeconds={setGraphWindow}
        scroll={scroll}
      >
        {() => (
          <>
            <header className="home-balance-header">
              <div className="home-balance-main">
                <h2 className="balance-label">{view?.name ?? 'All investments'}</h2>
                <div className="home-balance-value">
                  <LineValue data={chartData} value={shownValue} />
                </div>
                <PeriodChange
                  span={span}
                  value={historyIsAvailable ? chartPointsFromStartDate(chartData, startDate) : []}
                  deposits={netDeposits && chartPointsFromStartDate(netDeposits, startDate)}
                  seconds={graphWindow}
                  waitingAt={graphEnd && asOfWaiting(data) ? chartDayTime(graphEnd) : undefined}
                />
              </div>
              <div className="chart-header-aside">
                <ChartSpan seconds={graphWindow} />
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
                      scroll={scroll}
                    />
                  ) : (
                    <ValueHistoryEmptyState
                      incomplete={
                        view ? incomplete : Boolean(data.insights?.allocation?.incomplete)
                      }
                      valueAvailable={view ? !incomplete : data.insights?.allocation != null}
                    />
                  )}
                </div>
              </div>
            </div>
          </>
        )}
      </ExpandableChart>
    </Card>
  )
}
