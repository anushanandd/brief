import type { Transaction } from './schema'

export type NetWorthHistoryPoint = { date: string; value: number }

const isoDate = /^\d{4}-\d{2}-\d{2}$/
const round = (value: number) => Math.round(value * 100) / 100

function utcDate(date: Date, offset: number) {
  const value = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()))
  value.setUTCDate(value.getUTCDate() - offset)
  return value.toISOString().slice(0, 10)
}

export function hasRecordedNetWorthTrend(history: NetWorthHistoryPoint[]) {
  return history.filter((point) => point.date !== '—' && Number.isFinite(point.value)).length > 1
}

/**
 * Gives a newly connected account a useful, clearly labeled starting trend. Posted Plaid
 * transactions are signed as their effect on net worth, so walking them backward from today's
 * balance reconstructs the cash and credit portion without fabricating market returns.
 */
export function buildNetWorthHistory(
  history: NetWorthHistoryPoint[],
  transactions: Transaction[],
  currentNetWorth: number,
  now: Date,
  days = 90,
): NetWorthHistoryPoint[] {
  const recorded = history.filter((point) => point.date !== '—' && Number.isFinite(point.value))

  if (recorded.length > 1) {
    if (!recorded.every((point) => isoDate.test(point.date))) return recorded.slice(-180)

    const today = utcDate(now, 0)
    return [
      ...recorded
        .toSorted((a, b) => a.date.localeCompare(b.date))
        .filter((point) => point.date !== today),
      { date: today, value: round(currentNetWorth) },
    ].slice(-180)
  }

  const flowByDate = new Map<string, number>()
  for (const transaction of transactions) {
    if (transaction.pending || !isoDate.test(transaction.date)) continue
    flowByDate.set(
      transaction.date,
      round((flowByDate.get(transaction.date) ?? 0) + transaction.amount),
    )
  }

  let estimatedValue = currentNetWorth
  const result: NetWorthHistoryPoint[] = []
  for (let offset = 0; offset < days; offset += 1) {
    const date = utcDate(now, offset)
    result.push({ date, value: round(estimatedValue) })
    estimatedValue = round(estimatedValue - (flowByDate.get(date) ?? 0))
  }
  return result.toReversed()
}
