import { addDays } from './dates'
import {
  useGraphWindow,
  type ChartPeriod,
  chartPeriod,
  chartPeriodSeconds,
} from './graph-preferences'

/**
 * Investments follows the app's chart period (W, M, Q, Y, A), shared with Home and Accounts for
 * the session. Each period is one of the returns windows Rust computes.
 */
const DAY = 86_400

/** The page's period, from the chart period chosen anywhere in the app this session. */
export function useInvestmentRange() {
  const [seconds] = useGraphWindow()
  return chartPeriod(seconds)
}

/**
 * The security history to request for a range. Native history comes in fixed ranges, so three
 * months crops the year's daily bars rather than asking for new ones.
 */
export function holdingHistoryRange(range: ChartPeriod) {
  return range === 'quarter' ? 365 * DAY : chartPeriodSeconds(range)
}

/**
 * The calendar days a range covers, ending `today`, for reports over the same period. A returns
 * window's first day is its baseline, so its flows begin the day after.
 */
export function rangeDates(range: ChartPeriod, today: string, windowStart?: string) {
  if (windowStart) return { from: addDays(windowStart, 1), to: today }
  if (range === 'all') return { from: undefined, to: today }
  const days = { week: 7, month: 30, quarter: 91, year: 365 }[range]
  return { from: addDays(today, 1 - days), to: today }
}
