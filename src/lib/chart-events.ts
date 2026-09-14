import { formatCurrency, formatPercent } from './format'
import { transactionMarkKind } from './logos'
import type { FinanceSnapshot } from './schema'
import { formatActivityDate, isSpendingTransaction, transactionDateKey } from './spending'

const DAY_SECONDS = 24 * 60 * 60

export type ChartEventKind = 'income' | 'expense' | 'transfer' | 'buy' | 'sale'

export type ChartEvent = {
  id: string
  date: string
  kind: ChartEventKind
  title: string
  amount?: number
  profitLossPct?: number
  direction: 'in' | 'out'
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
  windowSeconds: number,
): ChartEventGroup[] {
  const relevantAccounts = chartAccountIds(data, accountId)
  const includesAccount = (id?: string) => !relevantAccounts || (!!id && relevantAccounts.has(id))
  const transactionThreshold = Math.max(500, Math.abs(data.spending.monthTotal) * 0.1)
  const transferThreshold = Math.max(1_000, Math.abs(data.netWorth) * 0.005)
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
      direction: transaction.amount > 0 ? 'in' : 'out',
      accountId: transaction.accountId,
      score: amount / threshold,
    })
  }

  for (const trade of data.trades) {
    const type = trade.type.toLocaleUpperCase()
    const direction = type.includes('SELL') ? 'out' : type.includes('BUY') ? 'in' : undefined
    if (!includesAccount(trade.accountId) || !direction) continue
    events.push({
      id: `trade:${trade.id}`,
      date: trade.date,
      kind: direction === 'out' ? 'sale' : 'buy',
      title: `${direction === 'out' ? 'Sold' : 'Bought'} ${trade.ticker ?? 'security'}`,
      amount: Math.abs(trade.amount),
      profitLossPct:
        direction === 'out' ? (trade.estimatedRealizedGainPct ?? undefined) : undefined,
      direction,
      accountId: trade.accountId,
      score: 1 + Math.abs(trade.amount) / 1_000,
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
        direction: 'in',
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
    const value = event.amount == null ? '' : formatCurrency(event.amount)
    const profitLoss = event.profitLossPct == null ? '' : ` · ${formatPercent(event.profitLossPct)}`
    return `${event.title}${value ? ` ${value}` : ''}${profitLoss}${event.note ? ` · ${event.note}` : ''}`
  })
  const remainder = group.events.length - details.length
  return `${formatActivityDate(group.date, referenceIso)} · ${details.join(' · ')}${remainder ? ` · +${remainder} more` : ''}`
}
