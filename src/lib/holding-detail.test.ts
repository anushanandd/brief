import { expect, it } from 'vitest'

import { holdingDetail } from './holding-detail'
import type { FinanceSnapshot } from './schema'

const position: FinanceSnapshot['holdings'][number] = {
  ticker: 'TEST',
  name: 'Synthetic',
  accountId: 'one',
  shares: 2,
  price: 50,
  value: 100,
  costBasis: 80,
  dailyChangePct: 2,
  weeklyChangePct: 3,
  weeklyReferencePrice: null,
  weeklyReferenceDate: null,
  totalChangePct: 25,
  color: '#fff',
}

it('combines the selected security across accounts without counting other tickers', () => {
  const result = holdingDetail(
    [
      position,
      { ...position, accountId: 'two', shares: 1, value: 50, costBasis: 35 },
      { ...position, ticker: 'OTHER', value: 150 },
    ],
    'TEST',
  )
  expect(result).toMatchObject({
    shares: 3,
    value: 150,
    basis: 115,
    averageBasis: 115 / 3,
    totalChange: (35 / 115) * 100,
    weight: 50,
    dailyChange: 2,
    weeklyChange: 3,
  })
  expect(result.positions).toHaveLength(2)
})

it('preserves unavailable totals, zero basis, and conflicting return evidence', () => {
  expect(
    holdingDetail(
      [
        position,
        {
          ...position,
          accountId: 'two',
          shares: null,
          value: null,
          costBasis: null,
          dailyChangePct: 4,
          weeklyChangePct: null,
        },
      ],
      'TEST',
    ),
  ).toMatchObject({
    shares: null,
    value: null,
    basis: null,
    totalChange: null,
    averageBasis: null,
    weight: null,
    dailyChange: null,
    weeklyChange: null,
  })
  expect(holdingDetail([{ ...position, costBasis: 0 }], 'TEST')).toMatchObject({
    averageBasis: 0,
    totalChange: null,
  })
  expect(holdingDetail([], 'TEST')).toMatchObject({
    shares: null,
    value: null,
    basis: null,
    totalChange: null,
    weight: null,
  })
  expect(holdingDetail([{ ...position, value: 0, shares: 0 }], 'TEST')).toMatchObject({
    averageBasis: null,
    weight: null,
  })
})
