import { describe, expect, it } from 'vitest'

import { normalizeSnapshot } from './normalize'

describe('provider normalization', () => {
  it('normalizes balances, transaction signs, and history deterministically', () => {
    const snapshot = normalizeSnapshot(
      {
        accounts: [
          {
            account_id: 'card',
            balances: { current: 125 },
            institution_name: 'American Express',
            name: 'Platinum Card',
            type: 'credit',
          },
        ],
        transactions: [
          {
            account_id: 'card',
            amount: 20,
            authorized_date: '2026-08-29',
            date: '2026-08-30',
            logo_url: 'https://plaid-merchant-logos.plaid.com/coffee.png',
            merchant_name: 'Coffee Shop',
            name: 'Coffee',
            pending: false,
            transaction_id: 'transaction',
          },
        ],
        ignoredAccounts: [],
      },
      {
        accounts: [],
        positions: {},
        activities: {},
        ignoredAccounts: [],
      },
      [],
      new Date('2026-08-31T12:00:00Z'),
    )

    expect(snapshot.netWorth).toBe(-125)
    expect(snapshot.debt).toBe(125)
    expect(snapshot.transactions[0]?.amount).toBe(-20)
    expect(snapshot.transactions[0]?.date).toBe('2026-08-29')
    expect(snapshot.transactions[0]?.logoUrl).toBe(
      'https://plaid-merchant-logos.plaid.com/coffee.png',
    )
    expect(snapshot.transactions[0]?.logoName).toBe('Coffee Shop')
    expect(snapshot.netWorthHistory).toHaveLength(90)
    expect(snapshot.netWorthHistory.at(-3)).toEqual({ date: '2026-08-29', value: -125 })
    expect(snapshot.netWorthHistory.at(-2)).toEqual({ date: '2026-08-30', value: -125 })
    expect(snapshot.netWorthHistory.at(-1)).toEqual({ date: '2026-08-31', value: -125 })
    expect(snapshot.netWorthHistoryEstimated).toBe(true)
  })

  it('adds balances across all connected credit cards', () => {
    const snapshot = normalizeSnapshot(
      {
        accounts: [
          { account_id: 'card-1', balances: { current: 125 }, type: 'credit' },
          { account_id: 'card-2', balances: { current: 75 }, type: 'credit' },
        ],
        transactions: [],
        ignoredAccounts: [],
      },
      { accounts: [], positions: {}, activities: {}, ignoredAccounts: [] },
      [],
      new Date('2026-08-31T12:00:00Z'),
    )

    expect(snapshot.debt).toBe(200)
    expect(snapshot.spending.statementBalance).toBe(200)
  })
})
