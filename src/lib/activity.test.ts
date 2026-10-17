import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import {
  activityMatchesSearch,
  activityPlace,
  activityMethod,
  linkedAccountIds,
  buildActivities,
  sortActivities,
  type ActivityItem,
  type ActivitySort,
} from './activity'
import { financeSnapshotSchema } from './schema'

describe('financial activities', () => {
  it('searches descriptions and every Activity filter field', () => {
    const activity: ActivityItem = {
      id: 'synthetic',
      kind: 'spending',
      title: 'Acme',
      description: 'Monthly cloud storage',
      account: 'Daily card',
      category: 'Software',
      location: { city: 'Seattle', region: 'WA' },
      paymentChannel: 'online',
      date: '2026-09-03',
      amount: -12.34,
      detail: 'Daily card · Software',
    }
    for (const query of [
      'cloud',
      'cloud daily',
      'description:cloud storage',
      'account:daily',
      'category:software',
      'method:online',
      'location:seattle',
      'amount:$12.34',
      'date:2026-09-03',
    ]) {
      expect(activityMatchesSearch(activity, query)).toBe(true)
    }
    expect(activityMatchesSearch(activity, 'description:daily')).toBe(false)
    expect(activityMatchesSearch(activity, 'method:in store')).toBe(false)
  })

  it('offers linked accounts only when their activity is present', () => {
    expect(
      linkedAccountIds(['snaptrade:account'], {
        plaid: 'snaptrade:account',
        unrelated: 'snaptrade:other',
      }),
    ).toEqual(new Set(['snaptrade:account', 'plaid']))
  })
  it('sorts activity by each visible column without changing the source rows', () => {
    const rows: ActivityItem[] = [
      {
        id: 'b',
        kind: 'income',
        title: 'Bravo',
        account: 'Zulu',
        category: 'Dining',
        location: { city: 'Seattle', region: 'WA' },
        paymentChannel: 'in store',
        date: '2026-09-02',
        amount: -20,
        detail: '',
      },
      {
        id: 'a',
        kind: 'income',
        title: 'alpha',
        account: 'Alpha',
        category: 'Trade',
        location: { city: 'Austin', region: 'TX' },
        paymentChannel: 'online',
        date: '2026-09-03',
        amount: 10,
        detail: '',
      },
      {
        id: 'c',
        kind: 'income',
        title: 'Charlie',
        account: 'Alpha',
        category: 'Dining',
        date: '2026-09-01',
        amount: 5,
        detail: '',
      },
    ]
    const ids = (sort: ActivitySort) => sortActivities(rows, sort).map(({ id }) => id)

    expect(ids({ column: 'description', direction: 'asc' })).toEqual(['a', 'b', 'c'])
    expect(ids({ column: 'account', direction: 'asc' })).toEqual(['a', 'c', 'b'])
    expect(ids({ column: 'category', direction: 'asc' })).toEqual(['b', 'c', 'a'])
    expect(ids({ column: 'location', direction: 'asc' })).toEqual(['c', 'a', 'b'])
    expect(ids({ column: 'method', direction: 'asc' })).toEqual(['c', 'b', 'a'])
    expect(ids({ column: 'date', direction: 'desc' })).toEqual(['a', 'b', 'c'])
    expect(ids({ column: 'amount', direction: 'desc' })).toEqual(['a', 'c', 'b'])
    expect(ids({ column: 'amount', direction: 'asc' })).toEqual(['b', 'c', 'a'])
    expect(rows.map(({ id }) => id)).toEqual(['b', 'a', 'c'])
  })

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
          location: { address: '123 Example St', city: 'San Francisco', region: 'CA' },
          paymentChannel: 'in store',
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
    expect(activityMethod(buildActivities(snapshot)[0])).toBe('Brokerage')
    expect(
      activityMethod({ ...buildActivities(snapshot)[1], accountId: 'snaptrade:example' }),
    ).toBe('Brokerage')
    expect(
      buildActivities({
        ...snapshot,
        trades: [{ ...snapshot.trades[0], type: 'REINVESTMENT' }],
      })[0],
    ).toMatchObject({ title: 'Reinvested VTI', amount: -50 })
    expect(buildActivities(snapshot)[1]).toMatchObject({
      title: 'Lunch',
      location: { address: '123 Example St', city: 'San Francisco', region: 'CA' },
      paymentChannel: 'in store',
      logoUrl: undefined,
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
        mark: 'transfer',
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
      account: 'Everyday card',
      detail: 'Everyday card · Dining',
      website: 'https://coffee.example',
    })
    expect(snapshot.transactions[0].account).toBe('Platinum Card')
  })
})

it('describes where a purchase happened in one place, then adds the street, channel, or site', () => {
  const base: ActivityItem = {
    id: 'place',
    kind: 'spending',
    title: 'Example',
    detail: '',
    category: 'Dining',
    date: '2026-09-01',
    amount: -10,
  }
  expect(
    activityPlace({
      ...base,
      paymentChannel: 'in store',
      location: { address: '1 Main St', city: 'Oakland', region: 'CA', country: 'US' },
    }),
  ).toEqual({ kind: 'place', primary: 'Oakland, CA', secondary: '1 Main St' })
  expect(
    activityPlace({
      ...base,
      paymentChannel: 'in store',
      location: { city: 'Paris', country: 'FR' },
    }),
  ).toEqual({ kind: 'place', primary: 'Paris · FR', secondary: 'In store' })
  expect(
    activityPlace({ ...base, paymentChannel: 'online', website: 'https://www.example.com/a' }),
  ).toEqual({ kind: 'online', primary: 'Online', secondary: 'example.com' })
  expect(activityPlace({ ...base, paymentChannel: 'other' })).toEqual({
    kind: 'none',
    primary: '—',
    secondary: '',
  })
})

const leg = (id: string, accountId: string, account: string, date: string, amount: number) => ({
  id,
  accountId,
  account,
  merchant: 'Synthetic transfer',
  category: 'Transfers',
  date,
  amount,
  pending: false,
  classification: classification('transfer'),
})

it('links each leg of a paired transfer to the account on its other side', () => {
  const [sent, arrived, alone] = buildActivities(
    {
      transactions: [
        {
          ...leg('sent', 'plaid:checking', 'Checking', '2026-09-03', -50),
          transferPairId: 'arrived',
        },
        { ...leg('arrived', 'plaid:savings', 'Savings', '2026-09-02', 50), transferPairId: 'sent' },
        leg('alone', 'plaid:checking', 'Checking', '2026-09-01', -20),
      ],
      trades: [],
      updatedAt: '2026-09-16T12:00:00Z',
    },
    { 'plaid:savings': 'Rainy day' },
  )
  expect(sent.transfer).toEqual({
    id: 'spending:arrived',
    direction: 'to',
    account: 'Rainy day',
    accountId: 'plaid:savings',
    date: '2026-09-02',
  })
  expect(arrived.transfer).toMatchObject({ id: 'spending:sent', direction: 'from' })
  expect(activityPlace(sent)).toEqual({ kind: 'transfer', primary: 'To Rainy day', secondary: '' })
  expect(activityPlace(arrived).primary).toBe('From Checking')
  expect(alone.transfer).toBeUndefined()
  expect(activityPlace(alone).kind).not.toBe('transfer')
})
