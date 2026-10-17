import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { account, trade, transaction } from '../data/fixtures/records'
import { accountStartDate, parseAccountStartDates } from './account-start-dates'

const data = {
  updatedAt: '2026-09-10T12:00:00Z',
  accounts: [
    account({
      id: 'brokerage',
      name: 'Brokerage',
      institution: 'Broker',
      type: 'brokerage',
      value: 100,
    }),
    account({ id: 'cash', name: 'Cash', institution: 'Bank', type: 'cash', value: 50 }),
  ],
  accountLinks: { bank: 'brokerage' },
  transactions: [
    transaction({
      id: 'deposit',
      accountId: 'bank',
      account: 'Brokerage',
      merchant: 'Deposit',
      category: 'Transfer',
      date: '2024-06-03',
      amount: 100,
      classification: classification('transfer'),
      pending: false,
    }),
    transaction({
      id: 'cash-deposit',
      accountId: 'cash',
      account: 'Cash',
      merchant: 'Deposit',
      category: 'Transfer',
      date: '2025-02-01',
      amount: 50,
      classification: classification('transfer'),
      pending: false,
    }),
  ],
  trades: [
    trade({
      id: 'buy',
      type: 'BUY',
      accountId: 'brokerage',
      account: 'Brokerage',
      date: '2024-07-01',
      amount: -100,
    }),
  ],
} satisfies Parameters<typeof accountStartDate>[0]

describe('account start dates', () => {
  it('uses the first linked activity unless Settings overrides it', () => {
    expect(accountStartDate(data, 'brokerage', {})).toBe('2024-06-03')
    expect(accountStartDate(data, 'brokerage', { brokerage: '2020-01-15' })).toBe('2020-01-15')
    expect(accountStartDate(data, 'net-worth', {})).toBe('2024-06-03')
  })

  it('rejects malformed saved dates', () => {
    expect(
      parseAccountStartDates(
        JSON.stringify({ brokerage: '2020-01-15', impossible: '2025-02-30', other: 4 }),
      ),
    ).toEqual({ brokerage: '2020-01-15' })
  })
})
