import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import seed from '../data/seed.json'
import { buildChartEventGroups, chartEventGroupLabel } from './chart-events'
import { financeSnapshotSchema } from './schema'

describe('buildChartEventGroups', () => {
  it('keeps external money transfers directional on the all-accounts chart', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.updatedAt = '2026-09-10T20:00:00Z'
    data.netWorth = 100_000
    data.transactions = [
      {
        id: 'external-in',
        merchant: 'External deposit',
        category: 'Transfer',
        date: '2026-09-08',
        amount: 1_500,
        account: 'Checking',
        accountId: 'plaid:checking',
        classification: classification('transfer'),
        pending: false,
      },
      {
        id: 'external-out',
        merchant: 'External withdrawal',
        category: 'Transfer',
        date: '2026-09-09',
        amount: -1_600,
        account: 'Checking',
        accountId: 'plaid:checking',
        classification: classification('transfer'),
        pending: false,
      },
    ]
    data.trades = []

    const chartEvents = buildChartEventGroups(data, 'net-worth', 0).flatMap((group) => group.events)
    expect(chartEvents.map(({ title, direction }) => [title, direction])).toEqual([
      ['External deposit', 'in'],
      ['External withdrawal', 'out'],
    ])
  })

  it('includes small directional trades and deduplicates internal transfers', () => {
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
        classification: classification('income'),
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
        classification: classification('transfer'),
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
        classification: classification('transfer'),
        pending: false,
      },
    ]
    data.trades = [
      {
        id: 'buy',
        type: 'BUY',
        date: '2026-09-06',
        amount: -1_200,
        account: 'Brokerage',
        accountId: 'snaptrade:brokerage',
        ticker: 'VXUS',
      },
      {
        id: 'sale',
        type: 'SELL',
        date: '2026-09-07',
        amount: 207.05,
        account: 'Brokerage',
        accountId: 'snaptrade:brokerage',
        ticker: 'VTI',
        estimatedRealizedGainPct: -12.5,
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

    const allAccounts = buildChartEventGroups(data, 'net-worth', 0)
    expect(allAccounts.flatMap(({ events }) => events).map(({ title }) => title)).toEqual([
      'Bought VXUS',
      'Sold VTI',
      'Payroll',
      'Internal transfer',
    ])

    const brokerage = buildChartEventGroups(data, 'snaptrade:brokerage', 0)
    const brokerageEvents = brokerage.flatMap(({ events }) => events)
    expect(brokerageEvents.map(({ kind, direction }) => [kind, direction])).toEqual([
      ['buy', 'in'],
      ['sale', 'out'],
      ['transfer', 'out'],
    ])
    expect(chartEventGroupLabel(brokerage[1], data.updatedAt)).toContain(
      'Sold VTI $207.05 · -12.50%',
    )
    expect(chartEventGroupLabel(brokerage[2], data.updatedAt)).toContain('Near sold vti')
    expect(chartEventGroupLabel(brokerage[0], data.updatedAt)).toContain('Bought VXUS')
  })
})
