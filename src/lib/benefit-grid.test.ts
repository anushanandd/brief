import { expect, it } from 'vitest'

import { benefitYearTotals, capturedForOthers } from './benefit-grid'
import type { BenefitActivity, BenefitHistory, BenefitPeriod } from './schema'

const credit = (id: string, amount: number, pending = false): BenefitActivity => ({
  id,
  merchant: 'Synthetic credit',
  amount,
  pending,
  date: '2026-08-20',
  benefitId: 'resy',
  benefitName: 'Resy',
  kind: amount < 0 ? 'reversal' : 'credit',
})

const period = (start: string, credited: number, activityIds: string[]): BenefitPeriod => ({
  start,
  end: start,
  label: 'Synthetic',
  cap: 100,
  credited,
  state: 'partial',
  activityIds,
})

const resy: BenefitHistory = {
  id: 'resy',
  name: 'Resy',
  recurring: false,
  external: false,
  retired: false,
  periods: [
    period('2026-07-01', 60, ['mine', 'theirs', 'their-reversal', 'their-pending']),
    // Last year's credits belong to last year.
    period('2025-07-01', 50, ['theirs-before']),
  ],
}

it('splits the year’s captured credits into yours and someone else’s', () => {
  const activity = new Map(
    [
      credit('mine', 40),
      credit('theirs', 30),
      credit('their-reversal', -10),
      credit('their-pending', 25, true),
      credit('theirs-before', 50),
    ].map((item) => [item.id, item]),
  )
  const others = new Set(['theirs', 'their-reversal', 'their-pending', 'theirs-before'])
  // Posted credits net of reversals; a pending one hasn't landed yet.
  expect(capturedForOthers([resy], 2026, activity, others)).toBe(20)
  // Yours and theirs add up to what the year captured.
  expect(benefitYearTotals([resy], 2026).captured - 20).toBe(40)
  expect(capturedForOthers([resy], 2026, activity, new Set())).toBe(0)
})
