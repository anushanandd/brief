import type { LivelinePoint } from 'liveline'
import { useMemo } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useLiveFinance } from '../hooks/use-live-finance'
import type { AccountGroup } from '../lib/account-groups'
import { buildChartEventGroups } from '../lib/chart-events'
import { accountValueChart } from '../lib/dashboard-account-views'
import {
  chartDayTime,
  chartPointAtOrBefore,
  firstChartDay,
  referenceLines,
} from '../lib/live-chart'
import type { Account, ChartChanges, FinanceSnapshot } from '../lib/schema'

const noPoints: LivelinePoint[] = []
const noChanges: ChartChanges = []

/**
 * What an account's chart draws: its value line with the live market's points merged in, the
 * deposit and benchmark lines for an investment, and its events. Memoized so the chart's line
 * keeps its identity until a point changes. Scrolled back to `end`, its change is Rust's to that
 * day (`pastChanges`, none known until it arrives), its value the one saved then, and its
 * events stop there.
 */
export function useAccountChart({
  data,
  committed,
  account,
  group,
  isAll,
  isInvestment,
  graphWindow,
  startDate,
  end,
  pastChanges,
}: {
  data: FinanceSnapshot
  /** The held snapshot, which edits update; events read it. */
  committed: FinanceSnapshot
  account: Account
  group: AccountGroup | undefined
  isAll: boolean
  isInvestment: boolean
  graphWindow: number
  startDate: string | undefined
  end: string | undefined
  pastChanges: ChartChanges | undefined
}) {
  const live = useLiveFinance()
  const isCombined = isAll || Boolean(group)
  const performance = isCombined
    ? undefined
    : (isInvestment ? data.brokeragePerformance : data.accountBalanceHistory).find(
        ({ accountId }) => accountId === account.id,
      )
  const chartNow = Date.parse(live.valuationAsOf ?? data.updatedAt) / 1_000
  const now = Date.now() / 1_000
  const safeChartNow = Number.isFinite(chartNow) ? chartNow : now
  const marketSeries =
    (group
      ? live.marketSeries[`group:${group.type}`]
      : isAll
        ? live.marketSeries.accounts
        : live.marketSeries[account.id]) ?? noPoints
  const changes = end ? (pastChanges ?? noChanges) : live.chartChanges
  const { chartData, chartChange, incomplete, historyIsAvailable } = useMemo(
    () =>
      accountValueChart(
        data,
        account,
        marketSeries,
        safeChartNow,
        graphWindow,
        changes,
        startDate,
        group,
      ),
    [data, account, marketSeries, safeChartNow, graphWindow, changes, startDate, group],
  )
  const today = committed.insights?.today
  const scroll = useMemo(
    () => (today ? { today, first: firstChartDay(chartData, startDate) } : undefined),
    [today, chartData, startDate],
  )
  const { netDeposits, benchmark } = useMemo(
    () => referenceLines(performance, safeChartNow),
    [performance, safeChartNow],
  )
  // Event labels carry formatted amounts, so they are built again when the mask changes.
  const amountsHidden = useAmountsHidden()
  const chartEvents = useMemo(
    () =>
      isInvestment && !group
        ? buildChartEventGroups(committed, isAll ? 'net-worth' : account.id, graphWindow, [], end)
        : [],
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- the mask changes what is formatted
    [committed, isInvestment, isAll, group, account.id, graphWindow, end, amountsHidden],
  )
  const current = group ? group.value : account.value
  return {
    chartData,
    chartChange,
    incomplete,
    historyIsAvailable,
    // Scrolled back to a day with no saved value, the value is unknown, as on Home.
    chartValue: end ? (chartPointAtOrBefore(chartData, chartDayTime(end))?.value ?? null) : current,
    currentValue: current,
    netDeposits,
    benchmark,
    chartEvents,
    scroll,
  }
}
