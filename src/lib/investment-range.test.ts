import { expect, it } from 'vitest'

import { graphWindows, chartPeriod, chartPeriodSeconds } from './graph-preferences'
import { holdingHistoryRange, rangeDates } from './investment-range'

const DAY = 86_400

it('follows the shared chart periods, each one a returns window', () => {
  expect(graphWindows.map(({ secs }) => chartPeriod(secs))).toEqual([
    'day',
    'week',
    'month',
    'quarter',
    'year',
    'all',
  ])
  expect(chartPeriod(123)).toBe('week')
  expect(chartPeriodSeconds('quarter')).toBe(90 * DAY)
  expect(chartPeriodSeconds('all')).toBe(0)
})

it('requests only history ranges the native chart supports, cropping the year for a quarter', () => {
  expect(holdingHistoryRange('day')).toBe(DAY)
  expect(holdingHistoryRange('week')).toBe(7 * DAY)
  expect(holdingHistoryRange('month')).toBe(30 * DAY)
  expect(holdingHistoryRange('quarter')).toBe(365 * DAY)
  expect(holdingHistoryRange('year')).toBe(365 * DAY)
  expect(holdingHistoryRange('all')).toBe(0)
})

it('covers the same days as the returns window, starting the day after its baseline', () => {
  expect(rangeDates('year', '2026-09-16', '2025-09-16')).toEqual({
    from: '2025-09-17',
    to: '2026-09-16',
  })
  expect(rangeDates('week', '2026-09-16')).toEqual({ from: '2026-09-10', to: '2026-09-16' })
  expect(rangeDates('all', '2026-09-16')).toEqual({ from: undefined, to: '2026-09-16' })
})
