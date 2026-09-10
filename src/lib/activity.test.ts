import { describe, expect, it } from 'vitest'

import { buildActivities, mergeAccountMovements } from './activity'
import { financeSnapshotSchema } from './schema'

describe('financial activities', () => {
  it('combines spending and trades without account balance changes', () => {
    const snapshot = financeSnapshotSchema.parse({
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: 125,
      accounts: [
        { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 125 },
      ],
      netWorthHistory: [{ date: '2026-09-03', value: 125 }],
      holdings: [],
      trades: [
        {
          id: 'buy',
          type: 'BUY',
          date: '2026-09-02',
          amount: -50,
          account: 'Brokerage',
          accountId: 'brokerage',
          ticker: 'VTI',
          units: 2,
        },
      ],
      spending: { monthTotal: 25, categories: [] },
      transactions: [
        {
          id: 'lunch',
          merchant: 'Lunch',
          category: 'Dining',
          date: '2026-09-01',
          amount: -25,
          account: 'Card',
          pending: false,
          logoUrl: 'https://plaid-merchant-logos.plaid.com/lunch.png',
        },
      ],
      lastChange: {
        observedAt: '2026-09-03T12:00:00Z',
        previousUpdatedAt: '2026-09-02T12:00:00Z',
        previousNetWorth: 100,
        netWorthChange: 25,
        accountChanges: [{ accountId: 'cash', name: 'Checking', change: 25 }],
        newTransactionIds: ['lunch'],
      },
    })

    expect(buildActivities(snapshot).map(({ kind }) => kind)).toEqual(['trade', 'spending'])
    expect(buildActivities(snapshot)[0]).toMatchObject({ title: 'Bought VTI', amount: -50 })
    expect(
      buildActivities({
        ...snapshot,
        trades: [{ ...snapshot.trades[0], type: 'REINVESTMENT' }],
      })[0],
    ).toMatchObject({ title: 'Reinvested VTI', amount: -50 })
    expect(buildActivities(snapshot)[1]).toMatchObject({
      title: 'Lunch',
      logoUrl: expect.stringMatching(/^data:image\/svg\+xml/),
    })
  })

  it('keeps a transfer after its one-refresh account delta clears', () => {
    const snapshot = financeSnapshotSchema.parse({
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: 100,
      accounts: [
        { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 100 },
        { id: 'savings', name: 'Savings', institution: 'Bank', type: 'cash', value: 100 },
      ],
      netWorthHistory: [{ date: '2026-09-03', value: 100 }],
      holdings: [],
      trades: [],
      spending: { monthTotal: 0, categories: [] },
      transactions: [
        {
          id: 'transfer',
          merchant: 'Transfer to savings',
          category: 'Transfer In',
          date: '2026-09-02',
          amount: 100,
          account: 'Savings',
          accountId: 'savings',
          pending: false,
        },
      ],
      lastChange: {
        observedAt: '2026-09-03T12:00:00Z',
        previousUpdatedAt: '2026-09-02T12:00:00Z',
        previousNetWorth: 100,
        netWorthChange: 0,
        accountChanges: [],
        newTransactionIds: [],
      },
    })

    expect(buildActivities(snapshot)).toEqual([
      expect.objectContaining({
        kind: 'transfer',
        title: 'Transfer to savings',
        amount: 100,
      }),
    ])
    expect(
      buildActivities({
        ...snapshot,
        transactions: [
          {
            ...snapshot.transactions[0],
            id: 'income',
            merchant: 'Payroll',
            category: 'Income',
          },
        ],
      })[0],
    ).toMatchObject({ kind: 'income', title: 'Payroll' })
    expect(
      buildActivities({
        ...snapshot,
        transactions: [
          {
            ...snapshot.transactions[0],
            id: 'credit',
            merchant: 'Platinum Digital Entertainment Credit',
            category: 'Entertainment',
            amount: 15.99,
          },
        ],
      })[0],
    ).toMatchObject({ kind: 'credit', title: 'Platinum Digital Entertainment Credit' })
  })

  it('keeps account movements after later refreshes report no new change', () => {
    const previous = financeSnapshotSchema.parse({
      updatedAt: '2026-09-02T12:00:00Z',
      netWorth: 100,
      accounts: [
        { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 100 },
        { id: 'savings', name: 'Savings', institution: 'Bank', type: 'cash', value: 100 },
      ],
      netWorthHistory: [{ date: '2026-09-02', value: 100 }],
      holdings: [],
      trades: [],
      spending: { monthTotal: 0, categories: [] },
      transactions: [],
    })
    const firstChange = {
      observedAt: '2026-09-03T12:00:00Z',
      previousUpdatedAt: previous.updatedAt,
      previousNetWorth: 100,
      netWorthChange: 100,
      accountChanges: [{ accountId: 'savings', name: 'Savings', change: 100 }],
      newTransactionIds: [],
    }
    const first = mergeAccountMovements(previous, firstChange)
    const second = mergeAccountMovements(
      { ...previous, accountMovements: first, lastChange: firstChange },
      {
        ...firstChange,
        observedAt: '2026-09-04T12:00:00Z',
        previousUpdatedAt: firstChange.observedAt,
        netWorthChange: 0,
        accountChanges: [],
      },
    )

    expect(first).toHaveLength(1)
    expect(second).toEqual(first)
    expect(
      buildActivities({ ...previous, accountMovements: second, lastChange: undefined }),
    ).toEqual([])
  })

  it('uses local display names without changing account IDs', () => {
    const snapshot = financeSnapshotSchema.parse({
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: 100,
      accounts: [
        { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 100 },
      ],
      netWorthHistory: [{ date: '2026-09-03', value: 100 }],
      holdings: [],
      trades: [],
      spending: { monthTotal: 0, categories: [] },
      transactions: [
        {
          id: 'purchase',
          merchant: 'Coffee',
          category: 'Dining',
          date: '2026-09-03',
          amount: -5,
          account: 'Platinum Card',
          accountId: 'amex',
          pending: false,
          website: 'https://coffee.example',
        },
      ],
    })

    expect(buildActivities(snapshot, { amex: 'Everyday card' })[0]).toMatchObject({
      detail: 'Everyday card · Dining',
      website: 'https://coffee.example',
    })
    expect(snapshot.transactions[0].account).toBe('Platinum Card')
  })
})
