import { buildLiveChartData, chartPointsFromStartDate, chartRangeChange } from './live-chart'
import type { Account, FinanceSnapshot } from './schema'

export function accountValueChart(
  data: Pick<
    FinanceSnapshot,
    | 'accounts'
    | 'netWorthIncomplete'
    | 'netWorthHistory'
    | 'brokeragePerformance'
    | 'accountBalanceHistory'
    | 'updatedAt'
  >,
  account: Account,
  livePoints: Parameters<typeof buildLiveChartData>[1],
  valuationTime: number,
  windowSeconds: number,
  now: number,
  startDate?: string,
) {
  const combined = account.id === 'all' || account.type === 'combined'
  const investment = account.type === 'brokerage' || account.type === 'retirement'
  const history =
    account.value == null
      ? []
      : combined
        ? data.netWorthHistory
        : ((investment ? data.brokeragePerformance : data.accountBalanceHistory).find(
            ({ accountId }) => accountId === account.id,
          )?.points ?? [])
  const chartData =
    account.value == null
      ? []
      : buildLiveChartData(history, livePoints, account.value, valuationTime)
  const incomplete = accountValueIncomplete(data, combined ? 'all' : account.id)
  return {
    chartData,
    chartChange: chartRangeChange(
      chartPointsFromStartDate(chartData, startDate),
      windowSeconds,
      now,
    ),
    incomplete,
    historyIsAvailable: !incomplete && hasValueHistory(history, startDate),
  }
}

type BrokeragePerformance = FinanceSnapshot['brokeragePerformance'][number]
type ValuePoint = { date: string; value: number }
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/
const WEEK_MS = 7 * 24 * 60 * 60 * 1_000

export function accountValueIncomplete(
  data: Pick<FinanceSnapshot, 'accounts' | 'netWorthIncomplete'>,
  accountId: string,
) {
  if (accountId === 'all' || accountId === 'net-worth') return Boolean(data.netWorthIncomplete)
  if (accountId === 'total') {
    return data.accounts.some(
      ({ type, value }) => (type === 'brokerage' || type === 'retirement') && value == null,
    )
  }
  return data.accounts.find(({ id }) => id === accountId)?.value == null
}

export function hasValueHistory(points: ValuePoint[], startDate?: string) {
  return (
    new Set(
      points
        .filter(
          ({ date, value }) =>
            DATE_PATTERN.test(date) &&
            Number.isFinite(Date.parse(`${date}T00:00:00Z`)) &&
            Number.isFinite(value) &&
            (!startDate || date >= startDate),
        )
        .map(({ date }) => date),
    ).size > 1
  )
}

export function accountWeeklyChangePct(
  data: Pick<
    FinanceSnapshot,
    'accountBalanceHistory' | 'brokeragePerformance' | 'holdings' | 'updatedAt'
  >,
  account: Account,
  valuationAsOf = data.updatedAt,
) {
  if (account.value == null) return null
  const targetTime = Date.parse(valuationAsOf) - WEEK_MS
  if (!Number.isFinite(targetTime)) return null
  const targetDate = new Date(targetTime).toISOString().slice(0, 10)
  const history = [...data.accountBalanceHistory, ...data.brokeragePerformance].find(
    ({ accountId }) => accountId === account.id,
  )
  const baseline = history?.points
    .filter(({ date }) => DATE_PATTERN.test(date) && date <= targetDate)
    .toSorted((left, right) => left.date.localeCompare(right.date))
    .at(-1)
  if (baseline?.value && Date.parse(targetDate) - Date.parse(baseline.date) <= 4 * 86_400_000) {
    return ((account.value - baseline.value) / Math.abs(baseline.value)) * 100
  }

  // A repriced basket of today's positions is not an account balance change.
  return null
}

function totalNetWorthPoints(
  data: Pick<FinanceSnapshot, 'netWorthHistory' | 'updatedAt' | 'netWorth'>,
) {
  const history = data.netWorthHistory?.length
    ? data.netWorthHistory
    : [{ date: data.updatedAt.slice(0, 10), value: data.netWorth }]
  return history.map((point) => ({ ...point, netDeposits: null, sp500: null }))
}

function accountRank({ name }: BrokeragePerformance) {
  const normalizedName = name.toLocaleLowerCase()
  if (normalizedName.includes('roth')) return 0
  if (/stock\s*plan/.test(normalizedName)) return 2
  return 1
}

export function dashboardAssetBreakdown(data: Pick<FinanceSnapshot, 'accounts' | 'holdings'>) {
  return data.accounts
    .filter(({ id, value }) => id !== 'all' && value != null && value > 0)
    .toSorted((left, right) => (right.value ?? 0) - (left.value ?? 0))
    .map((account) => {
      const holdings = data.holdings.filter(
        ({ accountId, value }) => accountId === account.id && value != null && value > 0,
      )
      const holdingsValue = holdings.reduce((total, { value }) => total + (value ?? 0), 0)
      const remainder = Math.max(0, (account.value ?? 0) - holdingsValue)
      return {
        id: account.id,
        name: account.name,
        type: account.type,
        value: account.value ?? 0,
        children: holdings.length
          ? [
              ...holdings.map(({ ticker, value }) => ({ name: ticker, value: value ?? 0 })),
              ...(remainder >= 0.01 ? [{ name: 'Cash & other', value: remainder }] : []),
            ]
          : [],
      }
    })
}

export function dashboardAccountViews(
  data: Pick<
    FinanceSnapshot,
    'accounts' | 'brokeragePerformance' | 'netWorthHistory' | 'updatedAt' | 'netWorth'
  >,
): BrokeragePerformance[] {
  const individualBrokerages = data.brokeragePerformance.filter(
    ({ accountId }) => accountId !== 'total',
  )
  const representedAccounts = new Set(individualBrokerages.map(({ accountId }) => accountId))
  const missingInvestmentAccounts = data.accounts
    .filter(
      ({ id, type, value }) =>
        value != null &&
        (type === 'brokerage' || type === 'retirement') &&
        !representedAccounts.has(id),
    )
    .map((account): BrokeragePerformance => ({
      accountId: account.id,
      name: account.name,
      institution: account.institution,
      currentValue: account.value!,
      historySource: 'unavailable',
      historyStart: data.updatedAt.slice(0, 10),
      performanceMethod: 'value-only',
      points: [
        {
          date: data.updatedAt.slice(0, 10),
          value: account.value!,
          netDeposits: null,
          sp500: null,
        },
      ],
    }))
  const orderedBrokerages = [...individualBrokerages, ...missingInvestmentAccounts].toSorted(
    (left, right) => accountRank(left) - accountRank(right) || left.name.localeCompare(right.name),
  )
  const total = data.brokeragePerformance.find(({ accountId }) => accountId === 'total')
  return [
    ...(total ? [{ ...total, name: 'Combined brokerages' }] : []),
    ...orderedBrokerages,
    {
      accountId: 'net-worth',
      name: 'All accounts',
      institution: 'Total net worth',
      currentValue: data.netWorth,
      performanceMethod: 'value-only',
      points: totalNetWorthPoints(data),
    },
  ]
}
