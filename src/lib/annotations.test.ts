import { invoke } from '@tauri-apps/api/core'
import { afterEach, describe, expect, it, vi } from 'vitest'

import empty from '../data/empty.json'
import { applyTransactionAnnotations, loadTransactionAnnotations } from './annotations'
import { financeSnapshotSchema } from './schema'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetAllMocks()
})

describe('shared transaction annotations', () => {
  it('loads native annotations even if legacy JSON is malformed', async () => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
    vi.stubGlobal('localStorage', { getItem: () => '{ broken' })
    vi.mocked(invoke).mockResolvedValue({
      transaction: { category: 'Travel', benefitConfirmed: true },
    })
    const result = await loadTransactionAnnotations()
    expect(result.annotations.transaction).toEqual({ category: 'Travel', benefitConfirmed: true })
    expect(result.warning).toContain('legacy')
    expect(invoke).toHaveBeenCalledWith('transaction_annotations', { changes: {}, importing: true })
  })

  it('keeps browser finance available if optional metadata is malformed or unreadable', async () => {
    vi.stubGlobal('window', {})
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('Unavailable')
      },
    })
    const result = await loadTransactionAnnotations()
    expect(result.annotations).toEqual({})
    expect(result.warning).toContain('Finance data remains available')
  })
  it('overlays interaction state without recalculating native totals or guessing accounts', () => {
    const snapshot = financeSnapshotSchema.parse({
      ...empty,
      updatedAt: '2026-08-31T12:00:00Z',
      accounts: ['first', 'second'].map((id) => ({
        id,
        name: 'Card',
        institution: 'Bank',
        type: 'credit',
        value: 0,
      })),
      transactions: [
        {
          id: 'explicit',
          accountId: 'second',
          account: 'Card',
          merchant: 'Coffee',
          category: 'Food',
          amount: -20,
          date: '2026-08-01',
          pending: false,
        },
        {
          id: 'ambiguous',
          account: 'Card',
          merchant: 'Shop',
          category: 'Shopping',
          amount: -10,
          date: '2026-08-02',
          pending: false,
        },
      ],
    })
    const result = applyTransactionAnnotations(snapshot, { explicit: { category: 'Transfer Out' } })
    expect(result.transactions[0].accountId).toBe('second')
    expect(result.transactions[1].accountId).toBeUndefined()
    expect(result.spending.monthTotal).toBe(snapshot.spending.monthTotal)
    expect(snapshot.transactions[0].category).toBe('Food')
  })
})
