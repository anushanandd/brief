import { expect, it } from 'vitest'

import { benefitYearTotals, cardYearTotals } from './benefit-grid'
import type { BenefitHistory, CardBenefits } from './schema'

const year = {
  year: 2026,
  captured: 60,
  planned: 13.99,
  missed: { low: 10, high: 20 },
  left: { low: 0, high: 0 },
}

const resy: BenefitHistory = {
  id: 'resy',
  name: 'Resy',
  recurring: false,
  external: false,
  retired: false,
  periods: [],
  years: [year],
}

it("reads a benefit's year as Rust summed it, nothing for a year without periods", () => {
  expect(benefitYearTotals(resy, 2026)).toBe(year)
  expect(benefitYearTotals(resy, 2025)).toMatchObject({ captured: 0, missed: { low: 0, high: 0 } })
  expect(benefitYearTotals(undefined, 2026).planned).toBe(0)
})

it("reads the card's year over the benefits shown, with what went to someone else", () => {
  const card: Pick<CardBenefits, 'years'> = { years: [{ ...year, forOthers: 20 }] }
  expect(cardYearTotals(card, 2026)).toMatchObject({ captured: 60, forOthers: 20 })
  expect(cardYearTotals(card, 2025)).toMatchObject({ captured: 0, forOthers: 0 })
  expect(cardYearTotals(undefined, 2026).forOthers).toBe(0)
})
