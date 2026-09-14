import { describe, expect, it } from 'vitest'

import {
  accountWeeklyChangePct,
  chartValueChange,
  currentMonthIncome,
  dashboardAccountViews,
  dashboardAssetBreakdown,
  monthlyPortfolioChange,
} from '../lib/dashboard-account-views'
import type { FinanceSnapshot } from '../lib/schema'

type BrokeragePerformance = FinanceSnapshot['brokeragePerformance'][number]

const view = (accountId: string, name = accountId): BrokeragePerformance => ({
  accountId,
  name,
  institution: 'Broker',
  currentValue: 100,
  points: [],
})

describe('dashboard account views', () => {
  it('uses complete account or holding evidence for seven-day value changes', () => {
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
    ).toBeCloseTo(9.09, 2)
  })

  it('uses the same latest change for Home and account charts', () => {
    const points = [
      { date: '2026-09-02', value: 100 },
      { date: '2026-09-03', value: 105 },
    ]
    expect(chartValueChange(points, 105, '2026-09-03')).toEqual({
      change: 5,
      percent: 5,
      period: 'today',
    })
    expect(chartValueChange(points, 110, '2026-09-04')).toEqual({
      change: 5,
      percent: 5 / 1.05,
      period: 'today',
    })
  })

  it('calculates the current monthly overview from imported evidence', () => {
    expect(
      monthlyPortfolioChange({
        updatedAt: '2026-08-31T12:00:00Z',
        brokeragePerformance: [
          {
            ...view('total'),
            currentValue: 110,
            points: [
              { date: 'Jul', value: 100, netDeposits: null, sp500: null },
              { date: 'Aug', value: 110, netDeposits: null, sp500: null },
            ],
          },
        ],
      }),
    ).toBeCloseTo(10)
    expect(
      currentMonthIncome({
        updatedAt: '2026-08-31T12:00:00Z',
        transactions: [
          {
            id: 'salary',
            merchant: 'Salary',
            category: 'Income',
            date: 'Aug 29',
            amount: 100,
            account: 'Checking',
            pending: false,
          },
          {
            id: 'pending',
            merchant: 'Salary',
            category: 'Income',
            date: 'Aug 30',
            amount: 50,
            account: 'Checking',
            pending: true,
          },
        ],
      }),
    ).toBe(100)
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
