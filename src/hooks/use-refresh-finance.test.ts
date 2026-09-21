import { describe, expect, it } from 'vitest'

import { isFinanceSyncStale } from './use-refresh-finance'

describe('finance snapshot staleness', () => {
  it('marks the saved sync stale at the one-hour boundary', () => {
    const now = Date.parse('2026-09-10T12:00:00Z')

    expect(isFinanceSyncStale('2026-09-10T11:00:00Z', now)).toBe(true)
    expect(isFinanceSyncStale('2026-09-10T11:00:00.001Z', now)).toBe(false)
    expect(isFinanceSyncStale('invalid', now)).toBe(true)
  })
})
