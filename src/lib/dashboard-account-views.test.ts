import { describe, expect, it } from 'vitest'

import {
  accountValueChart,
  accountWeeklyChangePct,
  accountValueIncomplete,
  hasValueHistory,
  dashboardAccountViews,
  dashboardAssetBreakdown,
} from './dashboard-account-views'
import type { FinanceSnapshot } from './schema'

type BrokeragePerformance = FinanceSnapshot['brokeragePerformance'][number]

it('matches account selector changes to the selected chart range and available history', () => {
  const account = { id: 'cash', name: 'Cash', institution: 'Example', type: 'cash', value: 120 }
  const now = Date.parse('2026-09-08T12:00:00Z') / 1000
  const history = [
    { date: '2026-08-09', value: 150, netDeposits: null, sp500: null },
    { date: '2026-09-01', value: 100, netDeposits: null, sp500: null },
    { date: '2026-09-08', value: 120, netDeposits: null, sp500: null },
  ]
  const data = {
    accounts: [account],
    updatedAt: '2026-09-08T12:00:00Z',
    netWorthHistory: history,
    brokeragePerformance: [],
    accountBalanceHistory: [
      {
        accountId: 'cash',
        name: 'Cash',
        institution: 'Example',
        currentValue: 120,
        points: history,
      },
    ],
  }
  const week = accountValueChart(data, account, [], now, 7 * 86400, now)
  expect(week.historyIsAvailable).toBe(true)
  expect(week.chartChange.percent).toBe(20)
  expect(accountValueChart(data, account, [], now, 30 * 86400, now).chartChange.percent).toBe(-20)
  expect(accountValueChart(data, account, [], now, 0, now).chartChange.percent).toBe(-20)
  expect(accountValueChart(data, account, [], now, 0, now, '2026-09-01').chartChange.percent).toBe(
    20,
  )
  expect(
    accountValueChart(data, { ...account, id: 'all', type: 'combined' }, [], now, 7 * 86400, now)
      .chartChange.percent,
  ).toBe(20)
  expect(
    accountValueChart(data, account, [{ time: now, value: 110 }], now, 7 * 86400, now).chartChange
      .percent,
  ).toBe(10)
  expect(
    accountValueChart(data, { ...account, value: null }, [], now, 0, now).historyIsAvailable,
  ).toBe(false)
  expect(
    accountValueChart({ ...data, accountBalanceHistory: [] }, account, [], now, 0, now)
      .historyIsAvailable,
  ).toBe(false)
})

const view = (accountId: string, name = accountId): BrokeragePerformance => ({
  accountId,
  name,
  institution: 'Broker',
  currentValue: 100,
  points: [],
})

