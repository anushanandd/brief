import { describe, expect, it } from 'vitest'

import { adjacentRowIndex } from './keyboard-regions'

describe('list motions', () => {
  it('steps one row and wraps at either end', () => {
    expect(adjacentRowIndex(0, 10, 1)).toBe(1)
    expect(adjacentRowIndex(0, 10, -1)).toBe(9)
    expect(adjacentRowIndex(9, 10, 1)).toBe(0)
    expect(adjacentRowIndex(-1, 4, 1)).toBe(0)
    expect(adjacentRowIndex(0, 0, 1)).toBe(-1)
  })
})
