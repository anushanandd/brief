import { describe, expect, it } from 'vitest'

import { buildBrokeragePerformance } from './performance'

describe('brokerage performance', () => {
  it('builds value, net deposit, and cash-flow-adjusted benchmark lines per account', () => {
    const performance = buildBrokeragePerformance(
      {
        accounts: [
          {
            id: 'brokerage',
            name: 'Individual',
            institution_name: 'Broker',
            balance: { total: { amount: 140 } },
          },
        ],
        activities: {
          brokerage: [{ type: 'CONTRIBUTION', trade_date: '2026-08-30', amount: 20 }],
        },
        balanceHistory: {
          brokerage: [
            { date: '2026-08-29', total_value: '100' },
            { date: '2026-08-30', total_value: '130' },
          ],
        },
      },
      [
        { date: '2026-08-29', value: 100 },
        { date: '2026-08-30', value: 110 },
        { date: '2026-08-31', value: 121 },
      ],
      new Date('2026-08-31T12:00:00Z'),
      3,
    )

    expect(performance[0]?.accountId).toBe('total')
    expect(performance[1]?.accountId).toBe('snaptrade:brokerage')
    expect(performance[1]?.points).toEqual([
      { date: '2026-08-29', value: 100, netDeposits: 100, sp500: 100 },
      { date: '2026-08-30', value: 130, netDeposits: 120, sp500: 130 },
      { date: '2026-08-31', value: 140, netDeposits: 120, sp500: 143 },
    ])
  })

  it('counts signed transfers and excludes dividends, interest, and trades', () => {
    const performance = buildBrokeragePerformance(
      {
        accounts: [
          {
            id: 'brokerage',
            balance: { total: { amount: 155 } },
          },
        ],
        activities: {
          brokerage: [
            { type: 'TRANSFER', trade_date: '2026-08-27', amount: 30 },
            { type: 'TRANSFER', trade_date: '2026-08-28', amount: -10 },
            { type: 'EXTERNAL_ASSET_TRANSFER_IN', trade_date: '2026-08-29', amount: 25 },
            { type: 'EXTERNAL_ASSET_TRANSFER_OUT', trade_date: '2026-08-30', amount: 5 },
            { type: 'DIVIDEND', trade_date: '2026-08-30', amount: 9 },
            { type: 'INTEREST', trade_date: '2026-08-30', amount: 3 },
            { type: 'BUY', trade_date: '2026-08-30', amount: -50 },
          ],
        },
      },
      [],
      new Date('2026-08-31T12:00:00Z'),
      5,
    )

    expect(performance[1]?.points.map((point) => point.netDeposits)).toEqual([
      145, 135, 160, 155, 155,
    ])
  })

  it('sorts provider history before forward-filling missing dates', () => {
    const performance = buildBrokeragePerformance(
      {
        accounts: [{ id: 'brokerage', balance: { total: { amount: 140 } } }],
        activities: { brokerage: [] },
        balanceHistory: {
          brokerage: [
            { date: '2026-08-30', total_value: '130' },
            { date: '2026-08-28', total_value: '100' },
          ],
        },
      },
      [],
      new Date('2026-08-31T12:00:00Z'),
      4,
    )

    expect(performance[1]?.points.map((point) => point.value)).toEqual([100, 100, 130, 140])
  })
})
