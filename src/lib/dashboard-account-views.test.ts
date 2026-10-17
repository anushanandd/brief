import { describe, expect, it } from 'vitest'

import { emptyRollup, insights } from '../data/fixtures/insights'
import { account as record, performance } from '../data/fixtures/records'
import {
  accountValueChart,
  accountValueIncomplete,
  hasValueHistory,
  dashboardAccountViews,
  withoutSpendingCard,
} from './dashboard-account-views'

/** A history point as Rust sends one with no deposits, benchmark, or market change. */
const point = (date: string, value: number) => ({
  date,
  value,
  netDeposits: null,
  sp500: null,
  marketChange: null,
  marketChangePct: null,
})

it('matches account selector changes to the selected chart range and available history', () => {
  const account = record({ id: 'cash', name: 'Cash', institution: 'Example', value: 120 })
  const now = Date.parse('2026-09-08T12:00:00Z') / 1000
  const history = [point('2026-08-09', 150), point('2026-09-01', 100), point('2026-09-08', 120)]
  const data = {
    accounts: [account],
    updatedAt: '2026-09-08T12:00:00Z',
    netWorthIncomplete: false,
    netWorthHistory: history,
    brokeragePerformance: [],
    accountBalanceHistory: [
      performance({
        accountId: 'cash',
        name: 'Cash',
        institution: 'Example',
        currentValue: 120,
        points: history,
      }),
    ],
  }
  // Rust read each chart's change from the line it draws; the combined account is net worth.
  const changes = [
    { series: 'cash', windows: { week: { change: 20, percent: 20 } } },
    { series: 'net-worth', windows: { week: { change: 5, percent: 5 } } },
  ]
  const week = accountValueChart(data, account, [], now, 7 * 86400, changes)
  expect(week.historyIsAvailable).toBe(true)
  expect(week.chartChange).toEqual({ change: 20, percent: 20 })
  expect(
    accountValueChart(
      data,
      { ...account, id: 'all', type: 'combined' },
      [],
      now,
      7 * 86400,
      changes,
    ).chartChange.percent,
  ).toBe(5)
  // A window Rust has no reading for changed nothing.
  expect(accountValueChart(data, account, [], now, 30 * 86400, changes).chartChange).toEqual({
    change: 0,
    percent: 0,
  })
  expect(
    accountValueChart(data, { ...account, value: null }, [], now, 0, []).historyIsAvailable,
  ).toBe(false)
  expect(
    accountValueChart({ ...data, accountBalanceHistory: [] }, account, [], now, 0, [])
      .historyIsAvailable,
  ).toBe(false)
  // A manual retirement account has no performance history, only its balance history.
  const manual = { ...account, id: 'manual:plan', type: 'retirement' }
  const manualData = {
    ...data,
    accounts: [manual],
    accountBalanceHistory: [{ ...data.accountBalanceHistory[0], accountId: 'manual:plan' }],
  }
  expect(accountValueChart(manualData, manual, [], now, 0, []).historyIsAvailable).toBe(true)
})

it('keeps a provisional combined balance out of the chart', () => {
  const now = Date.parse('2026-09-24T12:00:00Z') / 1000
  const account = record({
    id: 'all',
    name: 'All accounts',
    institution: 'Brief',
    type: 'combined',
    value: 9300,
  })
  const chart = accountValueChart(
    {
      accounts: [account],
      netWorthIncomplete: false,
      netWorthProvisional: true,
      updatedAt: '2026-09-24T12:00:00Z',
      netWorthHistory: [
        { date: '2026-09-22', value: 300 },
        { date: '2026-09-23', value: 300 },
      ],
      brokeragePerformance: [],
      accountBalanceHistory: [],
    },
    account,
    [{ time: now, value: 9300 }],
    now,
    0,
    [],
  )
  expect(chart.chartData.at(-1)?.value).toBe(300)
})

const view = (accountId: string, name = accountId) =>
  performance({ accountId, name, institution: 'Broker', currentValue: 100 })

describe('dashboard account views', () => {
  it('shows all six account graphs in the requested order', () => {
    const views = dashboardAccountViews({
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: 500,
      netWorthHistory: [{ date: '2026-09-03', value: 500 }],
      accounts: [
        record({
          id: 'all',
          name: 'All accounts',
          institution: 'Brief',
          type: 'combined',
          value: 500,
        }),
        record({
          id: 'plaid:stock-plan',
          name: 'Stock Plan',
          institution: 'E*TRADE',
          type: 'brokerage',
          value: 25,
        }),
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
  const complete = record({
    id: 'complete',
    name: 'Complete',
    institution: 'Synthetic',
    type: 'brokerage',
    value: 100,
  })
  const missing = { ...complete, id: 'missing', value: null }
  const data = { accounts: [complete, missing], netWorthIncomplete: true }
  expect(accountValueIncomplete(data, 'all')).toBe(true)
  expect(accountValueIncomplete(data, 'net-worth')).toBe(true)
  expect(accountValueIncomplete(data, 'total')).toBe(true)
  expect(accountValueIncomplete(data, 'missing')).toBe(true)
  expect(accountValueIncomplete(data, 'complete')).toBe(false)
})

it('requires two distinct supported history dates after the chosen start date', () => {
  const start = { date: '2026-09-01', value: 100 }
  expect(hasValueHistory([])).toBe(false)
  expect(hasValueHistory([start])).toBe(false)
  expect(hasValueHistory([start, start])).toBe(false)
  expect(hasValueHistory([start, { date: '2026-99-99', value: 110 }])).toBe(false)
  expect(hasValueHistory([start, { date: '2026-09-02', value: Number.NaN }])).toBe(false)
  const points = [start, { date: '2026-09-02', value: 110 }]
  expect(hasValueHistory(points)).toBe(true)
  expect(hasValueHistory(points, '2026-09-02')).toBe(false)
})

it('leaves the spending card out of the All accounts total and history, as Rust totaled them', () => {
  const data = {
    accounts: [
      record({
        id: 'all',
        name: 'All accounts',
        institution: 'Brief',
        type: 'combined',
        value: 900.1,
      }),
      record({ id: 'card', name: 'Card', institution: 'Example', type: 'credit', value: -300.2 }),
    ],
    netWorthHistory: [{ date: '2026-09-01', value: 800 }],
    insights: insights({
      accountsTotal: {
        excludedAccountIds: ['card'],
        excludedValue: -300.2,
        value: 1200.3,
        history: [{ date: '2026-09-01', value: 950 }],
      },
      rollup: emptyRollup({ netWorth: 1200.3, combined: 1200.3 }),
    }),
  }
  const scoped = withoutSpendingCard({ ...data, netWorth: 900.1 })
  expect(scoped.accounts.find(({ id }) => id === 'all')?.value).toBe(1200.3)
  expect(scoped.accounts.find(({ id }) => id === 'card')?.value).toBe(-300.2)
  expect(scoped.netWorth).toBe(1200.3)
  expect(scoped.netWorthHistory).toEqual([{ date: '2026-09-01', value: 950 }])
  const unscoped = { ...data, insights: undefined }
  expect(withoutSpendingCard(unscoped)).toBe(unscoped)
})
