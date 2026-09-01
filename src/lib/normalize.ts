import { buildNetWorthHistory, hasRecordedNetWorthTrend } from './history'
import { buildBrokeragePerformance, type BenchmarkPoint } from './performance'
import type { FinanceSnapshot } from './schema'

export type PlaidData = {
  accounts: Array<Record<string, any>>
  transactions: Array<Record<string, any>>
  ignoredAccounts: string[]
}

export type SnapTradeData = {
  accounts: Array<Record<string, any>>
  positions: Record<string, Array<Record<string, any>>>
  activities: Record<string, Array<Record<string, any>>>
  balanceHistory?: Record<string, Array<Record<string, any>>>
  ignoredAccounts: string[]
}

export type ProviderSyncPayload = {
  plaid: PlaidData
  snaptrade: SnapTradeData
  netWorthHistory: Array<{ date: string; value: number }>
  netWorthHistoryEstimated?: boolean
  benchmarkHistory?: BenchmarkPoint[]
}

const COLORS = ['#232424', '#71877c', '#b87543', '#9d9588', '#6f7680', '#a39b8e']
const number = (value: unknown) => {
  const parsed = typeof value === 'number' ? value : Number(value ?? 0)
  return Number.isFinite(parsed) ? parsed : 0
}
const round = (value: number) => Math.round(value * 100) / 100
const percent = (change: number, basis: number) =>
  basis ? round((change / Math.abs(basis)) * 100) : 0

function instrumentLabel(instrument: Record<string, any>) {
  const underlying = instrument.underlying ?? {}
  return {
    ticker:
      instrument.raw_symbol ??
      instrument.symbol ??
      underlying.raw_symbol ??
      underlying.symbol ??
      '—',
    name:
      instrument.description ??
      underlying.description ??
      instrument.symbol ??
      'Investment position',
    kind: instrument.kind ?? 'other',
  }
}

function categoryName(transaction: Record<string, any>): string {
  const value =
    transaction.personal_finance_category?.primary ?? transaction.category?.[0] ?? 'Other'
  return String(value)
    .toLocaleLowerCase()
    .split('_')
    .map((part) => part.charAt(0).toLocaleUpperCase() + part.slice(1))
    .join(' ')
}

