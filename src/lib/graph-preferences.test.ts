import { describe, expect, it } from 'vitest'

import { graphWindows, parseGraphWindow } from './graph-preferences'

describe('graph preferences', () => {
  it('accepts supported windows and falls back to one week', () => {
    expect(parseGraphWindow(String(graphWindows[1].secs))).toBe(graphWindows[1].secs)
    expect(parseGraphWindow(String(graphWindows[3].secs))).toBe(0)
    expect(parseGraphWindow(null)).toBe(graphWindows[0].secs)
    expect(parseGraphWindow('unsupported')).toBe(graphWindows[0].secs)
  })
})
