import { describe, expect, it } from 'vitest'

import seed from '../data/seed.json'
import { buildChartEventGroups, chartEventGroupLabel } from './chart-events'
import { financeSnapshotSchema } from './schema'

describe('buildChartEventGroups', () => {
  it('selects reported activity, deduplicates internal transfers, and derives portfolio moves', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.updatedAt = '2026-09-10T20:00:00Z'
    data.netWorth = 100_000
    data.spending.monthTotal = 2_000
    data.transactions = [
      {
        id: 'salary',
        merchant: 'Payroll',
        category: 'Income',
        date: '2026-09-08',
        postedOn: '2026-09-08',
        amount: 4_000,
        account: 'Checking',
        accountId: 'plaid:checking',
        pending: false,
      },
      {
        id: 'transfer-out',
        merchant: 'Transfer out',
        category: 'Transfer',
        date: '2026-09-09',
        amount: -2_000,
        account: 'Brokerage',
        accountId: 'snaptrade:brokerage',
        pending: false,
      },
      {
        id: 'transfer-in',
        merchant: 'Transfer in',
        category: 'Transfer',
        date: '2026-09-09',
        amount: 2_000,
        account: 'Checking',
        accountId: 'plaid:checking',
        pending: false,
      },
    ]
    data.trades = [
      {
        id: 'sale',
        type: 'SELL',
        date: '2026-09-07',
        amount: 2_100,
        account: 'Brokerage',
        accountId: 'snaptrade:brokerage',
        ticker: 'VTI',
      },
    ]
    data.brokeragePerformance = [
      {
        accountId: 'snaptrade:brokerage',
        name: 'Brokerage',
        institution: 'Broker',
        currentValue: 6_500,
        historySource: 'provider-estimated',
        performanceMethod: 'value-with-comparisons',
        points: [
          { date: '2026-09-07', value: 8_000, netDeposits: 8_000, sp500: 8_000 },
          {
            date: '2026-09-09',
            value: 6_500,
            netDeposits: 6_000,
            sp500: 6_000,
            marketChange: 500,
            marketChangePct: 6.25,
          },
        ],
      },
    ]

    const allAccounts = buildChartEventGroups(data, 'net-worth', [], 0)
    expect(allAccounts.flatMap(({ events }) => events).map(({ title }) => title)).toEqual([
      'Sold VTI',
      'Payroll',
      'Internal transfer',
    ])

    const brokerage = buildChartEventGroups(
      data,
      'snaptrade:brokerage',
      data.brokeragePerformance[0].points,
      0,
    )
    expect(brokerage.flatMap(({ events }) => events).map(({ kind }) => kind)).toEqual([
      'sale',
      'transfer',
      'market-move',
    ])
    expect(chartEventGroupLabel(brokerage[1], data.updatedAt)).toContain('Near sold vti')
    expect(chartEventGroupLabel(brokerage[1], data.updatedAt)).toContain('estimated')
  })
})
