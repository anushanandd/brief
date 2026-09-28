import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { accountTransactionSummary } from './account-overview'

it('summarizes selected-range and all-time cash activity', () => {
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
    accountTransactionSummary(
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
      30 * 24 * 60 * 60,
    ),
  ).toMatchObject({
    range: {
      moneyIn: 5_015.75,
      moneyOut: 0,
      dividends: 12.5,
      interest: 3.25,
      netFlow: 5_015.75,
    },
    allTime: { moneyIn: 5_025.75, netFlow: 5_025.75, dividends: 22.5, interest: 3.25 },
    start: '2026-08-04',
    end: '2026-09-03',
    days: 30,
  })
})
