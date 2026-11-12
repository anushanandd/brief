import { expect, it } from 'vitest'

import { emptyRollup, insights } from '../data/fixtures/insights'
import { holding } from '../data/fixtures/records'
import { holdingDetail } from './holding-detail'

const position = holding({
  ticker: 'TEST',
  name: 'Synthetic',
  accountId: 'one',
  shares: 2,
  price: 50,
  value: 100,
  costBasis: 80,
  dailyChangePct: 2,
  weeklyChangePct: 3,
  totalChangePct: 25,
  color: '#fff',
})

const total = {
  ticker: 'TEST',
  name: 'Synthetic',
  instrumentKind: null,
  color: '#fff',
  quoteEligible: null,
  valuationNote: null,
  currency: 'USD',
  shares: 3,
  vestedShares: null,
  value: 150,
  costBasis: 115,
  unrealizedGain: 35,
  totalChangePct: 30.43,
  averageBasis: 38.33,
  weight: 50,
  price: 50,
  dailyChangePct: 2,
  weeklyChangePct: 3,
  marketAsOf: null,
  accounts: [
    {
      accountId: 'one',
      currency: 'USD',
      shares: 2,
      value: 100,
      costBasis: 80,
      unrealizedGain: 20,
    },
    { accountId: 'two', currency: 'USD', shares: 1, value: 50, costBasis: 35, unrealizedGain: 15 },
  ],
}

it("reads a security's position as Rust totaled it, with the positions behind it", () => {
  const holdings = [
    position,
    { ...position, accountId: 'two', shares: 1, value: 50, costBasis: 35 },
    { ...position, ticker: 'OTHER', value: 150 },
  ]
  const data = { holdings, insights: insights({ rollup: emptyRollup({ holdings: [total] }) }) }
  const detail = holdingDetail(data, 'TEST')
  expect(detail).toMatchObject({ shares: 3, value: 150, costBasis: 115, gain: 35, weight: 50 })
  expect(detail.positions.map(({ accountId }) => accountId)).toEqual(['one', 'two'])
  expect(detail.accounts).toBe(total.accounts)
  // A security Rust has no total for reads as unknown, never as a sum made here.
  expect(holdingDetail(data, 'OTHER')).toMatchObject({ value: null, shares: null, accounts: [] })
  expect(holdingDetail({ holdings }, 'TEST').value).toBeNull()
})
