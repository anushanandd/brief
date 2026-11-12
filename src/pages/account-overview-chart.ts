import type { LivelinePoint } from 'liveline'
import { useMemo } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useLiveFinance } from '../hooks/use-live-finance'
import type { AccountGroup } from '../lib/account-groups'
import { buildChartEventGroups } from '../lib/chart-events'
import { accountValueChart } from '../lib/dashboard-account-views'
import { referenceLines } from '../lib/live-chart'
import type { Account, FinanceSnapshot } from '../lib/schema'

const noPoints: LivelinePoint[] = []

/**
 * What an account's chart draws: its value line with the live market's points merged in, the
 * deposit and benchmark lines for an investment, and its events. Memoized so the chart's line
 * keeps its identity until a point changes.
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
  const { chartData, chartChange, incomplete, historyIsAvailable } = useMemo(
    () =>
      accountValueChart(
        data,
        account,
        marketSeries,
        safeChartNow,
        graphWindow,
        live.chartChanges,
        startDate,
        group,
      ),
    [data, account, marketSeries, safeChartNow, graphWindow, live.chartChanges, startDate, group],
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
        ? buildChartEventGroups(committed, isAll ? 'net-worth' : account.id, graphWindow)
        : [],
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- the mask changes what is formatted
    [committed, isInvestment, isAll, group, account.id, graphWindow, amountsHidden],
  )
  return {
    chartData,
    chartChange,
    incomplete,
    historyIsAvailable,
    chartValue: group ? group.value : account.value,
    netDeposits,
    benchmark,
    chartEvents,
  }
}
