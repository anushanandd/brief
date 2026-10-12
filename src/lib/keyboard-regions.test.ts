import { describe, expect, it } from 'vitest'

import { countedRowIndex } from './keyboard-regions'

describe('list motions', () => {
  it('applies counts and wraps deterministically', () => {
    expect(countedRowIndex(0, 10, 1, 5)).toBe(5)
    expect(countedRowIndex(1, 10, -1, 3)).toBe(8)
    expect(countedRowIndex(-1, 4, 1, 1)).toBe(0)
    expect(countedRowIndex(0, 0, 1, 4)).toBe(-1)
  })
})
