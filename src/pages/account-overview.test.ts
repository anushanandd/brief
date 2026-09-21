import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { accountIncomeBreakdown } from './account-overview'

it('splits posted current-year dividends and interest without counting other income', () => {
  const transaction = {
    category: 'Income',
    date: '2026-08-21',
    amount: 10,
    account: 'Brokerage',
    accountId: 'brokerage',
    classification: classification('income'),
    pending: false,
  }
  expect(
    accountIncomeBreakdown(
      [
        {
          ...transaction,
          id: 'dividend',
          merchant: 'Dividend · VTI',
          amount: 12.5,
          classification: classification('dividend'),
        },
        {
          ...transaction,
          id: 'interest',
          merchant: 'Cash interest',
          amount: 3.25,
          classification: classification('interest'),
        },
        {
          ...transaction,
          id: 'salary',
          merchant: 'Salary',
          amount: 5_000,
          classification: classification('income'),
        },
        {
          ...transaction,
          id: 'old',
          merchant: 'Dividend · VTI',
          date: '2025-12-31',
          classification: classification('dividend'),
        },
        {
          ...transaction,
          id: 'pending',
          merchant: 'Interest',
          pending: true,
          classification: classification('interest'),
        },
      ],
      '2026-09-03T12:00:00Z',
    ),
  ).toEqual({ dividends: 12.5, interest: 3.25 })
})
