import { buildNetWorthHistory, hasRecordedNetWorthTrend } from './history'
import { buildBrokeragePerformance, type BenchmarkPoint } from './performance'
import type { FinanceSnapshot, MarketSnapshot } from './schema'

export type PlaidData = {
  accounts: Array<Record<string, any>>
  transactions: Array<Record<string, any>>
  investmentAccounts?: Array<Record<string, any>>
  holdings?: Array<Record<string, any>>
  securities?: Array<Record<string, any>>
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
const SPENDING_COLORS = ['#477d75', '#4f78a8', '#b58a3f', '#7d6da5', '#a45f79', '#79924b']
export const spendingCategoryColor = (name: string, index: number) =>
  /food|dining/.test(name.toLocaleLowerCase())
    ? '#c9684b'
    : SPENDING_COLORS[index % SPENDING_COLORS.length]
const number = (value: unknown) => {
  const parsed = typeof value === 'number' ? value : Number(value ?? 0)
  return Number.isFinite(parsed) ? parsed : 0
}
const round = (value: number) => Math.round(value * 100) / 100
const percent = (change: number, basis: number) =>
  basis ? round((change / Math.abs(basis)) * 100) : 0
export function applyMarketSnapshots(
  snapshot: FinanceSnapshot,
  market: Record<string, MarketSnapshot>,
): FinanceSnapshot {
  const accountDeltas = new Map<string, number>()
  const holdings = snapshot.holdings.map((holding) => {
    const quote = market[holding.ticker.trim().toUpperCase()]
    if (!quote) return holding
    const value = round(holding.shares * quote.price)
    accountDeltas.set(
      holding.accountId,
      round((accountDeltas.get(holding.accountId) ?? 0) + value - holding.value),
    )
    return {
      ...holding,
      price: quote.price,
      value,
      dailyChangePct: quote.dailyChangePct,
      totalChangePct: percent(value - holding.costBasis, holding.costBasis),
    }
  })
  const totalDelta = round([...accountDeltas.values()].reduce((sum, value) => sum + value, 0))
  const adjustLastPoint = <T extends { value: number }>(points: T[], delta: number) =>
    points.map((point, index) =>
      index === points.length - 1 ? { ...point, value: round(point.value + delta) } : point,
    )
  return {
    ...snapshot,
    netWorth: round(snapshot.netWorth + totalDelta),
    accounts: snapshot.accounts.map((account) => ({
      ...account,
      value: round(
        account.value + (account.id === 'all' ? totalDelta : (accountDeltas.get(account.id) ?? 0)),
      ),
    })),
    netWorthHistory: adjustLastPoint(snapshot.netWorthHistory, totalDelta),
    brokeragePerformance: snapshot.brokeragePerformance.map((account) => {
      const delta =
        account.accountId === 'total' ? totalDelta : (accountDeltas.get(account.accountId) ?? 0)
      return {
        ...account,
        currentValue: round(account.currentValue + delta),
        points: adjustLastPoint(account.points, delta),
      }
    }),
    holdings,
  }
}

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
  const plaidInvestmentSourceAccounts = (plaid.investmentAccounts ?? []).filter(
    (account) =>
      String(account.subtype ?? '')
        .trim()
        .toLocaleLowerCase() === 'stock plan',
  )
  const plaidInvestmentAccountIds = new Set(
    plaidInvestmentSourceAccounts.map((account) => String(account.account_id ?? '')),
  )
  const plaidHoldings = (plaid.holdings ?? []).filter((holding) =>
    plaidInvestmentAccountIds.has(String(holding.account_id ?? '')),
  )
  const plaidHoldingValues = new Map<string, number>()
  for (const holding of plaidHoldings) {
    const accountId = String(holding.account_id ?? '')
    plaidHoldingValues.set(
      accountId,
      round(
        (plaidHoldingValues.get(accountId) ?? 0) +
          number(holding.institution_value ?? holding.value),
      ),
    )
  }
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
  const plaidInvestmentAccounts = plaidInvestmentSourceAccounts.map((account) => {
    const subtype = String(account.subtype ?? '').toLocaleLowerCase()
    return {
      id: `plaid:${account.account_id}`,
      name: String(account.name ?? 'Investment account'),
      institution: String(account.institution_name ?? 'Unknown institution'),
      type: /401|403|457|ira|roth|retirement|pension|profit sharing|thrift/.test(subtype)
        ? 'retirement'
        : 'brokerage',
      value: round(
        number(
          account.balances?.current ??
            account.balances?.available ??
            plaidHoldingValues.get(String(account.account_id ?? '')),
        ),
      ),
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
  const accountValues = [...plaidAccounts, ...plaidInvestmentAccounts, ...snapAccounts]
  const netWorth = round(accountValues.reduce((sum, account) => sum + account.value, 0))
  const accounts: FinanceSnapshot['accounts'] = [
    { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: netWorth },
    ...accountValues,
  ]

  const holdings: FinanceSnapshot['holdings'] = []
  const plaidSecurities = new Map(
    (plaid.securities ?? []).map((security) => [String(security.security_id ?? ''), security]),
  )
  for (const position of plaidHoldings) {
    const security = plaidSecurities.get(String(position.security_id ?? '')) ?? {}
    const shares = number(position.quantity)
    const price = number(
      position.institution_price ??
        security.close_price ??
        (shares ? number(position.institution_value) / shares : 0),
    )
    const value = number(position.institution_value ?? shares * price)
    const costBasis = number(position.cost_basis)
    holdings.push({
      ticker: String(security.ticker_symbol ?? security.name ?? '—'),
      name: String(security.name ?? security.ticker_symbol ?? 'Investment position'),
      accountId: `plaid:${position.account_id}`,
      shares,
      price,
      value: round(value),
      costBasis: round(costBasis),
      dailyChangePct: 0,
      totalChangePct: costBasis ? percent(value - costBasis, costBasis) : 0,
      color: COLORS[holdings.length % COLORS.length],
    })
  }
  for (const account of snaptrade.accounts) {
    for (const position of snaptrade.positions[account.id] ?? []) {
      const shares = number(position.units)
      const price = number(position.price)
      const costBasis = number(position.cost_basis)
      const value = shares * price
      const label = instrumentLabel(position.instrument ?? {})
      holdings.push({
        ticker: label.ticker,
        name: label.name,
        accountId: `snaptrade:${account.id}`,
        shares,
        price,
        value: round(value),
        costBasis: round(shares * costBasis),
        dailyChangePct: 0,
        totalChangePct: costBasis ? percent(price - costBasis, costBasis) : 0,
        color: COLORS[holdings.length % COLORS.length],
      })
    }
  }

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
  const investmentActivities = snaptrade.accounts.flatMap((account) => {
    const accountId = `snaptrade:${String(account.id ?? '')}`
    const accountName = String(account.name ?? account.raw_type ?? 'Brokerage account')
    return (snaptrade.activities[String(account.id ?? '')] ?? []).flatMap((activity) => {
      const type = String(activity.type ?? '')
        .trim()
        .toLocaleUpperCase()
        .replaceAll(/[^A-Z0-9]+/g, '_')
      if (
        type !== 'SELL' &&
        type !== 'WITHDRAWAL' &&
        type !== 'CONTRIBUTION' &&
        type !== 'DEPOSIT' &&
        type !== 'TRANSFER' &&
        type !== 'CASH_TRANSFER' &&
        !type.endsWith('_TRANSFER_IN') &&
        !type.endsWith('_TRANSFER_OUT')
      ) {
        return []
      }
      const date = String(activity.trade_date ?? activity.settlement_date ?? '').slice(0, 10)
      if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return []
      const rawAmount = number(activity.amount)
      const amount =
        type === 'SELL' ||
        type === 'CONTRIBUTION' ||
        type === 'DEPOSIT' ||
        type.endsWith('_TRANSFER_IN')
          ? Math.abs(rawAmount)
          : type === 'WITHDRAWAL' || type.endsWith('_TRANSFER_OUT')
            ? -Math.abs(rawAmount)
            : rawAmount
      const rawSymbol = activity.symbol?.raw_symbol ?? activity.symbol?.symbol
      return [
        {
          accountId,
          accountName,
          date,
          type,
          amount: round(amount),
          description: String(activity.description ?? type.replaceAll('_', ' ')),
          ...(rawSymbol ? { symbol: String(rawSymbol) } : {}),
        },
      ]
    })
  })

  const currentMonth = now.toISOString().slice(0, 7)
  const expenseTotal = (month: string) =>
    transactions
      .filter((transaction) => transaction.date.startsWith(month) && transaction.amount < 0)
      .reduce((sum, transaction) => sum + Math.abs(transaction.amount), 0)
  const monthTotal = expenseTotal(currentMonth)
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
      color: spendingCategoryColor(name, index),
    }))

  const historyWasEstimated = !hasRecordedNetWorthTrend(previousHistory)
  const netWorthHistory = buildNetWorthHistory(previousHistory, transactions, netWorth, now)
  const brokeragePerformance = buildBrokeragePerformance(
    {
      accounts: [
        ...snaptrade.accounts,
        ...plaidInvestmentAccounts.map((account) => ({
          id: account.id,
          accountId: account.id,
          name: account.name,
          institution_name: account.institution,
          balance: { total: { amount: account.value } },
        })),
      ],
      activities: snaptrade.activities,
      balanceHistory: snaptrade.balanceHistory,
    },
    benchmarkHistory,
    now,
  )
  return {
    updatedAt: now.toISOString(),
    netWorth,
    netWorthHistoryEstimated: previousHistoryEstimated || historyWasEstimated,
    benchmarkHistory,
    brokeragePerformance,
    accounts,
    netWorthHistory,
    holdings,
    spending: {
      monthTotal: round(monthTotal),
      categories,
    },
    transactions,
    investmentActivities,
    providers: [
      {
        id: 'plaid',
        name: 'Plaid',
        description: 'Bank and credit card accounts',
        status: plaid.accounts.length ? 'ready' : 'error',
        lastSync: plaid.accounts.length ? 'Just now' : 'Not connected',
      },
      {
        id: 'plaid-investments',
        name: 'Plaid Investments',
        description: 'Brokerage and stock plan accounts',
        status: plaidInvestmentAccounts.length ? 'ready' : 'error',
        lastSync: plaidInvestmentAccounts.length ? 'Just now' : 'Not connected',
      },
      {
        id: 'snaptrade',
        name: 'SnapTrade',
        description: 'Investment accounts',
        status: snaptrade.accounts.length ? 'ready' : 'error',
        lastSync: snaptrade.accounts.length ? 'Just now' : 'Not connected',
      },
      {
        id: 'alpaca',
        name: 'Alpaca',
        description: 'Market quotes',
        status: 'error',
        lastSync: 'Not configured',
      },
      {
        id: 'logos',
        name: 'Logo.dev',
        description: 'Company and merchant logos',
        status: 'ready',
        lastSync: 'On demand',
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
