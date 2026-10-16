import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { cashFlowBreakdown } from './money'
import type { Transaction } from './schema'

const transaction = (
  value: Partial<Transaction> & Pick<Transaction, 'id' | 'amount'>,
): Transaction => ({
  merchant: 'Merchant',
  category: 'Other',
  date: '2026-09-01',
  account: 'Checking',
  accountId: 'cash',
  classification: classification('expense'),
  pending: false,
  ...value,
})

describe('money model', () => {
  it('groups cash flow by exact income source and purchase category', () => {
    const transactions = [
      transaction({
        id: 'pay-1',
        amount: 1_500,
        category: 'Income',
        merchant: 'Acme Payroll',
        classification: classification('income'),
      }),
      transaction({
        id: 'pay-2',
        amount: 500,
        category: 'Income',
        merchant: 'Acme Payroll',
        classification: classification('income'),
      }),
      transaction({
        id: 'interest',
        amount: 25,
        category: 'Interest',
        merchant: 'Marcus',
        classification: classification('interest'),
      }),
      transaction({
        id: 'meal-1',
        amount: -80,
        category: 'Dining',
        merchant: 'Cafe',
        classification: classification('expense'),
      }),
      transaction({
        id: 'meal-2',
        amount: -20,
        category: 'Dining',
        merchant: 'Deli',
        classification: classification('expense'),
      }),
      transaction({
        id: 'flight',
        amount: -300,
        category: 'Travel',
        merchant: 'Airline',
        classification: classification('expense'),
      }),
    ]

    expect(cashFlowBreakdown(transactions)).toMatchObject({
      sources: [
        {
          label: 'Acme Payroll',
          value: 2_000,
          children: [
            { id: 'source-transaction:pay-1', value: 1_500 },
            { id: 'source-transaction:pay-2', value: 500 },
          ],
        },
        { label: 'Marcus', value: 25 },
      ],
      purchases: [
        {
          label: 'Travel',
          value: 300,
          children: [{ id: 'purchase-transaction:flight', label: 'Airline' }],
        },
        {
          label: 'Dining',
          value: 100,
          children: [
            { id: 'purchase-transaction:meal-1', label: 'Cafe' },
            { id: 'purchase-transaction:meal-2', label: 'Deli' },
          ],
        },
      ],
    })
  })
})
