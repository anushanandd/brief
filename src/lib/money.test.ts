import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import {
  cashFlowBreakdown,
  expectedMoneyEvents,
  isZelle,
  moneyKind,
  moneyRangeStart,
  moneySummary,
  scoreNewsArticle,
} from './money'
import type { FinanceSnapshot, Trade, Transaction } from './schema'

const transaction = (
  value: Partial<Transaction> & Pick<Transaction, 'id' | 'amount'>,
): Transaction => ({
  merchant: 'Merchant',
  category: 'Other',
  date: '2026-09-01',
  account: 'Checking',
  accountId: 'cash',
  classification: classification('expense'),
  pending: false,
  ...value,
})

const trade = (value: Partial<Trade> & Pick<Trade, 'id' | 'date'>): Trade => ({
  type: 'SELL',
  amount: 100,
  account: 'Brokerage',
  accountId: 'brokerage',
  ...value,
})

const snapshot = (transactions: Transaction[]): FinanceSnapshot => ({
  updatedAt: '2026-09-18T12:00:00Z',
  netWorth: 0,
  accounts: [],
  holdings: [],
  trades: [],
  transactions,
  spending: { monthTotal: 0, categories: [] },
  netWorthHistory: [],
  netWorthHistoryEstimated: false,
  benchmarkHistory: [],
  accountBalanceHistory: [],
  brokeragePerformance: [],
  accountMovements: [],
  possibleDuplicateAccounts: [],
  accountLinks: {},
})

