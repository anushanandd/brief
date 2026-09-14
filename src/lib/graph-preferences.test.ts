import { describe, expect, it } from 'vitest'

import {
  adjacentGraphWindow,
  graphWindowForKey,
  graphWindows,
  parseChartAccountPreferences,
  parseGraphWindow,
  reconcileChartAccountPreferences,
} from './graph-preferences'

describe('graph preferences', () => {
  it('offers week, month, quarter, and all-time ranges', () => {
    const day = 24 * 60 * 60
    expect(graphWindows.map(({ label }) => label)).toEqual(['1W', '1M', '3M', 'All'])
    expect(parseGraphWindow(String(day))).toBe(7 * day)
    expect(parseGraphWindow(String(30 * day))).toBe(30 * day)
    expect(parseGraphWindow(String(90 * day))).toBe(90 * day)
    expect(parseGraphWindow('0')).toBe(0)
    expect(parseGraphWindow(null)).toBe(7 * day)
    expect(adjacentGraphWindow(7 * day, -1)).toBe(0)
    expect(adjacentGraphWindow(7 * day, 1)).toBe(30 * day)
    expect(adjacentGraphWindow(30 * day, 1)).toBe(90 * day)
    expect(adjacentGraphWindow(90 * day, 1)).toBe(0)
    expect(adjacentGraphWindow(0, 1)).toBe(7 * day)
    expect(graphWindowForKey('w')).toBe(7 * day)
    expect(graphWindowForKey('M')).toBe(30 * day)
    expect(graphWindowForKey('q')).toBe(90 * day)
    expect(graphWindowForKey('a')).toBe(0)
    expect(graphWindowForKey('x')).toBeUndefined()
  })

  it('keeps saved chart visibility and order while appending new accounts', () => {
    const saved = parseChartAccountPreferences(
      JSON.stringify([
        { accountId: 'second', visible: false },
        { accountId: 'first', visible: true },
      ]),
    )
    expect(reconcileChartAccountPreferences(['first', 'second', 'new'], saved)).toEqual([
      { accountId: 'second', visible: false },
      { accountId: 'first', visible: true },
      { accountId: 'new', visible: true },
    ])
    expect(parseChartAccountPreferences('{broken')).toEqual([])
  })
})
