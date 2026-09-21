import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import type { Account, Transaction } from './schema'
import {
  buildPlatinumBenefitTracker,
  buildPlatinumBenefitHistory,
  buildMonthlySpendingHistory,
  buildSpendingView,
  formatActivityDate,
  getPlatinumBenefitActivity,
  resolveSpendingAccount,
  sortTransactionsByRecency,
  spendingCategoryColor,
  spendingMonthDirectionForKey,
  spendingMonthReference,
  spendingPeriodLabel,
  spendingPeriodForKey,
  spendingPeriodReference,
  transactionDateKey,
  weeklySpendingCategoryPerformance,
} from './spending'

const transaction = (overrides: Partial<Transaction>): Transaction => ({
  id: 'transaction',
  merchant: 'Merchant',
  category: 'Shopping',
  date: '2026-09-01',
  amount: -10,
  account: 'Platinum Card',
  classification: classification('expense'),
  pending: false,
  ...overrides,
})

const spendingMonthShortcut = (key: string, metaKey = false) =>
  spendingMonthDirectionForKey({
    altKey: false,
    ctrlKey: false,
    key,
    metaKey,
    shiftKey: false,
  })

describe('spending view', () => {
  it('maps spending ranges and moves through calendar months', () => {
    expect(['m', 'q', 'y', 'a'].map(spendingPeriodForKey)).toEqual([1, 3, 12, 0])
    expect(spendingPeriodForKey('w')).toBeUndefined()
    expect(spendingMonthReference('2026-09-03T12:00:00Z', 0)).toBe('2026-09-03T12:00:00Z')
    expect(spendingMonthReference('2026-09-03T12:00:00Z', -1)).toBe('2026-08-31T12:00:00Z')
    expect(spendingMonthReference('2026-09-03T12:00:00Z', -9)).toBe('2025-12-31T12:00:00Z')
    expect(
      ([1, 3, 12, 0] as const).map((period) => spendingPeriodLabel(period, 0, 'September 2026')),
    ).toEqual(['This month', 'This quarter', 'This year', 'All time'])
    expect(spendingPeriodLabel(1, -1, 'August 2026')).toBe('August 2026')
    expect(spendingPeriodLabel(3, -1, 'August 2026')).toBe('Quarter through August 2026')
    expect(spendingPeriodLabel(12, -1, 'August 2026')).toBe('Year through August 2026')
    expect(spendingPeriodLabel(0, -1, 'August 2026')).toBe('All time through August 2026')
    expect(spendingPeriodLabel(1, 0, 'Sep 5, 2026', 'statement')).toBe('Current statement')
    expect(spendingPeriodLabel(3, 0, 'Sep 5, 2026', 'statement')).toBe('3 statements')
    expect(spendingPeriodLabel(1, -1, 'Sep 5, 2026', 'statement')).toBe(
      'Statement ending Sep 5, 2026',
    )
  })

  it('moves between months with plain or Command arrow shortcuts', () => {
    expect(spendingMonthShortcut('ArrowLeft')).toBe(-1)
    expect(spendingMonthShortcut('ArrowRight', true)).toBe(1)
    expect(spendingMonthShortcut('ArrowLeft', true)).toBe(-1)
    expect(spendingMonthShortcut('m', true)).toBeUndefined()
    expect(
      spendingMonthDirectionForKey({
        altKey: false,
        ctrlKey: true,
        key: 'ArrowRight',
        metaKey: true,
        shiftKey: false,
      }),
    ).toBeUndefined()
  })

  it('filters month and all-time views through the selected month', () => {
    const transactions = [
      transaction({ id: 'january', date: '2026-01-15', amount: -10 }),
      transaction({ id: 'august', date: '2026-08-15', amount: -20 }),
      transaction({ id: 'september', date: '2026-09-01', amount: -30 }),
    ]
    const selectedMonth = spendingMonthReference('2026-09-03T12:00:00Z', -1)

    expect(buildSpendingView(transactions, '2026-09-03T12:00:00Z', 1, selectedMonth).total).toBe(20)
    expect(buildSpendingView(transactions, '2026-09-03T12:00:00Z', 0, selectedMonth).total).toBe(30)
  })

  it('compares category spending across adjacent seven-day windows', () => {
    const performance = weeklySpendingCategoryPerformance(
      [
        transaction({ id: 'current-shopping', date: '2026-09-18', amount: -50 }),
        transaction({ id: 'previous-shopping', date: '2026-09-11', amount: -100 }),
        transaction({
          id: 'new-food',
          date: '2026-09-17',
          category: 'Food',
          amount: -20,
          classification: classification('expense'),
        }),
      ],
      '2026-09-18T12:00:00Z',
    )
    expect(performance.get('Shopping')).toBe(50)
    expect(performance.get('Food')).toBe(-100)
  })

  it('uses a stable posted autopay cadence as statement boundaries', () => {
    const transactions = [
      transaction({
        id: 'july-pay',
        date: '2026-07-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 90,
        classification: classification('transfer', { mark: 'payment' }),
      }),
      transaction({ id: 'july-spend', date: '2026-07-02', amount: -40 }),
      transaction({
        id: 'august-pay',
        date: '2026-08-06',
        postedOn: '2026-08-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 100,
        classification: classification('transfer', { mark: 'payment' }),
      }),
      transaction({ id: 'august-spend', date: '2026-08-03', amount: -60 }),
      transaction({
        id: 'september-pay',
        date: '2026-09-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 110,
        classification: classification('transfer', { mark: 'payment' }),
      }),
      transaction({ id: 'current-spend', date: '2026-09-12', amount: -25 }),
    ]

    const current = buildSpendingView(
      transactions,
      '2026-09-14T12:00:00Z',
      1,
      '2026-09-14T12:00:00Z',
      'statement',
    )
    expect(current).toMatchObject({
      start: '2026-08-24',
      end: '2026-09-14',
      total: 25,
      startingBalance: 110,
      statementBalance: 25,
      periodBasis: 'statement',
    })
    expect(current.trend.at(-1)?.current).toBe(25)
    expect(current.activityMarkers).toEqual([
      {
        id: 'september-pay',
        date: '2026-09-07',
        sequence: 0,
        count: 1,
        value: 0,
        direction: 'credit',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 110,
      },
      {
        id: 'current-spend',
        date: '2026-09-12',
        sequence: 0,
        count: 1,
        value: 25,
        direction: 'expense',
        merchant: 'Merchant',
        amount: 25,
      },
    ])
    const anchored = buildSpendingView(
      transactions,
      '2026-09-14T12:00:00Z',
      1,
      '2026-09-14T12:00:00Z',
      'statement',
      42,
    )
    expect(anchored).toMatchObject({ startingBalance: 127, statementBalance: 42 })
    expect(anchored.trend.at(-1)?.current).toBe(42)
    expect(
      buildSpendingView(
        transactions,
        '2026-09-14T12:00:00Z',
        3,
        '2026-09-14T12:00:00Z',
        'statement',
      ),
    ).toMatchObject({ start: '2026-06-23', end: '2026-09-14', total: 125 })

    const priorReference = spendingPeriodReference(transactions, '2026-09-14T12:00:00Z', -1)
    expect(priorReference).toBe('2026-08-23T12:00:00Z')
    expect(
      buildSpendingView(transactions, '2026-09-14T12:00:00Z', 1, priorReference, 'statement'),
    ).toMatchObject({
      start: '2026-07-24',
      end: '2026-08-23',
      total: 60,
      periodBasis: 'statement',
    })
  })

  it('falls back to calendar periods without consistent autopay evidence', () => {
    const transactions = [
      transaction({ id: 'spend', date: '2026-08-03', amount: -60 }),
      transaction({
        id: 'single-pay',
        date: '2026-08-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 60,
        classification: classification('transfer', { mark: 'payment' }),
      }),
    ]
    const view = buildSpendingView(
      transactions,
      '2026-08-14T12:00:00Z',
      1,
      '2026-08-14T12:00:00Z',
      'statement',
    )

    expect(view).toMatchObject({ start: '2026-08-01', periodBasis: 'calendar', total: 60 })
    expect(spendingPeriodReference(transactions, '2026-08-14T12:00:00Z', -1)).toBe(
      '2026-07-31T12:00:00Z',
    )
  })

  it('keeps category colors stable across ranking changes', () => {
    expect(spendingCategoryColor('Dining')).toBe('var(--spending-food)')
    expect(spendingCategoryColor('Travel')).toBe('var(--spending-travel)')
    expect(spendingCategoryColor('Dining')).toBe(spendingCategoryColor('Food & drink'))
    expect(spendingCategoryColor('General Merchandise')).toBe('var(--spending-shopping)')
    expect(spendingCategoryColor('General Merchandise')).not.toBe(
      spendingCategoryColor('General Services'),
    )
  })

  it('clamps previous-month comparisons before the current month, including leap years', () => {
    for (const year of [2024, 2025, 2026]) {
      const result = buildSpendingView(
        [
          transaction({ id: 'feb', date: `${year}-02-28`, amount: -100 }),
          transaction({ id: 'march', date: `${year}-03-01`, amount: -50 }),
        ],
        `${year}-03-31T12:00:00Z`,
        1,
      )
      expect(result.previousTotal).toBe(100)
      expect(result.total).toBe(50)
    }
  })

  it('does not count merchant refunds or generic Credit categories as benefits', () => {
    const refund = transaction({
      id: 'refund',
      merchant: 'Walmart',
      amount: 100,
      date: '2026-09-01',
      classification: classification('other'),
    })
    const matched = buildPlatinumBenefitTracker(
      [
        refund,
        transaction({
          id: 'generic',
          merchant: 'Walmart+',
          category: 'Credit',
          amount: 12.95,
          classification: classification('other'),
        }),
        transaction({
          id: 'refund-description',
          merchant: 'Walmart+ credit refund',
          amount: 12.95,
          classification: classification('other', { mark: 'refund' }),
        }),
      ],
      '2026-09-03T12:00:00Z',
    ).find(({ id }) => id === 'walmart-plus')!
    expect(matched.creditedAmount).toBe(0)
    expect(matched.remainingAmount).toBeNull()
    expect(matched.status).toBe('unused')
  })

  it('nets identifiable posted credits and reversals, excluding pending and rejected matches', () => {
    const transactions = [
      transaction({
        id: 'credit',
        merchant: 'AMEX Walmart+ Credit',
        amount: 13.73,
        classification: classification('other'),
      }),
      transaction({
        id: 'reversal',
        merchant: 'AMEX Walmart+ Credit reversal',
        amount: -13.73,
        classification: classification('expense', { mark: 'refund' }),
      }),
      transaction({
        id: 'pending',
        merchant: 'AMEX Walmart+ Credit',
        amount: 13.73,
        classification: classification('other'),
        pending: true,
      }),
      transaction({
        id: 'rejected',
        merchant: 'AMEX Walmart+ Credit',
        amount: 13.73,
        benefitConfirmed: false,
        classification: classification('other'),
      }),
      transaction({
        id: 'purchase',
        merchant: 'Walmart+ membership',
        amount: -13.73,
        classification: classification('expense'),
      }),
    ]
    const reference = '2026-09-03T12:00:00Z'
    const benefit = buildPlatinumBenefitTracker(transactions, reference).find(
      ({ id }) => id === 'walmart-plus',
    )!
    expect(benefit).toMatchObject({ creditedAmount: 0, remainingAmount: null, status: 'matched' })
    expect(
      buildPlatinumBenefitHistory(getPlatinumBenefitActivity(transactions, reference), 2026),
    ).toMatchObject({ creditedAmount: 0, creditCount: 1 })
    expect(
      buildPlatinumBenefitTracker([transactions[0]], reference).find(
        ({ id }) => id === 'walmart-plus',
      ),
    ).toMatchObject({ creditedAmount: 13.73, remainingAmount: null, status: 'credited' })
  })

  it('resets current calendar windows independently of stale saved transactions', () => {
    const saved = '2026-09-30T12:00:00Z'
    const transactions = [
      transaction({
        merchant: 'Resy credit',
        date: 'Sep 29',
        amount: 100,
        classification: classification('other'),
      }),
    ]
    const prior = buildPlatinumBenefitTracker(transactions, saved, saved).find(
      ({ id }) => id === 'resy',
    )!
    const current = buildPlatinumBenefitTracker(transactions, '2026-10-01T12:00:00Z', saved).find(
      ({ id }) => id === 'resy',
    )!
    expect(prior).toMatchObject({ remainingAmount: 0, windowEnd: '2026-09-30' })
    expect(current).toMatchObject({
      creditedAmount: 0,
      windowStart: '2026-10-01',
      windowEnd: '2026-12-31',
      lastCredit: { amount: 100, date: 'Sep 29, 2026' },
    })
    expect(
      buildPlatinumBenefitTracker(transactions, '2027-10-01T12:00:00Z', saved).find(
        ({ id }) => id === 'resy',
      )?.creditedAmount,
    ).toBe(0)
  })

  it('uses the hotel contractual timezone at a half-year boundary', () => {
    const benefit = buildPlatinumBenefitTracker([], '2026-07-01T04:00:00Z').find(
      ({ id }) => id === 'hotel',
    )!
    expect(benefit).toMatchObject({
      windowStart: '2026-01-01',
      windowEnd: '2026-06-30',
      daysRemaining: 0,
    })
    expect(
      buildPlatinumBenefitTracker([], '2026-07-01T06:00:00Z').find(({ id }) => id === 'hotel'),
    ).toMatchObject({ windowStart: '2026-07-01', windowEnd: '2026-12-31' })
  })
  it('uses only the explicitly selected credit account', () => {
    const accounts: Account[] = [
      { id: 'cash', name: 'Checking', institution: 'Bank', type: 'cash', value: 100 },
      { id: 'card-1', name: 'One', institution: 'Bank', type: 'credit', value: -10 },
      { id: 'card-2', name: 'Two', institution: 'Bank', type: 'credit', value: -20 },
    ]

    expect(resolveSpendingAccount(accounts, 'card-2')?.id).toBe('card-2')
    expect(resolveSpendingAccount(accounts, '')).toBeUndefined()
    expect(resolveSpendingAccount(accounts.slice(0, 2), '')).toBeUndefined()
  })

  it('compares matching elapsed periods and excludes transfers', () => {
    const view = buildSpendingView(
      [
        transaction({ id: 'current', amount: -100 }),
        transaction({ id: 'pending', date: '2026-09-02', amount: -25, pending: true }),
        transaction({
          id: 'payment',
          date: '2026-09-03',
          category: 'Loan Payments',
          amount: 40,
          classification: classification('transfer', { mark: 'payment' }),
        }),
        transaction({
          id: 'transfer',
          category: 'Transfer Out',
          amount: -500,
          classification: classification('transfer'),
        }),
        transaction({ id: 'previous', date: '2026-08-01', amount: -50 }),
        transaction({ id: 'outside-comparison', date: '2026-08-20', amount: -900 }),
      ],
      '2026-09-03T12:00:00Z',
      1,
    )

    expect(view.total).toBe(125)
    expect(view.pendingTotal).toBe(25)
    expect(view.previousTotal).toBe(50)
    expect(view.percentChange).toBe(150)
    expect(view.trend.map(({ current }) => current)).toEqual([100, 125, 85])
    expect(view.trend.at(-1)).toMatchObject({ date: '2026-09-03', previous: 50 })
  })

  it('groups posted card spending into recent calendar months', () => {
    expect(
      buildMonthlySpendingHistory(
        [
          transaction({ id: 'july', date: '2026-07-30', amount: -30 }),
          transaction({ id: 'august', date: '2026-09-01', postedOn: '2026-08-31', amount: -20 }),
          transaction({ id: 'september', date: '2026-09-02', amount: -10 }),
          transaction({
            id: 'payment',
            category: 'Payment',
            amount: -500,
            classification: classification('other'),
          }),
        ],
        '2026-09-03T12:00:00Z',
        3,
      ),
    ).toEqual([
      { month: '2026-07', value: 30 },
      { month: '2026-08', value: 20 },
      { month: '2026-09', value: 10 },
    ])
  })

  it('uses the computer calendar day instead of UTC month boundaries', () => {
    const view = buildSpendingView(
      [transaction({ date: '2026-08-31', amount: -25 })],
      '2026-09-01T00:30:00Z',
      1,
    )
    expect(view.end).toBe('2026-08-31')
    expect(view.total).toBe(25)
  })

  it('resolves short statement dates relative to the snapshot year', () => {
    expect(transactionDateKey('Aug 31', '2026-09-03T12:00:00Z')).toBe('2026-08-31')
    expect(transactionDateKey('Dec 31', '2026-01-03T12:00:00Z')).toBe('2025-12-31')
  })

  it('formats activity dates with the year last', () => {
    const reference = '2026-09-08T12:00:00Z'
    expect(formatActivityDate('2026-09-05', reference)).toBe('Sep 5, 2026')
    expect(formatActivityDate('Sep 5', reference)).toBe('Sep 5, 2026')
  })

  it('sorts recent transactions across statement months', () => {
    const sorted = sortTransactionsByRecency(
      [
        transaction({ id: 'july', date: '2026-07-30' }),
        transaction({ id: 'september', date: '2026-09-01' }),
        transaction({ id: 'august', date: 'Aug 31' }),
      ],
      '2026-09-03T12:00:00Z',
    )

    expect(sorted.map(({ id }) => id)).toEqual(['september', 'august', 'july'])
  })

  it('tracks Platinum activity inside each benefit reset window', () => {
    const benefits = buildPlatinumBenefitTracker(
      [
        transaction({
          id: 'uber',
          merchant: 'Uber',
          date: '2026-09-02',
          classification: classification('expense'),
        }),
        transaction({
          id: 'entertainment-credit',
          merchant: 'Platinum Digital Entertainment Credit',
          description: 'Platinum Digital Entertainment Credit',
          amount: 15.99,
          date: '2026-08-31',
          classification: classification('other'),
        }),
        transaction({
          id: 'hotel-credit',
          merchant: 'Platinum Hotel Credit',
          description: 'Platinum Hotel Credit',
          amount: 300,
          date: '2026-06-30',
          classification: classification('other'),
        }),
        transaction({
          id: 'clear-credit',
          merchant: 'AMEX CLEAR PLUS CREDIT',
          description: 'AMEX CLEAR PLUS CREDIT',
          amount: 209,
          date: '2026-05-08',
          classification: classification('other'),
        }),
        transaction({
          id: 'airline-credit',
          merchant: 'AMEX Airline Fee Reimbursement',
          description: 'AMEX Airline Fee Reimbursement',
          amount: 50,
          date: '2026-06-16',
          classification: classification('reimbursement'),
        }),
      ],
      '2026-09-03T12:00:00Z',
    )

    expect(benefits.find(({ id }) => id === 'uber-cash')).toMatchObject({
      status: 'external',
      estimatedCreditAmount: 15,
      reset: 'Sep 30',
    })
    expect(benefits.find(({ id }) => id === 'clear')).toMatchObject({
      status: 'credited',
      creditedAmount: 209,
    })
    expect(benefits.find(({ id }) => id === 'airline-fee')).toMatchObject({
      status: 'credited',
      creditedAmount: 50,
    })
    expect(benefits.find(({ id }) => id === 'digital-entertainment')).toMatchObject({
      status: 'unused',
      creditedAmount: 0,
      lastCredit: { amount: 15.99, date: 'Aug 31, 2026' },
    })
    expect(benefits.find(({ id }) => id === 'hotel')?.lastCredit).toEqual({
      amount: 300,
      date: 'Jun 30, 2026',
    })
    expect(benefits.find(({ id }) => id === 'resy')?.status).toBe('unused')
    expect(benefits.find(({ id }) => id === 'resy')).toMatchObject({
      remainingAmount: 100,
      daysRemaining: 27,
    })
    const calendar = benefits.filter(
      ({ cadence }) => cadence !== 'renewal' && cadence !== 'purchase',
    )
    expect(calendar.map(({ windowEnd }) => windowEnd)).toEqual(
      calendar.map(({ windowEnd }) => windowEnd).toSorted(),
    )
  })

  it('summarizes historical credits by year, month and benefit without applying current caps', () => {
    const activity = getPlatinumBenefitActivity(
      [
        transaction({
          id: 'prior',
          merchant: 'Hotel credit',
          date: '2025-12-31',
          amount: 500,
          classification: classification('other'),
        }),
        transaction({
          id: 'jan',
          merchant: 'Hotel credit',
          date: '2026-01-01',
          amount: 400,
          classification: classification('other'),
        }),
        transaction({
          id: 'small-a',
          merchant: 'Resy credit',
          date: '2026-01-02',
          amount: 0.1,
          classification: classification('other'),
        }),
        transaction({
          id: 'small-b',
          merchant: 'Resy credit',
          date: '2026-01-02',
          amount: 0.2,
          classification: classification('other'),
        }),
        transaction({
          id: 'purchase',
          merchant: 'Resy',
          date: '2026-02-01',
          amount: -80,
          classification: classification('expense'),
        }),
        transaction({
          id: 'pending',
          merchant: 'Resy credit',
          date: '2026-02-02',
          amount: 80,
          classification: classification('other'),
          pending: true,
        }),
        transaction({
          id: 'future',
          merchant: 'Resy credit',
          date: '2026-10-01',
          amount: 100,
          classification: classification('other'),
        }),
        transaction({
          id: 'invalid',
          merchant: 'Resy credit',
          date: 'unknown',
          amount: 100,
          classification: classification('other'),
        }),
      ],
      '2026-09-03T12:00:00Z',
    )
    const history = buildPlatinumBenefitHistory(activity, 2026)

    expect(history.creditedAmount).toBe(400.3)
    expect(history.creditCount).toBe(3)
    expect(history.months).toHaveLength(12)
    expect(history.months[0]).toEqual({ month: '2026-01', creditedAmount: 400.3 })
    expect(history.months[1].creditedAmount).toBe(0)
    expect(history.benefits.find(({ id }) => id === 'hotel')).toMatchObject({
      creditedAmount: 400,
      creditCount: 1,
    })
    expect(history.activity.map(({ id }) => id)).toEqual([
      'pending',
      'purchase',
      'small-a',
      'small-b',
      'jan',
    ])
    expect(buildPlatinumBenefitHistory(activity, 2025).creditedAmount).toBe(500)
    expect(
      buildPlatinumBenefitHistory(
        activity.filter(({ benefitId }) => benefitId === 'resy'),
        2026,
      ).creditedAmount,
    ).toBe(0.3)
  })

  it('counts Uber One once and resolves short dates against the snapshot, not the selected year', () => {
    const transactions = [
      transaction({
        id: 'membership',
        merchant: 'Uber One credit',
        date: 'Dec 31',
        amount: 120,
        classification: classification('other'),
      }),
      transaction({
        id: 'cash',
        merchant: 'Uber Cash',
        date: '2026-01-01',
        amount: 15,
        classification: classification('other'),
      }),
    ]
    const reference = '2026-01-03T12:00:00Z'
    const activity = getPlatinumBenefitActivity(transactions, reference)
    expect(activity.map(({ id, benefitId, date }) => ({ id, benefitId, date }))).toEqual([
      { id: 'cash', benefitId: 'uber-cash', date: '2026-01-01' },
      { id: 'membership', benefitId: 'uber-one', date: '2025-12-31' },
    ])
    expect(buildPlatinumBenefitHistory(activity, 2025).creditedAmount).toBe(120)
    const tracker = buildPlatinumBenefitTracker(transactions, reference)
    expect(tracker.find(({ id }) => id === 'uber-cash')).toMatchObject({
      creditedAmount: 0,
      estimatedCreditAmount: 0,
      status: 'external',
    })
    expect(buildPlatinumBenefitHistory(activity, 2026).creditedAmount).toBe(0)
    expect(tracker.find(({ id }) => id === 'uber-one')?.lastCredit?.amount).toBe(120)
  })

  it('assigns a $9.99 Uber charge to Uber One instead of Uber Cash', () => {
    const transactions = [
      transaction({ id: 'subscription', merchant: 'Uber', date: '2026-09-05', amount: -9.99 }),
    ]
    const activity = getPlatinumBenefitActivity(transactions, '2026-09-30T12:00:00Z')
    const tracker = buildPlatinumBenefitTracker(transactions, '2026-09-30T12:00:00Z')

    expect(activity).toEqual([
      expect.objectContaining({ id: 'subscription', benefitId: 'uber-one', kind: 'purchase' }),
    ])
    expect(tracker.find(({ id }) => id === 'uber-cash')?.estimatedCreditAmount).toBe(0)
  })

  it('estimates Uber Cash once per month from posted matched purchases', () => {
    const activity = getPlatinumBenefitActivity(
      [
        transaction({ id: 'sep-1', merchant: 'Uber', date: '2026-09-02', amount: -5 }),
        transaction({ id: 'sep-2', merchant: 'Uber Eats', date: '2026-09-08', amount: -12 }),
        transaction({ id: 'dec', merchant: 'Uber', date: '2026-12-20', amount: -20 }),
        transaction({
          id: 'pending',
          merchant: 'Uber',
          date: '2026-11-02',
          amount: -8,
          pending: true,
        }),
      ],
      '2026-12-31T12:00:00Z',
    )
    const history = buildPlatinumBenefitHistory(activity, 2026)

    expect(history.creditedAmount).toBe(0)
    expect(history.estimatedAmount).toBe(50)
    expect(history.estimatedActivity).toEqual([
      expect.objectContaining({ id: 'estimated-uber-cash-2026-12', amount: 35 }),
      expect.objectContaining({ id: 'estimated-uber-cash-2026-09', amount: 15 }),
    ])
    expect(history.benefits.find(({ id }) => id === 'uber-cash')).toMatchObject({
      creditedAmount: 50,
      creditCount: 0,
      estimatedCount: 2,
    })
  })

  it('returns an empty history without inventing credits', () => {
    const history = buildPlatinumBenefitHistory([], 2026)
    expect(history.creditedAmount).toBe(0)
    expect(history.estimatedAmount).toBe(0)
    expect(history.creditCount).toBe(0)
    expect(history.activity).toEqual([])
    expect(history.estimatedActivity).toEqual([])
    expect(history.months.every(({ creditedAmount }) => creditedAmount === 0)).toBe(true)
  })

  it('withholds near-reset estimates without moving posted credits into an inferred purchase period', () => {
    const transactions = [
      transaction({
        id: 'purchase',
        merchant: 'Resy',
        date: '2026-09-29',
        amount: -75,
        classification: classification('expense'),
      }),
      transaction({
        id: 'credit',
        merchant: 'Resy credit',
        date: '2026-10-03',
        amount: 75,
        classification: classification('other'),
      }),
    ]
    const reference = '2026-10-04T12:00:00Z'
    expect(
      buildPlatinumBenefitTracker(transactions, reference).find(({ id }) => id === 'resy'),
    ).toMatchObject({
      creditedAmount: 75,
      remainingAmount: null,
      periodUncertain: true,
      windowStart: '2026-10-01',
    })
    const history = buildPlatinumBenefitHistory(
      getPlatinumBenefitActivity(transactions, reference),
      2026,
    )
    expect(history.months[8].creditedAmount).toBe(0)
    expect(history.months[9].creditedAmount).toBe(75)
    // A same-month purchase is still not an explicit issuer link to a credit.
    expect(
      buildPlatinumBenefitTracker(
        [
          transaction({
            id: 'sub',
            merchant: 'Peacock',
            amount: -10,
            classification: classification('expense'),
          }),
          transaction({
            id: 'credit',
            merchant: 'Digital entertainment credit',
            amount: 10,
            classification: classification('other'),
          }),
        ],
        '2026-09-20T12:00:00Z',
      ).find(({ id }) => id === 'digital-entertainment'),
    ).toMatchObject({
      creditedAmount: 10,
      remainingAmount: null,
      periodUncertain: true,
    })
  })

  it('uses bounded late-window estimates and reopens uncertainty for reversals', () => {
    const credit = transaction({
      id: 'credit',
      merchant: 'Resy credit',
      date: '2026-09-15',
      amount: 70,
      classification: classification('other'),
    })
    const reference = '2026-09-20T12:00:00Z'
    expect(
      buildPlatinumBenefitTracker([credit], reference).find(({ id }) => id === 'resy'),
    ).toMatchObject({
      creditedAmount: 70,
      remainingAmount: 30,
      periodUncertain: false,
    })
    expect(
      buildPlatinumBenefitTracker(
        [
          credit,
          transaction({
            id: 'reversal',
            merchant: 'Resy credit reversal',
            date: '2026-09-17',
            amount: -20,
            classification: classification('expense', { mark: 'refund' }),
          }),
        ],
        reference,
      ).find(({ id }) => id === 'resy'),
    ).toMatchObject({
      creditedAmount: 50,
      remainingAmount: null,
      periodUncertain: true,
    })
    const orderBefore = buildPlatinumBenefitTracker([credit], reference).map(({ id }) => id)
    const orderAfter = buildPlatinumBenefitTracker([{ ...credit, amount: 100 }], reference).map(
      ({ id }) => id,
    )
    expect(orderAfter).toEqual(orderBefore)
  })

  it('respects the hotel posting delay and marks snapshots from before a reset', () => {
    expect(
      buildPlatinumBenefitTracker(
        [
          transaction({
            merchant: 'Hotel credit',
            date: '2026-09-20',
            amount: 300,
            classification: classification('other'),
          }),
        ],
        '2026-09-21T12:00:00Z',
      ).find(({ id }) => id === 'hotel'),
    ).toMatchObject({
      creditedAmount: 300,
      remainingAmount: null,
      periodUncertain: true,
    })
    expect(
      buildPlatinumBenefitTracker([], '2026-10-02T12:00:00Z', '2026-09-30T12:00:00Z').find(
        ({ id }) => id === 'resy',
      ),
    ).toMatchObject({ snapshotBeforeWindow: true, remainingAmount: null })
  })

  it('keeps Uber Cash in the Hawaii calendar through its December expiration', () => {
    const before = buildPlatinumBenefitTracker([], '2027-01-01T09:59:00Z').find(
      ({ id }) => id === 'uber-cash',
    )!
    expect(before).toMatchObject({
      cap: 35,
      windowEnd: '2026-12-31',
      daysRemaining: 0,
      remainingAmount: null,
    })
    const after = buildPlatinumBenefitTracker([], '2027-01-01T10:00:00Z').find(
      ({ id }) => id === 'uber-cash',
    )!
    expect(after).toMatchObject({ cap: 15, windowStart: '2027-01-01', windowEnd: '2027-01-31' })
  })

  it('versions changed allowances without applying the new hotel cap to the old annual window', () => {
    const earlier = buildPlatinumBenefitTracker([], '2025-09-17T12:00:00Z')
    expect(earlier.find(({ id }) => id === 'digital-entertainment')?.cap).toBe(20)
    expect(earlier.find(({ id }) => id === 'hotel')).toMatchObject({
      cap: 200,
      windowStart: '2025-01-01',
    })
    expect(earlier.find(({ id }) => id === 'resy')).toBeUndefined()
    const later = buildPlatinumBenefitTracker(
      [
        transaction({
          merchant: 'Hotel credit',
          date: '2025-08-01',
          amount: 200,
          classification: classification('other'),
        }),
      ],
      '2025-09-18T12:00:00Z',
    )
    expect(later.find(({ id }) => id === 'digital-entertainment')?.cap).toBe(25)
    expect(later.find(({ id }) => id === 'hotel')).toMatchObject({
      cap: 300,
      creditedAmount: 0,
      windowStart: '2025-09-18',
    })
    expect(
      buildPlatinumBenefitTracker([], '2026-06-30T12:00:00Z').find(({ id }) => id === 'clear')?.cap,
    ).toBe(209)
    expect(
      buildPlatinumBenefitTracker([], '2026-07-01T12:00:00Z').find(({ id }) => id === 'clear')?.cap,
    ).toBe(219)
  })

  it('retains late Saks credits and reversals in history after retirement', () => {
    const transactions = [
      transaction({
        id: 'credit',
        merchant: 'Saks credit',
        date: '2026-07-03',
        amount: 50,
        classification: classification('other'),
      }),
      transaction({
        id: 'reversal',
        merchant: 'Saks credit reversal',
        date: '2026-08-01',
        amount: -10,
        classification: classification('expense', { mark: 'refund' }),
      }),
      transaction({
        id: 'purchase',
        merchant: 'Saks',
        date: '2026-08-01',
        amount: -100,
        classification: classification('expense'),
      }),
    ]
    const reference = '2026-09-20T12:00:00Z'
    expect(
      buildPlatinumBenefitTracker(transactions, reference).find(({ id }) => id === 'saks'),
    ).toBeUndefined()
    const activity = getPlatinumBenefitActivity(transactions, reference)
    expect(activity).toHaveLength(2)
    expect(
      buildPlatinumBenefitHistory(activity, 2026).benefits.find(({ id }) => id === 'saks'),
    ).toMatchObject({
      creditedAmount: 40,
      creditCount: 1,
      retiredOn: '2026-07-01',
    })
  })

  it('keeps renewal credits and per-bike credits separate from annual allowances', () => {
    const reference = '2026-09-20T12:00:00Z'
    const transactions = [
      transaction({
        id: 'entry',
        merchant: 'Global Entry credit',
        date: '2023-02-01',
        amount: 100,
        classification: classification('other'),
      }),
      transaction({
        id: 'bike-1',
        merchant: 'Equinox SoulCycle bike credit',
        date: '2026-08-01',
        amount: 300,
        classification: classification('other'),
      }),
      transaction({
        id: 'bike-2',
        merchant: 'SoulCycle Equinox bike credit',
        date: '2026-09-01',
        amount: 300,
        classification: classification('other'),
      }),
    ]
    const benefits = buildPlatinumBenefitTracker(transactions, reference)
    expect(benefits.find(({ id }) => id === 'global-entry')).toMatchObject({
      renewalCheck: 'Feb 1, 2027',
      remainingAmount: null,
      creditedAmount: 100,
    })
    expect(benefits.find(({ id }) => id === 'soulcycle')).toMatchObject({
      creditedAmount: 600,
      remainingAmount: null,
    })
    expect(benefits.find(({ id }) => id === 'equinox')?.creditedAmount).toBe(0)
    expect(
      buildPlatinumBenefitTracker(
        [
          ...transactions,
          transaction({
            merchant: 'Global Entry credit reversal',
            date: '2023-02-02',
            amount: -100,
            classification: classification('expense', { mark: 'refund' }),
          }),
        ],
        reference,
      ).find(({ id }) => id === 'global-entry')?.renewalCheck,
    ).toBeUndefined()
  })

  it('recognizes Tock only after its eligibility change and never counts a purchase as a credit', () => {
    const activity = getPlatinumBenefitActivity(
      [
        transaction({
          id: 'before',
          merchant: 'Tock reservation',
          date: '2026-09-14',
          amount: -80,
          classification: classification('expense'),
        }),
        transaction({
          id: 'after',
          merchant: 'Tock reservation',
          date: '2026-09-15',
          amount: -80,
          classification: classification('expense'),
        }),
      ],
      '2026-09-20T12:00:00Z',
    )
    expect(activity).toMatchObject([{ id: 'after', benefitId: 'resy', kind: 'purchase' }])
    expect(buildPlatinumBenefitHistory(activity, 2026).creditedAmount).toBe(0)
  })
})
