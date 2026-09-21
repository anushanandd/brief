import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { buildActivities } from './activity'
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
          classification: classification('expense'),
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
          classification: classification('transfer'),
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
            classification: classification('income'),
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
            id: 'brokerage-income',
            merchant: 'TRANSFER MONEY FROM BROKERAGE XXXXX8549 Reference Number: MCK1SOY78',
            classification: classification('income', { brokerageIncomeTransfer: true }),
          },
        ],
      })[0],
    ).toMatchObject({ kind: 'income' })
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
            classification: classification('other', { credit: true }),
          },
        ],
      })[0],
    ).toMatchObject({ kind: 'credit', title: 'Platinum Digital Entertainment Credit' })
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
          classification: classification('expense'),
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
