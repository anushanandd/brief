import { expect, it } from 'vitest'

import { seedSnapshot } from '../data/fixtures/records'
import type { PeriodAsOf } from '../lib/bindings/PeriodAsOf'
import { asOfWaiting, withPeriodAsOf } from './use-as-of'

const past = (end: string): PeriodAsOf => ({
  end,
  chartChanges: [{ series: 'net-worth', windows: { month: { change: 10, percent: 1 } } }],
  periods: [],
  flows: [],
  changes: [],
  recap: [],
})

it('shows a past day the nearest figures answered, and none before any arrive', () => {
  const data = seedSnapshot()
  expect(withPeriodAsOf(data, undefined, past('2026-08-01'))).toBe(data)

  const scrolled = withPeriodAsOf(data, '2026-08-01', past('2026-08-01'))
  expect(scrolled.insights?.chartChanges).toEqual(past('2026-08-01').chartChanges)
  // What isn't a period's figure stays as the snapshot has it, the histories included.
  expect(scrolled.insights?.rollup).toBe(data.insights?.rollup)
  expect(scrolled.brokeragePerformance).toBe(data.brokeragePerformance)

  // Mid-scroll, the day answered last stands in for the one asked next.
  const moving = withPeriodAsOf(data, '2026-08-01', past('2026-08-02')).insights
  expect(moving?.chartChanges).toEqual(past('2026-08-02').chartChanges)

  const pending = withPeriodAsOf(data, '2026-08-01', undefined).insights
  expect(pending?.chartChanges).toEqual([])
  expect(pending?.periods).toEqual([])
  expect(pending?.flows).toEqual([])
  expect(pending?.changes).toEqual([])
  expect(pending?.recap).toEqual([])
})

it('says when its figures are another day’s, or none yet, until the end day’s arrive', () => {
  const data = seedSnapshot()
  expect(asOfWaiting(withPeriodAsOf(data, undefined, undefined))).toBe(false)
  expect(asOfWaiting(withPeriodAsOf(data, '2026-08-01', past('2026-08-01')))).toBe(false)
  expect(asOfWaiting(withPeriodAsOf(data, '2026-08-01', past('2026-08-02')))).toBe(true)
  expect(asOfWaiting(withPeriodAsOf(data, '2026-08-01', undefined))).toBe(true)
  expect(asOfWaiting(withPeriodAsOf(data, '2026-08-01', null))).toBe(false)
})
