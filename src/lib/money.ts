import type { FinanceSnapshot, MarketNewsArticle, Transaction } from './schema'
import { transactionDateKey } from './spending'
import { moneyKind } from './transaction-kind'
export { moneyKind, type MoneyKind } from './transaction-kind'

export type MoneyRange = 'week' | 'month' | 'quarter' | 'year' | 'all'

const dayMs = 86_400_000
const money = (value: number) => Math.round(value * 100) / 100
const dateKey = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`

export function moneyRangeStart(range: MoneyRange, referenceIso: string) {
  if (range === 'all') return '0000-01-01'
  const reference = new Date(`${referenceIso.slice(0, 10)}T12:00:00`)
  if (range === 'week') {
    const start = new Date(reference.getFullYear(), reference.getMonth(), reference.getDate())
    start.setDate(start.getDate() - ((start.getDay() + 6) % 7))
    return dateKey(start)
  }
  if (range === 'month') return dateKey(new Date(reference.getFullYear(), reference.getMonth(), 1))
  if (range === 'quarter')
    return dateKey(new Date(reference.getFullYear(), Math.floor(reference.getMonth() / 3) * 3, 1))
  return dateKey(new Date(reference.getFullYear(), 0, 1))
}

export function moneySummary(data: FinanceSnapshot, range: MoneyRange) {
  const start = moneyRangeStart(range, data.updatedAt)
  const end = data.updatedAt.slice(0, 10)
  const transactions = data.transactions.filter((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
    return !transaction.pending && date >= start && date <= end
  })
  const totals = {
    income: 0,
    dividends: 0,
    interest: 0,
    fees: 0,
    taxes: 0,
    reimbursements: 0,
    spending: 0,
    transfers: 0,
  }
  for (const transaction of transactions) {
    const amount = Math.abs(transaction.amount)
    switch (moneyKind(transaction)) {
      case 'income':
        if (transaction.amount > 0) totals.income += amount
        break
      case 'dividend':
        if (transaction.amount > 0) totals.dividends += amount
        break
      case 'interest':
        if (transaction.amount > 0) totals.interest += amount
        break
      case 'fee':
        totals.fees += amount
        break
      case 'tax':
        totals.taxes += amount
        break
      case 'reimbursement':
        if (transaction.amount > 0) totals.reimbursements += amount
        break
      case 'expense':
        totals.spending += amount
        break
      case 'transfer':
        totals.transfers += transaction.amount
        break
    }
  }
  const sales = data.trades.filter((trade) => {
    const date = transactionDateKey(trade.date, data.updatedAt)
    return trade.type.toLocaleUpperCase().includes('SELL') && date >= start && date <= end
  })
  const estimatedRealizedGain = sales.every((trade) => trade.estimatedRealizedGain != null)
    ? money(sales.reduce((total, trade) => total + (trade.estimatedRealizedGain ?? 0), 0))
    : null
  const grossIncome = totals.income + totals.dividends + totals.interest
  const personalSpending = Math.max(0, totals.spending - totals.reimbursements)
  const netCashFlow = grossIncome - personalSpending - totals.fees - totals.taxes
  return {
    income: money(totals.income),
    dividends: money(totals.dividends),
    interest: money(totals.interest),
    fees: money(totals.fees),
    taxes: money(totals.taxes),
    reimbursements: money(totals.reimbursements),
    spending: money(totals.spending),
    transfers: money(totals.transfers),
    estimatedRealizedGain,
    grossIncome: money(grossIncome),
    personalSpending: money(personalSpending),
    netCashFlow: money(netCashFlow),
    transactions,
    start,
    end,
  }
}

export type CashFlowBreakdownItem = {
  id: string
  label: string
  value: number
  children: CashFlowBreakdownTransaction[]
}

export type CashFlowBreakdownTransaction = {
  id: string
  label: string
  value: number
  date: string
}

function aggregateCashFlowItems(
  entries: Array<{ group: string; transaction: CashFlowBreakdownTransaction }>,
  prefix: string,
): CashFlowBreakdownItem[] {
  const groups = new Map<string, CashFlowBreakdownTransaction[]>()
  for (const { group, transaction } of entries) {
    const children = groups.get(group) ?? []
    children.push(transaction)
    groups.set(group, children)
  }
  return [...groups]
    .map(([label, children]) => ({
      id: `${prefix}:${label}`,
      label,
      value: money(children.reduce((total, child) => total + child.value, 0)),
      children: children.toSorted(
        (left, right) =>
          right.date.localeCompare(left.date) ||
          right.value - left.value ||
          left.label.localeCompare(right.label),
      ),
    }))
    .toSorted((left, right) => right.value - left.value || left.label.localeCompare(right.label))
}

export function cashFlowBreakdown(transactions: Transaction[]) {
  const sources: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  const purchases: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  let fees = 0
  let taxes = 0
  for (const transaction of transactions) {
    const kind = moneyKind(transaction)
    const amount = Math.abs(transaction.amount)
    if (
      transaction.amount > 0 &&
      (kind === 'income' ||
        kind === 'dividend' ||
        kind === 'interest' ||
        kind === 'reimbursement' ||
        kind === 'fee' ||
        kind === 'tax')
    ) {
      const source = transaction.merchant.trim() || 'Unknown source'
      sources.push({
        group: source,
        transaction: {
          id: `source-transaction:${transaction.id}`,
          label: transaction.description?.trim() || source,
          value: money(amount),
          date: (transaction.postedOn ?? transaction.date).slice(0, 10),
        },
      })
    } else if (kind === 'expense') {
      purchases.push({
        group: transaction.category.trim() || 'Other',
        transaction: {
          id: `purchase-transaction:${transaction.id}`,
          label: transaction.merchant.trim() || transaction.description?.trim() || 'Purchase',
          value: money(amount),
          date: (transaction.postedOn ?? transaction.date).slice(0, 10),
        },
      })
    } else if (kind === 'fee') {
      fees += amount
    } else if (kind === 'tax') {
      taxes += amount
    }
  }
  return {
    sources: aggregateCashFlowItems(sources, 'source'),
    purchases: aggregateCashFlowItems(purchases, 'purchase'),
    fees: money(fees),
    taxes: money(taxes),
  }
}

export type ExpectedMoneyEvent = {
  id: string
  title: string
  kind: 'income' | 'payment' | 'subscription'
  date: string
  amount: number
  confidence: 'High'
  accountId?: string
}

export const recurringMoneyKey = (transaction: Transaction) =>
  `${transaction.accountId ?? transaction.account}|${transaction.merchant}`
    .toLocaleLowerCase()
    .replace(/\d+/g, '')
    .replace(/[^a-z|]+/g, ' ')
    .trim()

const median = (values: number[]) => {
  const sorted = values.toSorted((left, right) => left - right)
  return sorted[Math.floor(sorted.length / 2)]
}

function recurringFrequency(days: number) {
  if (days >= 6 && days <= 9) return 7
  if (days >= 12 && days <= 16) return 14
  if (days >= 25 && days <= 36) return Math.round(days)
  if (days >= 80 && days <= 100) return Math.round(days)
  if (days >= 350 && days <= 380) return Math.round(days)
  return undefined
}

export function expectedMoneyEvents(data: FinanceSnapshot): ExpectedMoneyEvent[] {
  const groups = new Map<string, Transaction[]>()
  for (const transaction of data.transactions) {
    if (transaction.pending || moneyKind(transaction) === 'transfer') continue
    const key = recurringMoneyKey(transaction)
    const current = groups.get(key) ?? []
    current.push(transaction)
    groups.set(key, current)
  }
  const today = Date.parse(data.updatedAt.slice(0, 10))
  const events: ExpectedMoneyEvent[] = []
  for (const [key, values] of groups) {
    const ordered = values.toSorted((left, right) => left.date.localeCompare(right.date)).slice(-6)
    if (ordered.length < 3) continue
    const dates = ordered.map((transaction) =>
      Date.parse(transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)),
    )
    const intervals = dates.slice(1).map((date, index) => (date - dates[index]) / dayMs)
    const frequency = recurringFrequency(median(intervals))
    if (
      !frequency ||
      intervals.some((days) => Math.abs(days - frequency) > Math.max(4, frequency * 0.2))
    )
      continue
    const amounts = ordered.map(({ amount }) => Math.abs(amount))
    const typicalAmount = median(amounts)
    if (
      amounts.some((amount) => Math.abs(amount - typicalAmount) > Math.max(5, typicalAmount * 0.25))
    )
      continue
    const firstExpected = dates.at(-1)! + frequency * dayMs
    const missed = Math.max(0, Math.ceil((today - firstExpected) / (frequency * dayMs)))
    const expected = new Date(firstExpected + missed * frequency * dayMs)
    if (expected.getTime() - today > 120 * dayMs) continue
    const last = ordered.at(-1)!
    const kind = moneyKind(last)
    events.push({
      id: key,
      title: last.merchant,
      kind:
        kind === 'income' || kind === 'dividend' || kind === 'interest'
          ? 'income'
          : /subscription|streaming|membership/i.test(
                `${last.category} ${last.categoryDetail ?? ''} ${last.description ?? ''}`,
              )
            ? 'subscription'
            : 'payment',
      date: expected.toISOString().slice(0, 10),
      amount: money(last.amount > 0 ? typicalAmount : -typicalAmount),
      confidence: 'High',
      accountId: last.accountId,
    })
  }
  return events.toSorted((left, right) => left.date.localeCompare(right.date)).slice(0, 16)
}

export function scoreNewsArticle(
  article: MarketNewsArticle,
  data: Pick<FinanceSnapshot, 'holdings'>,
  now = Date.now(),
) {
  const positions = new Map<string, number>()
  let portfolio = 0
  for (const holding of data.holdings) {
    if (holding.value == null || holding.value <= 0) continue
    const symbol = holding.ticker.toLocaleUpperCase()
    positions.set(symbol, (positions.get(symbol) ?? 0) + holding.value)
    portfolio += holding.value
  }
  const exposure = article.symbols.reduce(
    (sum, symbol) => sum + (positions.get(symbol.toLocaleUpperCase()) ?? 0),
    0,
  )
  const ageHours = Math.max(0, (now - Date.parse(article.createdAt)) / 3_600_000)
  const freshness = Math.max(0, 45 - Math.min(45, ageHours * 2))
  const holdingWeight = portfolio ? Math.min(40, (exposure / portfolio) * 160) : 0
  const materialTopic =
    /earnings|guidance|acqui|merger|lawsuit|regulat|dividend|split|contract|partnership/i.test(
      article.headline,
    )
      ? 15
      : 0
  const score = Math.round(Math.min(100, freshness + holdingWeight + materialTopic))
  const reason = materialTopic
    ? 'Material company event'
    : exposure
      ? 'Matches a current holding'
      : 'Recent market story'
  return { score, reason }
}

export function chatEvidence(data: FinanceSnapshot) {
  const summary = moneySummary(data, 'month')
  return {
    savedAt: data.updatedAt,
    thisMonth: {
      income: summary.grossIncome,
      spendingAfterReimbursements: summary.personalSpending,
      fees: summary.fees,
      netCashFlow: summary.netCashFlow,
    },
    accounts: data.accounts
      .filter(({ id }) => id !== 'all')
      .map(({ name, institution, type, value }) => ({ name, institution, type, value })),
    holdings: data.holdings
      .toSorted((left, right) => (right.value ?? 0) - (left.value ?? 0))
      .slice(0, 12)
      .map(({ ticker, name, value, totalChangePct }) => ({ ticker, name, value, totalChangePct })),
    recentActivity: data.transactions.slice(0, 20).map((transaction) => ({
      merchant: transaction.merchant,
      category: transaction.category,
      date: transaction.date,
      amount: transaction.amount,
      pending: transaction.pending,
    })),
    upcoming: expectedMoneyEvents(data).slice(0, 8),
  }
}

export { isZelle } from './transaction-kind'
