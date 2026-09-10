import { formatCurrency, formatPercent } from './format'
import { holdingImpact } from './insights'
import { transactionMarkKind } from './logos'
import type { FinanceSnapshot } from './schema'
import { formatActivityDate, isSpendingTransaction, transactionDateKey } from './spending'

const DAY_SECONDS = 24 * 60 * 60

export type ChartEventKind =
  | 'income'
  | 'expense'
  | 'transfer'
  | 'sale'
  | 'market-move'
  | 'stock-move'

export type ChartEvent = {
  id: string
  date: string
  kind: ChartEventKind
  title: string
  amount?: number
  changePct?: number
  provenance: 'reported' | 'estimated'
  accountId?: string
  note?: string
  score: number
}

export type ChartEventGroup = {
  id: string
  date: string
  events: ChartEvent[]
  score: number
}

type PerformancePoint = {
  date: string
  value: number
  netDeposits?: number | null
  marketChange?: number | null
  marketChangePct?: number | null
}

function chartAccountIds(data: FinanceSnapshot, accountId: string) {
  if (accountId === 'net-worth') return undefined
  if (accountId === 'total') {
    return new Set(
      data.brokeragePerformance
        .map((performance) => performance.accountId)
        .filter((id) => id !== 'total'),
    )
  }
  return new Set([
    accountId,
    ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
      snaptradeId === accountId ? [plaidId] : [],
    ),
  ])
}

function eventLimit(windowSeconds: number) {
  if (!windowSeconds) return 12
  if (windowSeconds <= 7 * DAY_SECONDS) return 7
  if (windowSeconds <= 31 * DAY_SECONDS) return 10
  return 12
}