describe('dashboard account views', () => {
  it('requires actual balance history for seven-day balance changes', () => {
    const account = {
      id: 'account',
      name: 'Account',
      institution: 'Broker',
      type: 'brokerage',
      value: 120,
    }
    expect(
      accountWeeklyChangePct(
        {
          updatedAt: '2026-09-08T12:00:00Z',
          accountBalanceHistory: [
            {
              ...view('account'),
              currentValue: 120,
              points: [
                { date: '2026-09-01', value: 100, netDeposits: null, sp500: null },
                { date: '2026-09-08', value: 120, netDeposits: null, sp500: null },
              ],
            },
          ],
          brokeragePerformance: [],
          holdings: [],
        },
        account,
      ),
    ).toBe(20)
    expect(
      accountWeeklyChangePct(
        {
          updatedAt: '2026-09-08T12:00:00Z',
          accountBalanceHistory: [],
          brokeragePerformance: [],
          holdings: [
            {
              ticker: 'VTI',
              name: 'VTI',
              accountId: 'account',
              shares: 1,
              price: 100,
              value: 100,
              costBasis: null,
              dailyChangePct: 0,
              weeklyChangePct: 11.11,
              weeklyReferencePrice: 90,
              weeklyReferenceDate: '2026-09-01',
              totalChangePct: 0,
              color: '#000',
            },
          ],
        },
        account,
      ),
    ).toBeNull()
  })

  it('keeps investment accounts while breaking them into holdings', () => {
    expect(
      dashboardAssetBreakdown({
        accounts: [
          { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 130 },
          { id: 'cash', name: 'Checking', institution: 'Bank', type: 'cash', value: 30 },
          {
            id: 'brokerage',
            name: 'Brokerage',
            institution: 'Broker',
            type: 'brokerage',
            value: 100,
          },
        ],
        holdings: [
          {
            ticker: 'VTI',
            name: 'Vanguard Total Stock Market ETF',
            accountId: 'brokerage',
            shares: 1,
            price: 80,
            value: 80,
            dailyChangePct: null,
            weeklyChangePct: null,
            weeklyReferencePrice: null,
            weeklyReferenceDate: null,
            totalChangePct: null,
            color: '#fff',
            costBasis: null,
          },
        ],
      }),
    ).toEqual([
      {
        id: 'brokerage',
        name: 'Brokerage',
        type: 'brokerage',
        value: 100,
        children: [
          { name: 'VTI', value: 80 },
          { name: 'Cash & other', value: 20 },
        ],
      },
      { id: 'cash', name: 'Checking', type: 'cash', value: 30, children: [] },
    ])
  })

  it('shows all six account graphs in the requested order', () => {
    const views = dashboardAccountViews({
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: 500,
      netWorthHistory: [{ date: '2026-09-03', value: 500 }],
      accounts: [
        { id: 'all', name: 'All accounts', institution: 'Brief', type: 'combined', value: 500 },
        {
          id: 'plaid:stock-plan',
          name: 'Stock Plan',
          institution: 'E*TRADE',
          type: 'brokerage',
          value: 25,
        },
      ],
      brokeragePerformance: [
        view('total'),
        view('snaptrade:second', 'Brokerage 2'),
        view('snaptrade:roth', 'Roth IRA'),
        view('snaptrade:first', 'Brokerage 1'),
      ],
    })

    expect(views.map(({ accountId }) => accountId)).toEqual([
      'total',
      'snaptrade:roth',
      'snaptrade:first',
      'snaptrade:second',
      'plaid:stock-plan',
      'net-worth',
    ])
    expect(views[0]?.name).toBe('Combined brokerages')
    expect(views[0]?.currentValue).toBe(100)
    expect(views[0]?.points).toEqual([])
    expect(views[4]).toMatchObject({
      accountId: 'plaid:stock-plan',
      name: 'Stock Plan',
      currentValue: 25,
      points: [{ value: 25, sp500: null }],
    })
    expect(views.at(-1)?.name).toBe('All accounts')
    expect(views.at(-1)?.points[0]?.sp500).toBeNull()
  })
})

it('uses complete USD coverage consistently for aggregate and individual values', () => {
  const complete = {
    id: 'complete',
    name: 'Complete',
    institution: 'Synthetic',
    type: 'brokerage',
    value: 100,
  }
  const missing = { ...complete, id: 'missing', value: null }
  const data = { accounts: [complete, missing], netWorthIncomplete: true }
  expect(accountValueIncomplete(data, 'all')).toBe(true)
  expect(accountValueIncomplete(data, 'net-worth')).toBe(true)
  expect(accountValueIncomplete(data, 'total')).toBe(true)
  expect(accountValueIncomplete(data, 'missing')).toBe(true)
  expect(accountValueIncomplete(data, 'complete')).toBe(false)
})

it('requires two distinct supported history dates after the chosen start date', () => {
  const point = { date: '2026-09-01', value: 100 }
  expect(hasValueHistory([])).toBe(false)
  expect(hasValueHistory([point])).toBe(false)
  expect(hasValueHistory([point, point])).toBe(false)
  expect(hasValueHistory([point, { date: '2026-99-99', value: 110 }])).toBe(false)
  expect(hasValueHistory([point, { date: '2026-09-02', value: Number.NaN }])).toBe(false)
  const points = [point, { date: '2026-09-02', value: 110 }]
  expect(hasValueHistory(points)).toBe(true)
  expect(hasValueHistory(points, '2026-09-02')).toBe(false)
})
