import { expect, it } from 'vitest'

import { periodChangeText } from './period-totals'

it('reads a rise to ten times or more as a multiple, and a net as a percent', () => {
  expect(periodChangeText(50, true)).toBe('+50.00%')
  expect(periodChangeText(-40, true)).toBe('−40.00%')
  expect(periodChangeText(900, true)).toBe('10×')
  expect(periodChangeText(3_750, true)).toBe('39×')
  expect(periodChangeText(3_750, false)).toBe('+3750.00%')
})