export function buildChartEventGroups(
  data: FinanceSnapshot,
  accountId: string,
  points: PerformancePoint[],
  windowSeconds: number,
): ChartEventGroup[] {
  const relevantAccounts = chartAccountIds(data, accountId)
  const includesAccount = (id?: string) => !relevantAccounts || (!!id && relevantAccounts.has(id))
  const accountValue =
    data.accounts.find(({ id }) => id === accountId)?.value ??
    data.brokeragePerformance.find(({ accountId: id }) => id === accountId)?.currentValue ??
    data.netWorth
  const transactionThreshold = Math.max(500, Math.abs(data.spending.monthTotal) * 0.1)
  const transferThreshold = Math.max(1_000, Math.abs(data.netWorth) * 0.005)
  const tradeThreshold = Math.max(1_000, Math.abs(accountValue ?? 0) * 0.005)
  const movementThreshold = Math.max(500, Math.abs(accountValue ?? data.netWorth) * 0.0025)
  const events: ChartEvent[] = []

  for (const transaction of data.transactions) {
    if (transaction.pending || !includesAccount(transaction.accountId)) continue
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
    if (!date) continue
    const mark = transactionMarkKind(transaction)
    const amount = Math.abs(transaction.amount)
    const kind =
      ['income', 'interest', 'dividend'].includes(mark) && transaction.amount > 0
        ? 'income'
        : (mark === 'transfer' || mark === 'payment') && amount >= transferThreshold
          ? 'transfer'
          : isSpendingTransaction(transaction) && amount >= transactionThreshold
            ? 'expense'
            : undefined
    if (!kind || (kind === 'income' && amount < 500)) continue
    const threshold = kind === 'transfer' ? transferThreshold : transactionThreshold
    events.push({
      id: `transaction:${transaction.id}`,
      date,
      kind,
      title: transaction.merchant,
      amount: transaction.amount,
      provenance: 'reported',
      accountId: transaction.accountId,
      score: amount / threshold,
    })
  }

  for (const trade of data.trades) {
    if (
      !includesAccount(trade.accountId) ||
      !trade.type.toLocaleUpperCase().includes('SELL') ||
      Math.abs(trade.amount) < tradeThreshold
    ) {
      continue
    }
    events.push({
      id: `trade:${trade.id}`,
      date: trade.date,
      kind: 'sale',
      title: `Sold ${trade.ticker ?? 'security'}`,
      amount: Math.abs(trade.amount),
      provenance: 'reported',
      accountId: trade.accountId,
      score: Math.abs(trade.amount) / tradeThreshold,
    })
  }

  for (const event of events) {
    if (event.kind !== 'transfer' || (event.amount ?? 0) >= 0 || !event.accountId) continue
    const transferTime = Date.parse(`${event.date}T12:00:00Z`)
    const nearbySale = events.find(
      (candidate) =>
        candidate.kind === 'sale' &&
        candidate.accountId === event.accountId &&
        transferTime >= Date.parse(`${candidate.date}T12:00:00Z`) &&
        transferTime - Date.parse(`${candidate.date}T12:00:00Z`) <= 5 * DAY_SECONDS * 1_000,
    )
    if (nearbySale) event.note = `Near ${nearbySale.title.toLocaleLowerCase()}`
  }

  if (accountId !== 'net-worth') {
    for (const current of points) {
      const marketChange = current.marketChange
      if (marketChange == null) continue
      const changePct = current.marketChangePct ?? 0
      if (Math.abs(changePct) < 4 && Math.abs(marketChange) < movementThreshold) continue
      events.push({
        id: `market:${accountId}:${current.date}`,
        date: current.date,
        kind: 'market-move',
        title: 'Portfolio movement',
        amount: marketChange,
        changePct,
        provenance: 'estimated',
        score: Math.max(Math.abs(changePct) / 4, Math.abs(marketChange) / movementThreshold),
      })
    }
  }

  const minimumImpact = Math.max(100, Math.abs(data.netWorth) * 0.001)
  for (const holding of data.holdings) {
    if (!includesAccount(holding.accountId) || holding.dailyChangePct == null) continue
    const impact = holdingImpact(holding.value, holding.dailyChangePct)
    if (
      Math.abs(holding.dailyChangePct) < 8 &&
      (Math.abs(holding.dailyChangePct) < 4 || Math.abs(impact) < minimumImpact)
    ) {
      continue
    }
    events.push({
      id: `stock:${holding.accountId}:${holding.ticker}:${data.updatedAt.slice(0, 10)}`,
      date: data.updatedAt.slice(0, 10),
      kind: 'stock-move',
      title: `${holding.ticker} moved`,
      amount: impact,
      changePct: holding.dailyChangePct,
      provenance: 'estimated',
      score: Math.max(Math.abs(holding.dailyChangePct) / 4, Math.abs(impact) / minimumImpact),
    })
  }

  const pairedTransfers = new Set<string>()
  const internalTransfers: ChartEvent[] = []
  if (!relevantAccounts) {
    const transfers = events.filter(({ kind }) => kind === 'transfer')
    for (const outgoing of transfers.filter(({ amount }) => (amount ?? 0) < 0)) {
      const outgoingTime = Date.parse(`${outgoing.date}T12:00:00Z`)
      const incoming = transfers.find(
        (candidate) =>
          (candidate.amount ?? 0) > 0 &&
          candidate.accountId !== outgoing.accountId &&
          !pairedTransfers.has(candidate.id) &&
          Math.abs((candidate.amount ?? 0) + (outgoing.amount ?? 0)) < 0.01 &&
          Math.abs(Date.parse(`${candidate.date}T12:00:00Z`) - outgoingTime) <=
            3 * DAY_SECONDS * 1_000,
      )
      if (!incoming) continue
      pairedTransfers.add(outgoing.id)
      pairedTransfers.add(incoming.id)
      internalTransfers.push({
        id: `internal-transfer:${incoming.id}:${outgoing.id}`,
        date: incoming.date > outgoing.date ? incoming.date : outgoing.date,
        kind: 'transfer',
        title: 'Internal transfer',
        amount: Math.abs(incoming.amount ?? 0),
        provenance: 'reported',
        score: Math.max(incoming.score, outgoing.score),
      })
    }
  }

  const displayEvents = [
    ...events.filter(({ id }) => !pairedTransfers.has(id)),
    ...internalTransfers,
  ]
  const end = Date.parse(`${data.updatedAt.slice(0, 10)}T23:59:59Z`)
  const start = windowSeconds ? end - windowSeconds * 1_000 : Number.NEGATIVE_INFINITY
  const grouped = new Map<string, ChartEvent[]>()
  for (const event of displayEvents) {
    const time = Date.parse(`${event.date}T12:00:00Z`)
    if (!Number.isFinite(time) || time < start || time > end) continue
    const existing = grouped.get(event.date)
    if (existing) existing.push(event)
    else grouped.set(event.date, [event])
  }

  return [...grouped]
    .map(([date, groupedEvents]) => ({
      id: `events:${date}`,
      date,
      events: groupedEvents.toSorted((left, right) => right.score - left.score),
      score: Math.max(...groupedEvents.map(({ score }) => score)),
    }))
    .toSorted((left, right) => right.score - left.score)
    .slice(0, eventLimit(windowSeconds))
    .toSorted((left, right) => left.date.localeCompare(right.date))
}

export function chartEventGroupLabel(group: ChartEventGroup, referenceIso: string) {
  const details = group.events.slice(0, 3).map((event) => {
    const value =
      event.changePct == null
        ? event.amount == null
          ? ''
          : formatCurrency(event.amount)
        : `${formatPercent(event.changePct)} · ${formatCurrency(event.amount)}`
    return `${event.title}${value ? ` ${value}` : ''}${event.note ? ` · ${event.note}` : ''}${event.provenance === 'estimated' ? ' · estimated' : ''}`
  })
  const remainder = group.events.length - details.length
  return `${formatActivityDate(group.date, referenceIso)} · ${details.join(' · ')}${remainder ? ` · +${remainder} more` : ''}`
}
