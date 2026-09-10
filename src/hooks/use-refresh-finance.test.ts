import { describe, expect, it } from 'vitest'

import { isFinanceSyncStale } from './use-refresh-finance'

describe('automatic finance refresh', () => {
  it('refreshes once the saved sync is at least one hour old', () => {
    const now = Date.parse('2026-09-10T12:00:00Z')

    expect(isFinanceSyncStale('2026-09-10T11:00:00Z', now)).toBe(true)
    expect(isFinanceSyncStale('2026-09-10T11:00:00.001Z', now)).toBe(false)
    expect(isFinanceSyncStale('invalid', now)).toBe(true)
  })
})
