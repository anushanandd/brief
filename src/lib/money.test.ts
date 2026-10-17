import { expect, it } from 'vitest'

import { cents, sumCents } from './money'

it('rounds to whole cents with halves up, as Rust does, and sums in cents', () => {
  // Exact binary halves: up toward positive infinity, on either side of zero.
  expect(cents(0.125)).toBe(13)
  expect(cents(-0.125)).toBe(-12)
  expect(sumCents([0.1, 0.2])).toBe(0.3)
})
