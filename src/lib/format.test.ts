import { describe, expect, it } from 'vitest'

import { formatCompactCurrency, formatCurrency, formatPercent } from './format'

describe('financial formatting', () => {
  it('formats currency with cents', () => {
    expect(formatCurrency(2418.33)).toBe('$2,418.33')
  })

  it('formats compact dashboard values', () => {
    expect(formatCompactCurrency(286421)).toBe('$286.4K')
  })

  it('preserves the sign of percentage movement', () => {
    expect(formatPercent(0.65)).toBe('+0.65%')
    expect(formatPercent(-0.31)).toBe('-0.31%')
  })
})
