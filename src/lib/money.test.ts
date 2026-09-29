import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import {
  cashFlowBreakdown,
  expectedMoneyEvents,
  isZelle,
  recurringMoneyKey,
  moneyKind,
  moneyRangeStart,
  moneySummary,
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

  it('shows a recent continuous recurring series with its evidence', () => {
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
      frequency: 'Monthly',
      observations: 3,
      lastSeen: '2026-09-01',
      date: '2026-10-01',
    })
  })

  it('uses detailed descriptions for generic banking streams', () => {
    const data = snapshot(
      ['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
        transaction({
          id: `deposit-${index}`,
          amount: 2_000,
          merchant: 'ACH Electronic Credit',
          description: 'ACME Payroll Direct Deposit',
          category: 'Income',
          date,
          classification: classification('income'),
        }),
      ),
    )

    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      title: 'ACME Payroll Direct Deposit',
      kind: 'income',
      date: '2026-10-01',
    })
  })

  it('detects variable monthly card payments from a bank account', () => {
    const data = snapshot(
      ['2026-06-04', '2026-07-08', '2026-08-08', '2026-09-08'].map((date, index) =>
        transaction({
          id: `amex-payment-${index}`,
          amount: -[5_114.82, 1_800, 2_412.85, 323.73][index],
          merchant: 'American Express',
          description: 'Autopay payment received – thank you',
          category: 'Loan Payments',
          website: 'https://americanexpress.com',
          date,
          classification: classification('transfer'),
        }),
      ),
    )

    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      title: 'Autopay payment received – thank you',
      kind: 'payment',
      frequency: 'Monthly',
      amount: -2_412.85,
      amountLow: 323.73,
      amountHigh: 5_114.82,
      observations: 4,
      lastSeen: '2026-09-08',
      date: '2026-10-08',
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

  it('requires at least three observations for a monthly series', () => {
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

  it('does not revive old payroll or rent after a newly linked account supplies history', () => {
    const data = snapshot([
      ...['2024-06-07', '2024-06-21', '2024-07-05', '2024-07-19'].map((date, index) =>
        transaction({
          id: `intern-${index}`,
          accountId: 'plaid:new-account',
          merchant: 'Forge Payroll',
          amount: 2_213.48,
          date,
        }),
      ),
      ...['2025-01-14', '2025-02-14', '2025-03-14', '2025-04-14'].map((date, index) =>
        transaction({
          id: `old-rent-${index}`,
          accountId: 'plaid:new-account',
          merchant: 's359ao1 Rent 234642010',
          amount: -1_234,
          date,
        }),
      ),
    ])
    data.accounts = [
      {
        id: 'plaid:new-account',
        name: 'Checking',
        institution: 'Example',
        type: 'cash',
        value: 0,
        balanceFetchedAt: data.updatedAt,
      },
    ]

    expect(expectedMoneyEvents(data)).toEqual([])
  })

  it('does not skip a missed occurrence or trust stale account updates', () => {
    const data = snapshot(
      ['2026-07-07', '2026-08-07', '2026-09-07'].map((date, index) =>
        transaction({
          id: `bill-${index}`,
          accountId: 'plaid:checking',
          merchant: 'Utility',
          amount: -100 - index * 5,
          date,
        }),
      ),
    )
    data.accounts = [
      {
        id: 'plaid:checking',
        name: 'Checking',
        institution: 'Example',
        type: 'cash',
        value: 0,
        balanceFetchedAt: data.updatedAt,
      },
    ]
    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      date: '2026-10-07',
      amountLow: 100,
      amountHigh: 110,
    })
    data.updatedAt = '2026-10-12T12:00:00Z'
    data.accounts[0].balanceFetchedAt = data.updatedAt
    expect(expectedMoneyEvents(data, '2026-10-12')[0]).toMatchObject({
      date: '2026-10-07',
      state: 'awaiting-post',
    })
    data.updatedAt = '2026-10-16T12:00:00Z'
    data.accounts[0].balanceFetchedAt = data.updatedAt
    expect(expectedMoneyEvents(data, '2026-10-16')).toEqual([])
    data.accounts[0].balanceFetchedAt = '2026-09-01T12:00:00Z'
    expect(expectedMoneyEvents(data)).toEqual([])
  })

  it('handles twice-monthly and month-end schedules', () => {
    const twiceMonthly = snapshot(
      ['2026-08-01', '2026-08-15', '2026-09-01', '2026-09-15'].map((date, index) =>
        transaction({
          id: `pay-${index}`,
          merchant: 'Paycheck',
          amount: 800 + index * 25,
          date,
          classification: classification('income'),
        }),
      ),
    )
    const [event] = expectedMoneyEvents(twiceMonthly)
    expect(event.id).toBe('{"version":3,"account":"cash","direction":"in","merchant":"paycheck"}')
    expect(event).toMatchObject({
      frequency: 'Twice monthly',
      date: '2026-10-01',
      amountLow: 800,
      amountHigh: 875,
    })

    const monthEnd = snapshot(
      ['2026-06-30', '2026-07-31', '2026-08-31'].map((date, index) =>
        transaction({ id: `lease-${index}`, merchant: 'Lease', amount: -1_000, date }),
      ),
    )
    expect(expectedMoneyEvents(monthEnd)[0]).toMatchObject({
      frequency: 'Monthly',
      date: '2026-09-30',
    })
  })

  it('keeps different account IDs and payment directions separate', () => {
    const examples = ['plaid:123', 'plaid:456'].flatMap((accountId) =>
      ['2026-07-01', '2026-08-01'].map((date, index) =>
        transaction({
          id: `${accountId}-${index}`,
          accountId,
          merchant: 'Rent 42',
          amount: -1000,
          date,
        }),
      ),
    )
    expect(expectedMoneyEvents(snapshot(examples))).toEqual([])
  })

  it('reactivates a new continuous run but does not call varied shopping recurring', () => {
    const old = ['2024-06-01', '2024-07-01', '2024-08-01'].map((date, index) =>
      transaction({ id: `old-${index}`, merchant: 'Rent', amount: -900, date }),
    )
    const recent = ['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
      transaction({ id: `new-${index}`, merchant: 'Rent', amount: -1_000, date }),
    )
    expect(expectedMoneyEvents(snapshot([...old, ...recent]))[0]).toMatchObject({
      observations: 3,
      amount: -1_000,
      date: '2026-10-01',
    })
    const shopping = ['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
      transaction({
        id: `shopping-${index}`,
        merchant: 'Market',
        amount: -[60, 110, 180][index],
        date,
      }),
    )
    expect(expectedMoneyEvents(snapshot(shopping))).toEqual([])
  })

  it('keeps short merchant numbers distinct and uses a stable website when names change', () => {
    const transactions = [
      ...['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
        transaction({
          id: `plan-1-${index}`,
          merchant: `Video Plan 1 ref ${index}`,
          website: 'https://plan-one.example/account',
          amount: -10,
          date,
        }),
      ),
      ...['2026-07-15', '2026-08-15', '2026-09-15'].map((date, index) =>
        transaction({
          id: `plan-2-${index}`,
          merchant: `Video Plan 2 ref ${index}`,
          website: 'https://plan-two.example/account',
          amount: -20,
          date,
        }),
      ),
    ]

    expect(expectedMoneyEvents(snapshot(transactions)).map(({ amount }) => amount)).toEqual([
      -10, -20,
    ])
    expect(recurringMoneyKey(transaction({ id: 'one', merchant: 'Plan 1', amount: -1 }))).not.toBe(
      recurringMoneyKey(transaction({ id: 'two', merchant: 'Plan 2', amount: -1 })),
    )
  })

  it('suppresses habitual purchases unless the transaction identifies a subscription', () => {
    const groceries = ['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
      transaction({
        id: `grocery-${index}`,
        merchant: 'Neighborhood Market',
        category: 'Groceries',
        amount: -75,
        date,
      }),
    )
    expect(expectedMoneyEvents(snapshot(groceries))).toEqual([])

    expect(
      expectedMoneyEvents(
        snapshot(
          groceries.map((entry) => ({
            ...entry,
            description: 'Monthly grocery membership',
          })),
        ),
      )[0],
    ).toMatchObject({ kind: 'subscription', amount: -75 })
  })

  it('detects monthly Platinum credits by benefit instead of merging the card issuer', () => {
    const credits = [
      ...['2026-07-05', '2026-08-05', '2026-09-05'].map((date, index) =>
        transaction({
          id: `entertainment-credit-${index}`,
          merchant: 'American Express',
          description: 'Digital entertainment monthly credit',
          category: 'General Merchandise',
          website: 'https://americanexpress.com',
          amount: [20, 25, 25][index],
          date,
          classification: classification('reimbursement', { credit: true }),
        }),
      ),
      ...['2026-07-10', '2026-08-10', '2026-09-10'].map((date, index) =>
        transaction({
          id: `walmart-credit-${index}`,
          merchant: 'American Express',
          description: 'Walmart+ monthly credit',
          category: 'General Merchandise',
          website: 'https://americanexpress.com',
          amount: 12.95,
          date,
          classification: classification('reimbursement', { credit: true }),
        }),
      ),
    ]

    expect(
      expectedMoneyEvents(snapshot(credits)).map((event) => ({
        title: event.title,
        kind: event.kind,
        amount: event.amount,
        date: event.date,
        observations: event.observations,
      })),
    ).toEqual([
      {
        title: 'Digital entertainment',
        kind: 'credit',
        amount: 25,
        date: '2026-10-05',
        observations: 3,
      },
      {
        title: 'Walmart+',
        kind: 'credit',
        amount: 12.95,
        date: '2026-10-10',
        observations: 3,
      },
    ])
  })

  it('matches one pending transaction without learning from it', () => {
    const posted = ['2026-06-15', '2026-07-15', '2026-08-15'].map((date, index) =>
      transaction({ id: `phone-${index}`, merchant: 'Phone Utility', amount: -100, date }),
    )
    const data = snapshot([
      ...posted,
      transaction({
        id: 'phone-pending',
        merchant: 'Phone Utility',
        amount: -102,
        date: '2026-09-15',
        pending: true,
      }),
    ])

    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      amount: -102,
      observations: 3,
      state: 'pending',
      matchedTransactionId: 'phone-pending',
    })
  })

  it('separates amount series and stitches a sustained price change', () => {
    const separate = [
      ...['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
        transaction({ id: `small-${index}`, merchant: 'Media', amount: -10, date }),
      ),
      ...['2026-06-15', '2026-07-15', '2026-08-15'].map((date, index) =>
        transaction({ id: `large-${index}`, merchant: 'Media', amount: -30, date }),
      ),
    ]
    expect(
      expectedMoneyEvents(snapshot(separate))
        .map(({ amount }) => amount)
        .toSorted((left, right) => left - right),
    ).toEqual([-30, -10])

    const changed = [
      ...['2026-04-01', '2026-05-01', '2026-06-01'].map((date, index) =>
        transaction({ id: `old-price-${index}`, merchant: 'Cloud Storage', amount: -10, date }),
      ),
      ...['2026-07-01', '2026-08-01', '2026-09-01'].map((date, index) =>
        transaction({ id: `new-price-${index}`, merchant: 'Cloud Storage', amount: -12, date }),
      ),
    ]
    expect(expectedMoneyEvents(snapshot(changed))).toMatchObject([
      { amount: -12, amountLow: 10, amountHigh: 12, observations: 6, date: '2026-10-01' },
    ])
  })

  it('matures annual bills after two observations and shifts bank schedules to business days', () => {
    const annual = snapshot(
      ['2024-10-15', '2025-10-15'].map((date, index) =>
        transaction({
          id: `insurance-${index}`,
          merchant: 'Home Insurance',
          category: 'Insurance',
          amount: -800,
          date,
        }),
      ),
    )
    expect(expectedMoneyEvents(annual)[0]).toMatchObject({
      frequency: 'Yearly',
      date: '2026-10-15',
      observations: 2,
    })

    const rent = snapshot(
      ['2026-07-31', '2026-08-31', '2026-09-30'].map((date, index) =>
        transaction({ id: `rent-${index}`, merchant: 'Rent', amount: -1_000, date }),
      ),
    )
    rent.updatedAt = '2026-10-10T12:00:00Z'
    expect(expectedMoneyEvents(rent)[0]).toMatchObject({ date: '2026-11-02' })
  })

  it('does not count a federal holiday against the late-posting grace period', () => {
    const data = snapshot(
      ['2026-08-25', '2026-09-25', '2026-10-25'].map((date, index) =>
        transaction({
          id: `subscription-${index}`,
          merchant: 'Software',
          category: 'Subscription',
          amount: -20,
          date,
        }),
      ),
    )
    data.updatedAt = '2026-12-03T12:00:00Z'
    expect(expectedMoneyEvents(data)[0]).toMatchObject({
      date: '2026-11-25',
      state: 'awaiting-post',
    })
  })
})