export function normalizeSnapshot(
  plaid: PlaidData,
  snaptrade: SnapTradeData,
  previousHistory: Array<{ date: string; value: number }>,
  now = new Date(),
  previousHistoryEstimated = false,
  benchmarkHistory: BenchmarkPoint[] = [],
): FinanceSnapshot {
  const plaidAccounts = plaid.accounts.map((account) => {
    const isCredit = account.type === 'credit'
    const rawValue = number(account.balances?.current ?? account.balances?.available)
    return {
      id: `plaid:${account.account_id}`,
      name: String(account.name ?? 'Account'),
      institution: String(account.institution_name ?? 'Unknown institution'),
      type: isCredit ? 'credit' : 'cash',
      value: round(isCredit ? -Math.abs(rawValue) : rawValue),
    }
  })
  const snapAccounts = snaptrade.accounts.map((account) => ({
    id: `snaptrade:${account.id}`,
    name: String(account.name ?? account.raw_type ?? 'Investment account'),
    institution: String(account.institution_name ?? 'Unknown institution'),
    type: `${account.name ?? ''} ${account.raw_type ?? ''}`.toLocaleLowerCase().includes('roth')
      ? 'retirement'
      : 'brokerage',
    value: round(number(account.balance?.total?.amount)),
  }))
  const accountValues = [...plaidAccounts, ...snapAccounts]
  const netWorth = round(accountValues.reduce((sum, account) => sum + account.value, 0))
  const accounts: FinanceSnapshot['accounts'] = [
    { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: netWorth },
    ...accountValues,
  ]

  const holdings: FinanceSnapshot['holdings'] = []
  let costBasisTotal = 0
  for (const account of snaptrade.accounts) {
    for (const position of snaptrade.positions[account.id] ?? []) {
      const shares = number(position.units)
      const price = number(position.price)
      const costBasis = number(position.cost_basis)
      const value = shares * price
      const label = instrumentLabel(position.instrument ?? {})
      costBasisTotal += shares * costBasis
      holdings.push({
        ticker: label.ticker,
        name: label.name,
        accountId: `snaptrade:${account.id}`,
        shares,
        price,
        value: round(value),
        dailyChangePct: 0,
        totalChangePct: costBasis ? percent(price - costBasis, costBasis) : 0,
        afterHoursPrice: price,
        color: COLORS[holdings.length % COLORS.length],
        summary: '',
      })
    }
  }

  const allocationMap = new Map<string, number>()
  for (const account of snaptrade.accounts) {
    for (const position of snaptrade.positions[account.id] ?? []) {
      const kind = instrumentLabel(position.instrument ?? {}).kind
      const label =
        kind === 'stock' || kind === 'adr'
          ? 'Stocks'
          : kind === 'etf' || kind === 'mutualfund' || kind === 'cef'
            ? 'Funds'
            : kind === 'crypto'
              ? 'Crypto'
              : kind === 'option' || kind === 'future'
                ? 'Derivatives'
                : 'Other'
      allocationMap.set(
        label,
        (allocationMap.get(label) ?? 0) + number(position.units) * number(position.price),
      )
    }
  }
  const allocationTotal = [...allocationMap.values()].reduce((sum, value) => sum + value, 0)
  const allocation = [...allocationMap.entries()].map(([name, value], index) => ({
    name,
    value: round(value),
    percent: percent(value, allocationTotal),
    color: COLORS[index % COLORS.length],
  }))

  const dividendMonths = new Map<string, number>()
  for (let offset = 11; offset >= 0; offset -= 1) {
    const date = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - offset, 1))
    dividendMonths.set(date.toISOString().slice(0, 7), 0)
  }
  for (const activities of Object.values(snaptrade.activities)) {
    for (const activity of activities) {
      if (!['DIVIDEND', 'SUBSTITUTE_DIVIDEND'].includes(activity.type ?? '')) continue
      const month = activity.trade_date?.slice(0, 7)
      if (month && dividendMonths.has(month)) {
        dividendMonths.set(month, (dividendMonths.get(month) ?? 0) + number(activity.amount))
      }
    }
  }
  const dividends = [...dividendMonths].map(([month, value]) => ({
    month: new Date(`${month}-01T00:00:00Z`).toLocaleString('en-US', {
      month: 'short',
      timeZone: 'UTC',
    }),
    value: round(value),
  }))

  const accountNames = new Map(plaid.accounts.map((account) => [account.account_id, account.name]))
  const transactions = plaid.transactions
    .map((transaction) => {
      const counterparty = transaction.counterparties?.find(
        (value: Record<string, any>) => value.type === 'merchant',
      )
      const logoUrl = transaction.logo_url ?? counterparty?.logo_url
      const website = transaction.website ?? counterparty?.website
      const logoName = transaction.merchant_name ?? counterparty?.name
      return {
        id: String(transaction.transaction_id),
        merchant: String(logoName ?? transaction.name ?? 'Transaction'),
        category: categoryName(transaction),
        date: String(
          transaction.authorized_datetime?.slice(0, 10) ??
            transaction.authorized_date ??
            transaction.datetime?.slice(0, 10) ??
            transaction.date,
        ),
        amount: round(-number(transaction.amount)),
        account: String(accountNames.get(transaction.account_id) ?? 'Account'),
        pending: Boolean(transaction.pending),
        ...(logoUrl ? { logoUrl: String(logoUrl) } : {}),
        ...(website ? { website: String(website) } : {}),
        ...(logoName ? { logoName: String(logoName) } : {}),
      }
    })
    .toSorted((a, b) => b.date.localeCompare(a.date))

  const currentMonth = now.toISOString().slice(0, 7)
  const priorDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - 1, 1))
  const priorMonth = priorDate.toISOString().slice(0, 7)
  const expenseTotal = (month: string) =>
    transactions
      .filter((transaction) => transaction.date.startsWith(month) && transaction.amount < 0)
      .reduce((sum, transaction) => sum + Math.abs(transaction.amount), 0)
  const monthTotal = expenseTotal(currentMonth)
  const priorMonthTotal = expenseTotal(priorMonth)
  const categoryMap = new Map<string, number>()
  for (const transaction of transactions) {
    if (transaction.date.startsWith(currentMonth) && transaction.amount < 0) {
      categoryMap.set(
        transaction.category,
        (categoryMap.get(transaction.category) ?? 0) + Math.abs(transaction.amount),
      )
    }
  }
  const categories = [...categoryMap.entries()]
    .toSorted((a, b) => b[1] - a[1])
    .map(([name, value], index) => ({
      name,
      value: round(value),
      percent: percent(value, monthTotal),
      color: COLORS[index % COLORS.length],
    }))

  const historyWasEstimated = !hasRecordedNetWorthTrend(previousHistory)
  const netWorthHistory = buildNetWorthHistory(previousHistory, transactions, netWorth, now)
  const brokeragePerformance = buildBrokeragePerformance(snaptrade, benchmarkHistory, now)
  const priorNetWorth = netWorthHistory.at(-2)?.value ?? netWorth
  const netWorthChange = round(netWorth - priorNetWorth)
  const investedAssets = round(snapAccounts.reduce((sum, account) => sum + account.value, 0))
  const cash = round(
    plaidAccounts
      .filter((account) => account.type === 'cash')
      .reduce((sum, account) => sum + account.value, 0),
  )
  const debt = round(
    Math.abs(
      plaidAccounts
        .filter((account) => account.type === 'credit')
        .reduce((sum, account) => sum + account.value, 0),
    ),
  )
  const statementBalance = debt
  const totalReturn = round(
    holdings.reduce((sum, holding) => sum + holding.value, 0) - costBasisTotal,
  )

  return {
    updatedAt: now.toISOString(),
    currency: 'USD',
    netWorth,
    netWorthChange,
    netWorthChangePct: percent(netWorthChange, priorNetWorth),
    netWorthHistoryEstimated: previousHistoryEstimated || historyWasEstimated,
    benchmarkHistory,
    brokeragePerformance,
    investedAssets,
    cash,
    debt,
    totalReturn,
    totalReturnPct: percent(totalReturn, costBasisTotal),
    accounts,
    netWorthHistory,
    holdings,
    allocation,
    dividends,
    spending: {
      statementBalance,
      statementDueDate: 'Unavailable',
      monthTotal: round(monthTotal),
      monthChangePct: percent(monthTotal - priorMonthTotal, priorMonthTotal),
      categories,
    },
    transactions,
    credits: [],
    providers: [
      {
        id: 'plaid',
        name: 'Plaid',
        description: 'Bank and credit card accounts',
        status: plaid.accounts.length ? 'ready' : 'error',
        lastSync: plaid.accounts.length ? 'Just now' : 'Not connected',
      },
      {
        id: 'snaptrade',
        name: 'SnapTrade',
        description: 'Investment accounts',
        status: snaptrade.accounts.length ? 'ready' : 'error',
        lastSync: snaptrade.accounts.length ? 'Just now' : 'Not connected',
      },
      {
        id: 'local',
        name: 'Local cache',
        description: 'On-device financial snapshot',
        status: 'local',
        lastSync: 'Current',
      },
    ],
  }
}