describe('money model', () => {
  it('starts the week range on Monday', () => {
    expect(moneyRangeStart('week', '2026-09-18T12:00:00Z')).toBe('2026-09-14')
  })

  it('keeps reimbursements out of income and reduces attributed spending', () => {
    const data = snapshot([
      transaction({
        id: 'pay',
        amount: 2_000,
        category: 'Income',
        merchant: 'Payroll',
        classification: classification('income'),
      }),
      transaction({
        id: 'meal',
        amount: -120,
        category: 'Dining',
        classification: classification('expense'),
      }),
      transaction({
        id: 'repay',
        amount: 40,
        category: 'Reimbursement',
        merchant: 'Alex',
        classification: classification('reimbursement'),
      }),
      transaction({
        id: 'fee',
        amount: -5,
        category: 'Bank Fees',
        classification: classification('fee', { spending: true }),
      }),
    ])
    expect(moneySummary(data, 'month')).toMatchObject({
      grossIncome: 2_000,
      spending: 120,
      reimbursements: 40,
      personalSpending: 80,
      fees: 5,
      netCashFlow: 1_915,
    })
    expect(cashFlowBreakdown(data.transactions)).toMatchObject({
      sources: [
        { label: 'Payroll', value: 2_000, children: [{ id: 'source-transaction:pay' }] },
        { label: 'Alex', value: 40, children: [{ id: 'source-transaction:repay' }] },
      ],
      purchases: [{ label: 'Dining', value: 120, children: [{ id: 'purchase-transaction:meal' }] }],
      fees: 5,
    })
  })

  it('groups cash flow by exact income source and purchase category', () => {
    const transactions = [
      transaction({
        id: 'pay-1',
        amount: 1_500,
        category: 'Income',
        merchant: 'Acme Payroll',
        classification: classification('income'),
      }),
      transaction({
        id: 'pay-2',
        amount: 500,
        category: 'Income',
        merchant: 'Acme Payroll',
        classification: classification('income'),
      }),
      transaction({
        id: 'interest',
        amount: 25,
        category: 'Interest',
        merchant: 'Marcus',
        classification: classification('interest'),
      }),
      transaction({
        id: 'meal-1',
        amount: -80,
        category: 'Dining',
        merchant: 'Cafe',
        classification: classification('expense'),
      }),
      transaction({
        id: 'meal-2',
        amount: -20,
        category: 'Dining',
        merchant: 'Deli',
        classification: classification('expense'),
      }),
      transaction({
        id: 'flight',
        amount: -300,
        category: 'Travel',
        merchant: 'Airline',
        classification: classification('expense'),
      }),
    ]

    expect(cashFlowBreakdown(transactions)).toMatchObject({
      sources: [
        {
          label: 'Acme Payroll',
          value: 2_000,
          children: [
            { id: 'source-transaction:pay-1', value: 1_500 },
            { id: 'source-transaction:pay-2', value: 500 },
          ],
        },
        { label: 'Marcus', value: 25 },
      ],
      purchases: [
        {
          label: 'Travel',
          value: 300,
          children: [{ id: 'purchase-transaction:flight', label: 'Airline' }],
        },
        {
          label: 'Dining',
          value: 100,
          children: [
            { id: 'purchase-transaction:meal-1', label: 'Cafe' },
            { id: 'purchase-transaction:meal-2', label: 'Deli' },
          ],
        },
      ],
    })
  })

  it('scopes estimated realized P/L to the selected period and preserves missing coverage', () => {
    const data = snapshot([])
    data.trades = [
      trade({ id: 'september-sale', date: '2026-09-05', estimatedRealizedGain: 40 }),
      trade({ id: 'august-sale', date: '2026-08-05', estimatedRealizedGain: -10 }),
    ]

    expect(moneySummary(data, 'month').estimatedRealizedGain).toBe(40)
    expect(moneySummary(data, 'quarter').estimatedRealizedGain).toBe(30)

    data.trades.push(trade({ id: 'uncovered-sale', date: '2026-09-10' }))
    expect(moneySummary(data, 'month').estimatedRealizedGain).toBeNull()
  })

  it('detects only high-confidence recurring activity', () => {
    const data = snapshot([
      transaction({
        id: 'rent-1',
        amount: -1_000,
        merchant: 'Rent',
        date: '2026-07-01',
        classification: classification('expense'),
      }),
      transaction({
        id: 'rent-2',
        amount: -1_000,
        merchant: 'Rent',
        date: '2026-08-01',
        classification: classification('expense'),
      }),
      transaction({
        id: 'rent-3',
        amount: -1_000,
        merchant: 'Rent',
        date: '2026-09-01',
        classification: classification('expense'),
      }),
    ])
    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      title: 'Rent',
      amount: -1_000,
      confidence: 'High',
    })
  })

  it('always counts positive Zelle payment titles as income', () => {
    const zelle = transaction({
      id: 'zelle-payment',
      amount: 75,
      merchant: 'ZELLE PAYMENT',
      category: 'Transfer In',
      counterpartyType: 'payment_app',
      classification: classification('income', { zelle: true }),
    })
    const data = snapshot([zelle])

    expect(isZelle(zelle)).toBe(true)
    expect(moneyKind(zelle)).toBe('income')
    expect(moneySummary(data, 'month').income).toBe(75)
    expect(moneyKind({ ...zelle, category: 'Transfer' })).toBe('income')
    expect(moneyKind({ ...zelle, category: 'Reimbursement' })).toBe('income')
  })

  it('counts the identified brokerage transfer title as income', () => {
    const brokerageTransfer = transaction({
      id: 'brokerage-transfer',
      amount: 500,
      merchant: 'TRANSFER MONEY FROM BROKERAGE XXXXX8549 Reference Number: MCK1SOY78',
      category: 'Transfer In',
      classification: classification('income', { brokerageIncomeTransfer: true }),
    })

    expect(moneyKind(brokerageTransfer)).toBe('income')
    expect(moneySummary(snapshot([brokerageTransfer]), 'month').income).toBe(500)
  })

  it('does not emit medium-confidence recurring activity', () => {
    const data = snapshot([
      transaction({
        id: 'rent-1',
        amount: -1_000,
        merchant: 'Rent',
        date: '2026-08-01',
        classification: classification('expense'),
      }),
      transaction({
        id: 'rent-2',
        amount: -1_000,
        merchant: 'Rent',
        date: '2026-09-01',
        classification: classification('expense'),
      }),
    ])

    expect(expectedMoneyEvents(data)).toEqual([])
  })

  it('ranks fresh material news for a large holding above an unrelated story', () => {
    const data: Pick<FinanceSnapshot, 'holdings'> = {
      holdings: [
        {
          ticker: 'AAPL',
          name: 'Apple',
          accountId: 'brokerage',
          shares: 1,
          price: 800,
          value: 800,
          costBasis: 700,
          unrealizedGain: 100,
          dailyChangePct: 1,
          weeklyChangePct: 2,
          weeklyReferencePrice: 780,
          weeklyReferenceDate: '2026-09-11',
          totalChangePct: 14.29,
          color: '#fff',
        },
      ],
    }
    const createdAt = '2026-09-18T11:00:00Z'
    const relevant = scoreNewsArticle(
      {
        headline: 'Apple raises earnings guidance',
        summary: '',
        source: 'Wire',
        url: 'https://finance.yahoo.com/relevant',
        createdAt,
        symbols: ['AAPL'],
      },
      data,
      Date.parse('2026-09-18T12:00:00Z'),
    )
    const unrelated = scoreNewsArticle(
      {
        headline: 'Overseas market update',
        summary: '',
        source: 'Wire',
        url: 'https://finance.yahoo.com/unrelated',
        createdAt,
        symbols: ['OTHER'],
      },
      data,
      Date.parse('2026-09-18T12:00:00Z'),
    )

    expect(relevant.score).toBeGreaterThan(unrelated.score)
    expect(relevant.reason).toBe('Material company event')
  })
})
