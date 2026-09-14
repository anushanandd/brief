import { describe, expect, it } from 'vitest'

import type { Account, Transaction } from './schema'
import {
  buildPlatinumBenefitTracker,
  buildPlatinumBenefitHistory,
  buildMonthlySpendingHistory,
  buildSpendingView,
  formatActivityDate,
  formatTransactionDate,
  getPlatinumBenefitActivity,
  identifySubscriptions,
  resolveSpendingAccount,
  sortTransactionsByRecency,
  spendingCategoryColor,
  spendingMonthReference,
  spendingPeriodLabel,
  spendingPeriodForKey,
  spendingPeriodReference,
  transactionDateKey,
} from './spending'

const transaction = (overrides: Partial<Transaction>): Transaction => ({
  id: 'transaction',
  merchant: 'Merchant',
  category: 'Shopping',
  date: '2026-09-01',
  amount: -10,
  account: 'Platinum Card',
  pending: false,
  ...overrides,
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

  it('uses a stable posted autopay cadence as statement boundaries', () => {
    const transactions = [
      transaction({
        id: 'july-pay',
        date: '2026-07-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 90,
      }),
      transaction({ id: 'july-spend', date: '2026-07-02', amount: -40 }),
      transaction({
        id: 'august-pay',
        date: '2026-08-06',
        postedOn: '2026-08-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 100,
      }),
      transaction({ id: 'august-spend', date: '2026-08-03', amount: -60 }),
      transaction({
        id: 'september-pay',
        date: '2026-09-07',
        merchant: 'AUTOPAY PAYMENT RECEIVED - THANK YOU',
        amount: 110,
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

  it('counts posted credits matched by the local benefit rules', () => {
    const refund = transaction({
      id: 'refund',
      merchant: 'Walmart',
      amount: 100,
      date: '2026-09-01',
    })
    const matched = buildPlatinumBenefitTracker([refund], '2026-09-03T12:00:00Z').find(
      ({ id }) => id === 'walmart-plus',
    )!
    expect(matched.creditedAmount).toBe(100)
    expect(matched.remainingAmount).toBe(0)
    expect(matched.status).toBe('credited')
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
        transaction({ id: 'payment', date: '2026-09-03', category: 'Loan Payments', amount: 40 }),
        transaction({ id: 'transfer', category: 'Transfer Out', amount: -500 }),
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
          transaction({ id: 'payment', category: 'Payment', amount: -500 }),
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

  it('uses relative labels for recent transaction dates', () => {
    const reference = '2026-09-03T12:00:00Z'
    expect(formatTransactionDate('2026-09-03', reference)).toBe('Today')
    expect(formatTransactionDate('Sep 2', reference)).toBe('Yesterday')
    expect(formatTransactionDate('2026-09-01', reference)).toBe('2026-09-01')
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

  it('identifies only repeated, regular posted charges as possible subscriptions', () => {
    const subscriptions = identifySubscriptions(
      [
        transaction({ id: 'music-1', merchant: 'Music Cloud', date: '2026-06-30', amount: -10 }),
        transaction({ id: 'music-2', merchant: 'Music Cloud', date: '2026-07-31', amount: -10.5 }),
        transaction({ id: 'music-3', merchant: 'Music Cloud', date: '2026-08-31', amount: -10.99 }),
        transaction({ id: 'store-1', merchant: 'Corner Store', date: '2026-06-01', amount: -20 }),
        transaction({ id: 'store-2', merchant: 'Corner Store', date: '2026-07-10', amount: -20 }),
        transaction({ id: 'store-3', merchant: 'Corner Store', date: '2026-08-31', amount: -20 }),
        transaction({
          id: 'pending',
          merchant: 'Pending App',
          date: '2026-06-30',
          amount: -5,
          pending: true,
        }),
        transaction({
          id: 'pending-2',
          merchant: 'Pending App',
          date: '2026-07-31',
          amount: -5,
          pending: true,
        }),
        transaction({
          id: 'pending-3',
          merchant: 'Pending App',
          date: '2026-08-31',
          amount: -5,
          pending: true,
        }),
      ],
      '2026-09-03T12:00:00Z',
    )

    expect(subscriptions).toEqual([
      {
        merchant: 'Music Cloud',
        cadence: 'Monthly',
        occurrences: 3,
        latestAmount: 10.99,
        lastChargedOn: '2026-08-31',
      },
    ])
  })

  it('tracks Platinum activity inside each benefit reset window', () => {
    const benefits = buildPlatinumBenefitTracker(
      [
        transaction({ id: 'uber', merchant: 'Uber', date: '2026-09-02' }),
        transaction({
          id: 'entertainment-credit',
          merchant: 'Platinum Digital Entertainment Credit',
          description: 'Platinum Digital Entertainment Credit',
          amount: 15.99,
          date: '2026-08-31',
        }),
        transaction({
          id: 'hotel-credit',
          merchant: 'Platinum Hotel Credit',
          description: 'Platinum Hotel Credit',
          amount: 300,
          date: '2026-06-30',
        }),
        transaction({
          id: 'clear-credit',
          merchant: 'AMEX CLEAR PLUS CREDIT',
          description: 'AMEX CLEAR PLUS CREDIT',
          amount: 209,
          date: '2026-05-08',
        }),
        transaction({
          id: 'airline-credit',
          merchant: 'AMEX Airline Fee Reimbursement',
          description: 'AMEX Airline Fee Reimbursement',
          amount: 50,
          date: '2026-06-16',
        }),
      ],
      '2026-09-03T12:00:00Z',
    )

    expect(benefits.find(({ id }) => id === 'uber-cash')).toMatchObject({
      status: 'matched',
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
      lastCredit: { amount: 15.99, date: 'Aug 31' },
    })
    expect(benefits.find(({ id }) => id === 'hotel')?.lastCredit).toEqual({
      amount: 300,
      date: 'Jun 30',
    })
    expect(benefits.find(({ id }) => id === 'resy')?.status).toBe('unused')
    expect(benefits.map(({ id }) => id)).toEqual([
      'resy',
      'lululemon',
      'digital-entertainment',
      'uber-cash',
      'walmart-plus',
      'hotel',
      'equinox',
      'oura',
      'airline-fee',
      'uber-one',
      'clear',
    ])
    expect(benefits[0]).toMatchObject({ remainingAmount: 100, daysRemaining: 27 })
  })

  it('summarizes historical credits by year, month and benefit without applying current caps', () => {
    const activity = getPlatinumBenefitActivity(
      [
        transaction({ id: 'prior', merchant: 'Hotel credit', date: '2025-12-31', amount: 500 }),
        transaction({ id: 'jan', merchant: 'Hotel credit', date: '2026-01-01', amount: 400 }),
        transaction({ id: 'small-a', merchant: 'Resy credit', date: '2026-01-02', amount: 0.1 }),
        transaction({ id: 'small-b', merchant: 'Resy credit', date: '2026-01-02', amount: 0.2 }),
        transaction({ id: 'purchase', merchant: 'Resy', date: '2026-02-01', amount: -80 }),
        transaction({
          id: 'pending',
          merchant: 'Resy credit',
          date: '2026-02-02',
          amount: 80,
          pending: true,
        }),
        transaction({ id: 'future', merchant: 'Resy credit', date: '2026-10-01', amount: 100 }),
        transaction({ id: 'invalid', merchant: 'Resy credit', date: 'unknown', amount: 100 }),
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
      transaction({ id: 'membership', merchant: 'Uber One', date: 'Dec 31', amount: 120 }),
      transaction({ id: 'cash', merchant: 'Uber Cash', date: '2026-01-01', amount: 15 }),
    ]
    const reference = '2026-01-03T12:00:00Z'
    const activity = getPlatinumBenefitActivity(transactions, reference)
    expect(activity.map(({ id, benefitId, date }) => ({ id, benefitId, date }))).toEqual([
      { id: 'cash', benefitId: 'uber-cash', date: '2026-01-01' },
      { id: 'membership', benefitId: 'uber-one', date: '2025-12-31' },
    ])
    expect(buildPlatinumBenefitHistory(activity, 2025).creditedAmount).toBe(120)
    const tracker = buildPlatinumBenefitTracker(transactions, reference)
    expect(tracker.find(({ id }) => id === 'uber-cash')?.creditedAmount).toBe(15)
    expect(tracker.find(({ id }) => id === 'uber-one')?.lastCredit?.amount).toBe(120)
  })

  it('returns an empty history without inventing credits', () => {
    const history = buildPlatinumBenefitHistory([], 2026)
    expect(history.creditedAmount).toBe(0)
    expect(history.creditCount).toBe(0)
    expect(history.activity).toEqual([])
    expect(history.months.every(({ creditedAmount }) => creditedAmount === 0)).toBe(true)
  })
})
