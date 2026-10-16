import { buildLiveChartData, chartPointsFromStartDate, chartRangeChange } from './live-chart'
import { cents } from './money'
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
  > &
    Partial<Pick<FinanceSnapshot, 'netWorthProvisional'>>,
  account: Account,
  livePoints: Parameters<typeof buildLiveChartData>[1],
  valuationTime: number,
  windowSeconds: number,
  now: number,
  startDate?: string,
) {
  const combined = account.id === 'all' || account.type === 'combined'
  const investment = account.type === 'brokerage' || account.type === 'retirement'
  // An investment account's history is its performance; a manual one, valued by hand or by
  // its statements, has only a balance history.
  const ofAccount = ({ accountId }: { accountId: string }) => accountId === account.id
  const history =
    account.value == null
      ? []
      : combined
        ? data.netWorthHistory
        : ((
            (investment ? data.brokeragePerformance.find(ofAccount) : undefined) ??
            data.accountBalanceHistory.find(ofAccount)
          )?.points ?? [])
  const chartValue =
    combined && data.netWorthProvisional ? (history.at(-1)?.value ?? account.value) : account.value
  const chartData =
    account.value == null
      ? []
      : buildLiveChartData(
          history,
          combined && data.netWorthProvisional ? [] : livePoints,
          chartValue ?? account.value,
          valuationTime,
        )
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

/**
 * Spending presents the saved spending card, so the Accounts "All accounts" total and its
 * history leave that card out, using Rust's card-free history. Net worth is unchanged.
 */
export function withoutSpendingCard<
  T extends Pick<FinanceSnapshot, 'accounts' | 'netWorthHistory'> &
    Partial<Pick<FinanceSnapshot, 'insights' | 'netWorth'>>,
>(data: T): T {
  const total = data.insights?.accountsTotal
  if (!total) return data
  const without = (value: number) => (cents(value) - cents(total.excludedValue)) / 100
  return {
    ...data,
    ...(data.netWorth != null && { netWorth: without(data.netWorth) }),
    accounts: data.accounts.map((account) =>
      account.id === 'all' && account.value != null
        ? { ...account, value: without(account.value) }
        : account,
    ),
    netWorthHistory: total.history,
  }
}

/** Live net worth points without the spending card, whose balance is not market-driven. */
export function withoutSpendingCardPoints<P extends { value: number }>(
  data: Partial<Pick<FinanceSnapshot, 'insights'>>,
  points: P[],
): P[] {
  const excluded = data.insights?.accountsTotal?.excludedValue
  return excluded == null
    ? points
    : points.map((point) => ({ ...point, value: (cents(point.value) - cents(excluded)) / 100 }))
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
