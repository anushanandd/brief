import { dayKey, dayStart } from './dates'
import { cents } from './money'
import type { FinanceSnapshot, Transaction } from './schema'
import { transactionDateKey } from './spending'
import { transactionMarkKind } from './transaction-kind'

/** Posted money in and out, interest, and dividends, summed in whole cents. */
function summarize(items: Array<{ transaction: Transaction }>) {
  let moneyIn = 0
  let moneyOut = 0
  let interest = 0
  let dividends = 0
  for (const { transaction } of items) {
    const amount = cents(transaction.amount)
    if (amount > 0) moneyIn += amount
    if (amount < 0) moneyOut -= amount
    const kind = transactionMarkKind(transaction)
    if (amount > 0 && kind === 'interest') interest += amount
    if (amount > 0 && kind === 'dividend') dividends += amount
  }
  return {
    moneyIn: moneyIn / 100,
    moneyOut: moneyOut / 100,
    netFlow: (moneyIn - moneyOut) / 100,
    interest: interest / 100,
    dividends: dividends / 100,
  }
}

/**
 * An account's posted money in and out, interest, and dividends over a window ending at the
 * snapshot's day, summed in whole cents. A window of 0 seconds is all history.
 */
export function accountTransactionSummary(
  transactions: Transaction[],
  referenceIso: string,
  windowSeconds: number,
) {
  const end = transactionDateKey(referenceIso)
  const start = windowSeconds ? dayKey(dayStart(end) - windowSeconds * 1_000) : ''
  const posted = transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date)
    return !transaction.pending && date && date <= end ? [{ transaction, date }] : []
  })
  return {
    range: summarize(posted.filter(({ date }) => !start || date >= start)),
    start,
    end,
  }
}

export type PerformanceSeries = FinanceSnapshot['brokeragePerformance'][number]

/**
 * New money and market movement across `[start, end]` for one or more investment series. The
 * native running deposit total starts at the opening value, so contributions count only flows
 * after the range opens (or after tracking starts, when that is later).
 */
export function rangeInvestmentFlows(series: PerformanceSeries[], start: string, end: string) {
  let contributions: number | null = null
  let marketChange: number | null = null
  for (const { performanceMethod, points } of series) {
    if (performanceMethod === 'value-only') continue
    const deposits = points.filter(
      (point): point is typeof point & { netDeposits: number } =>
        typeof point.netDeposits === 'number' && point.date <= end,
    )
    const last = deposits.at(-1)
    const base = deposits.findLast(({ date }) => start && date < start) ?? deposits[0]
    if (last && base)
      contributions = (contributions ?? 0) + cents(last.netDeposits) - cents(base.netDeposits)
    for (const point of points) {
      if (point.marketChange == null || point.date > end || (start && point.date < start)) continue
      marketChange = (marketChange ?? 0) + cents(point.marketChange)
    }
  }
  return {
    contributions: contributions == null ? null : contributions / 100,
    marketChange: marketChange == null ? null : marketChange / 100,
  }
}

/** The sum of the known values, or null when none is known. */
export function sumKnown(values: Array<number | null | undefined>) {
  const known = values.filter((value): value is number => value != null)
  return known.length ? known.reduce((total, value) => total + value, 0) : null
}
