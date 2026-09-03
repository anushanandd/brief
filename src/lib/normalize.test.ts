import { describe, expect, it } from 'vitest'

import { applyMarketSnapshots, normalizeSnapshot } from './normalize'

describe('provider normalization', () => {
  it('imports Plaid stock plan accounts and holdings', () => {
    const snapshot = normalizeSnapshot(
      {
        accounts: [],
        transactions: [],
        investmentAccounts: [
          {
            account_id: 'stock-plan',
            balances: { current: 1250 },
            institution_name: 'E*TRADE',
            name: 'Stock Plan',
            subtype: 'stock plan',
            type: 'investment',
          },
          {
            account_id: 'brokerage',
            balances: { current: 9999 },
            institution_name: 'E*TRADE',
            name: 'Individual Brokerage',
            subtype: 'brokerage',
            type: 'investment',
          },
        ],
        holdings: [
          {
            account_id: 'stock-plan',
            cost_basis: 1000,
            institution_price: 125,
            institution_value: 1250,
            quantity: 10,
            security_id: 'security',
          },
          {
            account_id: 'brokerage',
            institution_price: 9999,
            institution_value: 9999,
            quantity: 1,
            security_id: 'excluded-security',
          },
        ],
        securities: [
          { security_id: 'security', ticker_symbol: 'AAPL', name: 'Apple Inc.' },
          { security_id: 'excluded-security', ticker_symbol: 'VOO', name: 'Vanguard S&P 500 ETF' },
        ],
        ignoredAccounts: [],
      },
      { accounts: [], positions: {}, activities: {}, ignoredAccounts: [] },
      [],
      new Date('2026-08-31T12:00:00Z'),
    )

    expect(snapshot.accounts).toContainEqual({
      id: 'plaid:stock-plan',
      name: 'Stock Plan',
      institution: 'E*TRADE',
      type: 'brokerage',
      value: 1250,
    })
    expect(snapshot.accounts.some(({ id }) => id === 'plaid:brokerage')).toBe(false)
    expect(snapshot.netWorth).toBe(1250)
    expect(snapshot.holdings).toHaveLength(1)
    expect(snapshot.holdings[0]).toMatchObject({
      accountId: 'plaid:stock-plan',
      ticker: 'AAPL',
      shares: 10,
      value: 1250,
      costBasis: 1000,
      totalChangePct: 25,
    })
    expect(
      snapshot.brokeragePerformance.some(({ accountId }) => accountId === 'plaid:stock-plan'),
    ).toBe(true)
    expect(snapshot.providers.find(({ id }) => id === 'plaid-investments')?.status).toBe('ready')
  })

  it('normalizes balances, transaction signs, and history deterministically', () => {
    const snapshot = normalizeSnapshot(
      {
        accounts: [
          {
            account_id: 'card',
            balances: { current: 125 },
            institution_name: 'American Express',
            name: 'Platinum Card',
            type: 'credit',
          },
        ],
        transactions: [
          {
            account_id: 'card',
            amount: 20,
            authorized_date: '2026-08-29',
            date: '2026-08-30',
            personal_finance_category: { primary: 'FOOD_AND_DRINK' },
            logo_url: 'https://plaid-merchant-logos.plaid.com/coffee.png',
            merchant_name: 'Coffee Shop',
            name: 'Coffee',
            pending: false,
            transaction_id: 'transaction',
          },
        ],
        ignoredAccounts: [],
      },
      {
        accounts: [],
        positions: {},
        activities: {},
        ignoredAccounts: [],
      },
      [],
      new Date('2026-08-31T12:00:00Z'),
    )

    expect(snapshot.netWorth).toBe(-125)
    expect(snapshot.transactions[0]?.amount).toBe(-20)
    expect(snapshot.transactions[0]?.date).toBe('2026-08-29')
    expect(snapshot.transactions[0]?.logoUrl).toBe(
      'https://plaid-merchant-logos.plaid.com/coffee.png',
    )
    expect(snapshot.transactions[0]?.logoName).toBe('Coffee Shop')
    expect(snapshot.spending.categories[0]?.color).toBe('#c9684b')
    expect(snapshot.netWorthHistory).toHaveLength(730)
    expect(snapshot.netWorthHistory.at(-3)).toEqual({ date: '2026-08-29', value: -125 })
    expect(snapshot.netWorthHistory.at(-2)).toEqual({ date: '2026-08-30', value: -125 })
    expect(snapshot.netWorthHistory.at(-1)).toEqual({ date: '2026-08-31', value: -125 })
    expect(snapshot.netWorthHistoryEstimated).toBe(true)
    expect(snapshot.providers.map((provider) => provider.id)).toContain('alpaca')
    expect(snapshot.providers.map((provider) => provider.id)).toContain('logos')
  })

  it('applies live holding deltas to account and net worth totals', () => {
    const snapshot = normalizeSnapshot(
      {
        accounts: [{ account_id: 'cash', balances: { current: 100 }, type: 'depository' }],
        transactions: [],
        ignoredAccounts: [],
      },
      {
        accounts: [{ id: 'brokerage', balance: { total: { amount: 1000 } } }],
        positions: {
          brokerage: [
            {
              units: 2,
              price: 100,
              cost_basis: 80,
              instrument: { symbol: 'AAPL', kind: 'stock' },
            },
          ],
        },
        activities: {
          brokerage: [
            {
              type: 'SELL',
              amount: 6000,
              trade_date: '2026-08-30',
              description: 'Sold Apple',
              symbol: { raw_symbol: 'AAPL' },
            },
            {
              type: 'WITHDRAWAL',
              amount: 6000,
              trade_date: '2026-08-31',
              description: 'Cash transfer',
            },
          ],
        },
        ignoredAccounts: [],
      },
      [],
      new Date('2026-08-31T12:00:00Z'),
    )
    const live = applyMarketSnapshots(snapshot, {
      AAPL: {
        symbol: 'AAPL',
        price: 110,
        previousClose: 105,
        dailyChangePct: 4.76,
        asOf: '2026-08-31T19:00:00Z',
      },
    })

    expect(live.holdings[0]?.value).toBe(220)
    expect(live.holdings[0]?.costBasis).toBe(160)
    expect(live.holdings[0]?.totalChangePct).toBe(37.5)
    expect(live.accounts.find((account) => account.id === 'snaptrade:brokerage')?.value).toBe(1020)
    expect(live.accounts.find((account) => account.id === 'plaid:cash')?.value).toBe(100)
    expect(live.netWorth).toBe(1120)
    expect(live.netWorthHistory.at(-1)?.value).toBe(1120)
    expect(live.investmentActivities).toEqual([
      {
        accountId: 'snaptrade:brokerage',
        accountName: 'Brokerage account',
        date: '2026-08-30',
        type: 'SELL',
        amount: 6000,
        description: 'Sold Apple',
        symbol: 'AAPL',
      },
      {
        accountId: 'snaptrade:brokerage',
        accountName: 'Brokerage account',
        date: '2026-08-31',
        type: 'WITHDRAWAL',
        amount: -6000,
        description: 'Cash transfer',
      },
    ])
    expect(
      live.brokeragePerformance
        .find((account) => account.accountId === 'snaptrade:brokerage')
        ?.points.at(-1)?.value,
    ).toBe(1020)
  })
})
