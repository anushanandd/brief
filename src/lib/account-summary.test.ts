import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { accountTransactionSummary, rangeInvestmentFlows } from './account-summary'

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
    start: '2026-08-04',
    end: '2026-09-03',
  })
})

const series = (accountId: string, points: Array<[string, number, number | null]>) => ({
  accountId,
  name: accountId,
  institution: 'Example',
  currentValue: 0,
  performanceMethod: 'modified-dietz' as const,
  points: points.map(([date, netDeposits, marketChange]) => ({
    date,
    value: 0,
    netDeposits,
    sp500: null,
    marketChange,
  })),
})

it('separates new contributions from market movement across investment series', () => {
  const flows = rangeInvestmentFlows(
    [
      series('a', [
        ['2026-08-01', 1_000, null],
        ['2026-08-31', 1_000, 5],
        ['2026-09-10', 1_250, 12.5],
        ['2026-09-20', 1_250, -2.25],
        ['2026-10-01', 1_400, 50],
      ]),
      // Tracking starts inside the range: its opening value is not a contribution.
      series('b', [
        ['2026-09-05', 500, null],
        ['2026-09-15', 600, 1],
      ]),
    ],
    '2026-09-01',
    '2026-09-30',
  )
  expect(flows).toEqual({ contributions: 350, marketChange: 11.25 })
  expect(rangeInvestmentFlows([], '2026-09-01', '2026-09-30')).toEqual({
    contributions: null,
    marketChange: null,
  })
})
