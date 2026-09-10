import { transactionMarkKind } from './logos'
import type { FinanceSnapshot } from './schema'
import { transactionDateKey } from './spending'

type BrokeragePerformance = FinanceSnapshot['brokeragePerformance'][number]

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

export function monthlyPortfolioChange(
  data: Pick<FinanceSnapshot, 'brokeragePerformance' | 'updatedAt'>,
) {
  const portfolio = data.brokeragePerformance.find(({ accountId }) => accountId === 'total')
  if (!portfolio) return null

  const monthStart = `${data.updatedAt.slice(0, 7)}-01`
  const datedPoints = portfolio.points.filter(({ date }) => /^\d{4}-\d{2}-\d{2}$/.test(date))
  const baseline =
    datedPoints.findLast(({ date }) => date < monthStart) ??
    datedPoints.find(({ date }) => date >= monthStart) ??
    portfolio.points.at(-2)

  return baseline?.value
    ? ((portfolio.currentValue - baseline.value) / Math.abs(baseline.value)) * 100
    : null
}

export function currentMonthIncome(data: Pick<FinanceSnapshot, 'transactions' | 'updatedAt'>) {
  const month = data.updatedAt.slice(0, 7)
  return data.transactions
    .filter(
      (transaction) =>
        !transaction.pending &&
        transaction.amount > 0 &&
        transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt).startsWith(
          month,
        ) &&
        ['income', 'interest', 'dividend'].includes(transactionMarkKind(transaction)),
    )
    .reduce((total, { amount }) => total + amount, 0)
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
