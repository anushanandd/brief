import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import seed from '../data/seed.json'
import { homeOverviewMetrics } from './home-overview'
import { financeSnapshotSchema } from './schema'
const time = (date: string) => Date.parse(date + 'T12:00:00Z') / 1000
function snapshot() {
  const data = financeSnapshotSchema.parse(seed)
  data.updatedAt = '2026-09-18T12:00:00Z'
  data.accounts = [{ id: 'card', name: 'Card', institution: 'Example', type: 'credit', value: -20 }]
  data.netWorth = 1350
  data.netWorthHistory = [
    { date: '2026-08-18', value: 1000 },
    { date: '2026-09-11', value: 1100 },
    { date: '2026-09-18', value: 1350 },
  ]
  data.brokeragePerformance = [
    {
      accountId: 'total',
      name: 'Portfolio',
      institution: 'Example',
      currentValue: 1350,
      performanceMethod: 'value-with-comparisons',
      points: data.netWorthHistory.map((p, i) => ({
        ...p,
        netDeposits: i === 2 ? 1200 : 1000,
        sp500: null,
        marketChange: i === 0 ? null : i === 1 ? 100 : 50,
      })),
    },
  ]
  data.benchmarkHistory = [
    { date: '2026-08-18', value: 100 },
    { date: '2026-09-11', value: 110 },
    { date: '2026-09-18', value: 132 },
  ]
  const transaction = data.transactions[0]
  data.transactions = [
    {
      ...transaction,
      id: 'salary',
      accountId: 'cash',
      merchant: 'Payroll',
      category: 'Income',
      amount: 1200,
      postedOn: '2026-09-17',
      classification: classification('income'),
      pending: false,
    },
    {
      ...transaction,
      id: 'old',
      accountId: 'card',
      merchant: 'Older expense',
      category: 'Groceries',
      amount: -200,
      postedOn: '2026-08-20',
      classification: classification('expense'),
      pending: false,
    },
    {
      ...transaction,
      id: 'expense',
      accountId: 'card',
      merchant: 'Grocer',
      category: 'Groceries',
      amount: -80,
      postedOn: '2026-09-17',
      classification: classification('expense'),
      pending: false,
    },
    {
      ...transaction,
      id: 'pending',
      accountId: 'card',
      merchant: 'Pending',
      category: 'Groceries',
      amount: -500,
      postedOn: '2026-09-17',
      classification: classification('expense'),
      pending: true,
    },
    {
      ...transaction,
      id: 'other',
      accountId: 'other',
      merchant: 'Other card',
      category: 'Groceries',
      amount: -999,
      postedOn: '2026-09-17',
      classification: classification('expense'),
      pending: false,
    },
  ]
  return data
}
it('uses the selected range for cash flow, portfolio and balance changes', () => {
  const data = snapshot()
  const week = homeOverviewMetrics(data, 'card', {
    start: time('2026-09-11'),
    end: time('2026-09-18'),
  })
  expect(week.portfolio).toBeCloseTo((50 / 1100) * 100)
  expect(week.benchmark).toBeCloseTo(20)
  expect(week.allAccounts).toBeCloseTo((250 / 1100) * 100)
  expect(week.spending).toBe(80)
  expect(week.income).toBe(1200)
  const all = homeOverviewMetrics(data, 'card', {
    start: time('2026-08-18'),
    end: time('2026-09-18'),
  })
  expect(all.portfolio).toBe(15)
  expect(all.allAccounts).toBe(35)
  expect(all.spending).toBe(280)
})
it('estimates partial intervals but never replaces missing performance evidence with deposits', () => {
  const data = snapshot()
  const range = { start: time('2026-09-11') + 3.5 * 86400, end: time('2026-09-18') }
  expect(homeOverviewMetrics(data, 'card', range).portfolio).toBeCloseTo((25 / 1225) * 100)
  data.brokeragePerformance[0].points[2].marketChange = null
  expect(homeOverviewMetrics(data, 'card', range).portfolio).toBeNull()
  data.netWorthIncomplete = true
  expect(homeOverviewMetrics(data, 'card', range).allAccounts).toBeNull()
  expect(homeOverviewMetrics(data, undefined, range).spending).toBeNull()
  expect(
    homeOverviewMetrics(data, 'card', { start: time('2026-10-01'), end: time('2026-10-08') })
      .income,
  ).toBeNull()
})
it('requires opening coverage instead of silently shortening the selected range', () => {
  const result = homeOverviewMetrics(snapshot(), 'card', {
    start: time('2026-01-01'),
    end: time('2026-09-18'),
  })
  expect(result.portfolio).toBeNull()
  expect(result.benchmark).toBeNull()
  expect(result.allAccounts).toBeNull()
})
