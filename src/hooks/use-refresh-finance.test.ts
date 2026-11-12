import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it, vi } from 'vitest'

import { emptySnapshot } from '../data/fixtures/records'
import { refreshFinanceSnapshot } from '../lib/api'
import { isFinanceSyncStale, refreshOnce } from './use-refresh-finance'

vi.mock('../lib/api', () => ({
  isTauri: () => false,
  refreshFinanceSnapshot: vi.fn(),
  syncWallet: vi.fn(),
}))

describe('finance snapshot staleness', () => {
  it('marks the saved sync stale at the one-hour boundary', () => {
    const now = Date.parse('2026-09-10T12:00:00Z')

    expect(isFinanceSyncStale('2026-09-10T11:00:00Z', now)).toBe(true)
    expect(isFinanceSyncStale('2026-09-10T11:00:00.001Z', now)).toBe(false)
    expect(isFinanceSyncStale('invalid', now)).toBe(true)
  })
})

type Result = Awaited<ReturnType<typeof refreshFinanceSnapshot>>

/** Refreshes that finish when the test says, in the order they started. */
function pendingRefreshes() {
  const finish: Array<(result: Result) => void> = []
  vi.mocked(refreshFinanceSnapshot).mockReset()
  vi.mocked(refreshFinanceSnapshot).mockImplementation(
    () => new Promise<Result>((resolve) => finish.push(resolve)),
  )
  return finish
}
/** Answers the `index`th refresh Rust was asked for. */
function settle(finish: Array<(result: Result) => void>, index: number, value: Result) {
  const resolve = finish[index]
  if (!resolve) throw new Error(`Refresh ${index} was never asked for`)
  resolve(value)
}
// Only which refresh answered matters here; the refresh hands Rust's result through untouched.
const result = (name: string): Result => ({
  snapshot: { ...emptySnapshot(), syncWarnings: [name] },
  warnings: [],
})

describe('one refresh at a time', () => {
  it('runs a live refresh asked for during an ordinary one, after it, never alongside', async () => {
    const finish = pendingRefreshes()
    const client = new QueryClient()
    const automatic = refreshOnce(client, false)
    const manual = refreshOnce(client, true)
    await Promise.resolve()
    expect(vi.mocked(refreshFinanceSnapshot).mock.calls).toEqual([[{ liveBalances: false }]])

    settle(finish, 0, result('saved'))
    await expect(automatic).resolves.toEqual(result('saved'))
    await vi.waitFor(() => expect(finish).toHaveLength(2))
    expect(vi.mocked(refreshFinanceSnapshot).mock.calls[1]).toEqual([{ liveBalances: true }])
    settle(finish, 1, result('live'))
    await expect(manual).resolves.toEqual(result('live'))
  })

  it('joins a refresh of the same kind, and an ordinary request joins a live one', async () => {
    const finish = pendingRefreshes()
    const client = new QueryClient()
    const first = refreshOnce(client, false)
    const second = refreshOnce(client, false)
    settle(finish, 0, result('saved'))
    await expect(Promise.all([first, second])).resolves.toEqual([result('saved'), result('saved')])

    const live = refreshOnce(client, true)
    const automatic = refreshOnce(client, false)
    settle(finish, 1, result('live'))
    await expect(Promise.all([live, automatic])).resolves.toEqual([result('live'), result('live')])
    // Only the explicit request ever asked Rust for live balances.
    expect(vi.mocked(refreshFinanceSnapshot).mock.calls).toEqual([
      [{ liveBalances: false }],
      [{ liveBalances: true }],
    ])
  })
})
