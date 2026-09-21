import type { LivelinePoint } from 'liveline'

import { accountValueIncomplete, hasValueHistory } from './dashboard-account-views'
import { buildLiveChartData, chartRangeChange } from './live-chart'
import type { FinanceSnapshot, Transaction } from './schema'
import { isSpendingTransaction, transactionDateKey } from './spending'
import { transactionMarkKind } from './transaction-kind'

export type OverviewRange = { start: number; end: number }
const day = 86_400
const dateKey = (seconds: number) => {
  const date = new Date(seconds * 1000)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

// Partial native market-change intervals use the same linear boundary estimate as value charts.
function performance(
  history: Array<{ date: string; value: number; marketChange?: number | null }>,
  range: OverviewRange,
  market: boolean,
) {
  const points = history
    .map((point) => ({ ...point, time: Date.parse(`${point.date}T12:00:00Z`) / 1000 }))
    .filter(
      ({ time, value }) => Number.isFinite(time) && Number.isFinite(value) && time <= range.end,
    )
    .toSorted((a, b) => a.time - b.time)
  const closing = points.at(-1)
  const before = points.findLastIndex(({ time }) => time <= range.start)
  if (before < 0 || !closing || closing.time <= range.start || range.end - closing.time > 7 * day)
    return null
  const opening = points[before]
  const next = points[before + 1]
  const fraction = next ? (range.start - opening.time) / (next.time - opening.time) : 0
  const baseline = opening.value + (next ? next.value - opening.value : 0) * fraction
  if (!baseline) return null
  if (!market) return ((closing.value - baseline) / Math.abs(baseline)) * 100
  let gain = 0
  for (let index = before + 1; index < points.length; index++) {
    const point = points[index]
    if (point.marketChange == null) return null
    gain += point.marketChange * (index === before + 1 ? 1 - fraction : 1)
  }
  return (gain / Math.abs(baseline)) * 100
}

const total = (items: Transaction[]) =>
  items.reduce((sum, t) => sum + Math.round(Math.abs(t.amount) * 100), 0) / 100

export function homeOverviewMetrics(
  data: FinanceSnapshot,
  spendingAccountId: string | undefined,
  range: OverviewRange,
  allAccountsPoints: LivelinePoint[] = buildLiveChartData(
    data.netWorthHistory,
    [],
    data.netWorth,
    range.end,
  ),
) {
  const start = dateKey(range.start)
  const end = dateKey(Math.min(range.end, Date.parse(data.updatedAt) / 1000))
  const valid =
    Number.isFinite(range.start) && Number.isFinite(range.end) && range.end > range.start
  const posted = valid
    ? data.transactions.filter((transaction) => {
        const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
        const account = data.accounts.find(({ id }) => id === transaction.accountId)
        return (
          !transaction.pending &&
          date >= start &&
          date <= end &&
          (!account?.currency || account.currency === 'USD')
        )
      })
    : []
  const expenses = posted.filter(
    (t) => t.accountId === spendingAccountId && isSpendingTransaction(t),
  )
  const income = posted.filter(
    (t) => t.amount > 0 && ['income', 'interest', 'dividend'].includes(transactionMarkKind(t)),
  )
  const portfolio = data.brokeragePerformance.find(({ accountId }) => accountId === 'total')
  const benchmarkDates = new Set(data.benchmarkHistory.map((point) => point.date))
  const comparisonEnd = Math.max(
    ...(portfolio?.points ?? [])
      .filter((point) => benchmarkDates.has(point.date))
      .map((point) => Date.parse(point.date + 'T12:00:00Z') / 1000)
      .filter((time) => time <= range.end),
  )
  const comparisonRange = { ...range, end: comparisonEnd }
  const comparable = valid && Number.isFinite(comparisonEnd) && range.end - comparisonEnd <= 7 * day
  const accountChange = chartRangeChange(allAccountsPoints, range.end - range.start, range.end)
  return {
    portfolio:
      comparable &&
      !accountValueIncomplete(data, 'total') &&
      portfolio?.performanceMethod === 'value-with-comparisons'
        ? performance(portfolio.points, comparisonRange, true)
        : null,
    benchmark: comparable ? performance(data.benchmarkHistory, comparisonRange, false) : null,
    allAccounts:
      valid &&
      !data.netWorthIncomplete &&
      hasValueHistory(data.netWorthHistory) &&
      allAccountsPoints.some((p) => p.time <= range.start && p.value !== 0)
        ? accountChange.percent
        : null,
    spending: valid && end >= start && spendingAccountId ? total(expenses) : null,
    income: valid && end >= start ? total(income) : null,
  }
}
