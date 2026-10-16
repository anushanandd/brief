import { addDays, localDateKey } from './dates'
import { type ChartPeriod } from './graph-preferences'
import { rangeDates } from './investment-range'
import type { FinanceSnapshot } from './schema'

/**
 * The investment summary's period: Rust writes every line (`summary.brief`); this only chooses
 * which period to ask for.
 */

/**
 * Where a summary of the range starts: the portfolio's reconciled period when there is one, so
 * every card on the page agrees, or else the day before the range's first day.
 */
export function summaryStart(data: FinanceSnapshot, range: ChartPeriod) {
  const windowStart = data.insights?.periods.find(({ window }) => window === range)?.start
  if (windowStart) return windowStart
  const today = data.insights?.today ?? localDateKey(Date.parse(data.updatedAt) / 1000)
  const firstDay = rangeDates(range, today).from
  return firstDay ? addDays(firstDay, -1) : undefined
}
