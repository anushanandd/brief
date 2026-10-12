import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { benefitWindow } from '../data/fixtures/insights'
import type { Account, BenefitActivity, BenefitOutcome, BenefitWindow, Transaction } from './schema'
import {
  allSpendingAccounts,
  buildMonthlySpendingHistory,
  buildSpendingBars,
  benefitWindowSummary,
  benefitsToUseFirst,
  buildSpendingView,
  formatActivityDate,
  platinumCreditSummary,
  resolveSpendingAccount,
  sortTransactionsByRecency,
  spendingCategoryColor,
  spendingScope,
  spendingMonthReference,
  spendingPeriodLabel,
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

const benefitActivity = (
  id: string,
  date: string,
  amount: number,
  kind: BenefitActivity['kind'],
  pending = false,
): BenefitActivity => ({
  id,
  merchant: 'Synthetic credit',
  amount,
  pending,
  date,
  benefitId: 'resy',
  benefitName: 'Resy',
  kind,
})

const outcome = (
  benefitId: string,
  windowEnd: string,
  overrides: Partial<BenefitOutcome> = {},
): BenefitOutcome => ({
  windowEnd,
  benefitId,
  state: 'settled',
  unused: 0,
  unusedHigh: 0,
  engaged: true,
  verdict: 'missed',
  ...overrides,
})

const judged = (opening: number) => [
  {
    kind: 'statement' as const,
    start: '2026-08-24',
    opening,
    projected: { total: 35.23, categories: { Shopping: 35.23 } },
  },
]

describe('spending view', () => {
  it('maps spending ranges and moves through calendar months', () => {
    expect(spendingMonthReference('2026-09-03T12:00:00Z', 0)).toBe('2026-09-03T12:00:00Z')
    expect(spendingMonthReference('2026-09-03T12:00:00Z', -1)).toBe('2026-08-31T12:00:00Z')
    expect(spendingMonthReference('2026-09-03T12:00:00Z', -9)).toBe('2025-12-31T12:00:00Z')
    expect(spendingPeriodReference(undefined, '2026-09-03T12:00:00Z', -1, 'week')).toBe(
      '2026-08-27T12:00:00Z',
    )
    expect(spendingPeriodReference(undefined, '2026-09-03T12:00:00Z', -1, 1)).toBe(
      '2026-08-31T12:00:00Z',
    )
    expect(spendingPeriodReference(undefined, '2026-09-03T12:00:00Z', -1, 3)).toBe(
      '2026-06-30T12:00:00Z',
    )
    expect(spendingPeriodReference(undefined, '2026-09-03T12:00:00Z', -1, 12)).toBe(
      '2025-12-31T12:00:00Z',
    )
    expect(spendingPeriodReference(undefined, '2026-09-03T12:00:00Z', -1, 0)).toBe(
      '2026-09-03T12:00:00Z',
    )
    expect(
      (['week', 1, 3, 12, 0] as const).map((period) =>
        spendingPeriodLabel(period, 0, 'September 2026'),
      ),
    ).toEqual(['This week', 'This month', 'This quarter', 'This year', 'All time'])
    expect(spendingPeriodLabel('statement', 0, 'Sep 5, 2026', 'statement')).toBe(
      'Current statement',
    )
    expect(spendingPeriodLabel('statement', 0, 'September 2026')).toBe('This month')
    expect(spendingPeriodLabel('statement', -1, 'Sep 5, 2026', 'statement')).toBe(
      'Statement ending Sep 5, 2026',
    )
    expect(spendingPeriodLabel('week', -1, 'Aug 27, 2026')).toBe('Week ending Aug 27, 2026')
    expect(spendingPeriodLabel(1, -1, 'August 2026')).toBe('August 2026')
    expect(spendingPeriodLabel(3, -1, 'August 2026')).toBe('Quarter through August 2026')
    expect(spendingPeriodLabel(12, -1, 'August 2026')).toBe('Year through August 2026')
    expect(spendingPeriodLabel(0, -1, 'August 2026')).toBe('All time through August 2026')
  })

  it('aligns month, quarter, and year views to calendar boundaries', () => {
    const transactions = [
      transaction({ id: 'january', date: '2026-01-15', amount: -10 }),
      transaction({ id: 'march', date: '2026-03-31', amount: -20 }),
      transaction({ id: 'april', date: '2026-04-01', amount: -30 }),
      transaction({ id: 'may', date: '2026-05-15', amount: -40 }),
    ]
    const reference = '2026-05-15T12:00:00Z'

    expect(buildSpendingView(transactions, reference, 1)).toMatchObject({
      start: '2026-05-01',
      total: 40,
    })
    expect(buildSpendingView(transactions, reference, 3)).toMatchObject({
      start: '2026-04-01',
      total: 70,
    })
    expect(buildSpendingView(transactions, reference, 12)).toMatchObject({
      start: '2026-01-01',
      total: 100,
    })
  })

  it('keeps all-time anchored to the snapshot', () => {
    const transactions = [
      transaction({ id: 'january', date: '2026-01-15', amount: -10 }),
      transaction({ id: 'august', date: '2026-08-15', amount: -20 }),
      transaction({ id: 'september', date: '2026-09-01', amount: -30 }),
    ]

    expect(buildSpendingView(transactions, '2026-09-03T12:00:00Z', 0).total).toBe(60)
  })

  it('builds a seven-day view with the matching prior-week comparison', () => {
    const view = buildSpendingView(
      [
        transaction({ id: 'current', date: '2026-09-03', amount: -30 }),
        transaction({ id: 'prior', date: '2026-08-27', amount: -20 }),
        transaction({ id: 'outside', date: '2026-08-20', amount: -100 }),
      ],
      '2026-09-03T12:00:00Z',
      'week',
    )
    expect(view).toMatchObject({
      start: '2026-08-28',
      end: '2026-09-03',
      total: 30,
      previousTotal: 20,
      periodBasis: 'calendar',
    })
    expect(view.transactions.map(({ id }) => id)).toEqual(['current'])
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

  it('uses native statement boundaries and autopay evidence for statement views', () => {
    // Native statement evidence for monthly autopays posted Jul 7, Aug 7 and Sep 7.
    const statement = {
      boundaries: ['2026-06-22', '2026-07-23', '2026-08-23', '2026-09-23', '2026-10-23'],
      autopays: [
        { date: '2026-07-07', amount: 90 },
        { date: '2026-08-07', amount: 100 },
        { date: '2026-09-07', amount: 110 },
      ],
    }
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

    // Rust judges the cycle: where it opened and where it is heading.
    const current = buildSpendingView(
      transactions,
      '2026-09-14T12:00:00Z',
      'statement',
      '2026-09-14T12:00:00Z',
      judged(110),
      statement,
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
    expect(current.pace).toMatchObject({ start: '2026-08-24', end: '2026-09-23' })
    expect(current.pace.current).toHaveLength(22)
    expect(current.pace.current.at(-1)).toBe(25)
    expect(current.pace.previous).toHaveLength(31)
    expect(current.pace.previous.at(-1)).toBe(60)
    expect(current.pace.projected).toBe(35.23)
    // Payments are not spending, so only the purchase is marked on the pace line.
    expect(current.pace.days).toEqual([
      {
        index: 19,
        date: '2026-09-12',
        activities: [{ id: 'spending:current-spend', value: 25 }],
      },
    ])
    const anchored = buildSpendingView(
      transactions,
      '2026-09-14T12:00:00Z',
      'statement',
      '2026-09-14T12:00:00Z',
      judged(127),
      statement,
    )
    expect(anchored).toMatchObject({ startingBalance: 127, statementBalance: 42 })
    expect(anchored.trend.at(-1)?.current).toBe(42)

    const calendarMonth = buildSpendingView(
      transactions,
      '2026-09-14T12:00:00Z',
      1,
      '2026-09-14T12:00:00Z',
    )
    expect(calendarMonth).toMatchObject({ start: '2026-09-01', periodBasis: 'calendar' })
    expect(spendingPeriodReference(statement, '2026-09-14T12:00:00Z', -1, 1)).toBe(
      '2026-08-31T12:00:00Z',
    )

    const priorReference = spendingPeriodReference(
      statement,
      '2026-09-14T12:00:00Z',
      -1,
      'statement',
    )
    expect(priorReference).toBe('2026-08-23T12:00:00Z')
    expect(
      buildSpendingView(
        transactions,
        '2026-09-14T12:00:00Z',
        'statement',
        priorReference,
        [],
        statement,
      ),
    ).toMatchObject({
      start: '2026-07-24',
      end: '2026-08-23',
      total: 60,
      periodBasis: 'statement',
    })
  })

  it('falls back to calendar periods without native statement evidence', () => {
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
      'statement',
      '2026-08-14T12:00:00Z',
    )

    expect(view).toMatchObject({ start: '2026-08-01', periodBasis: 'calendar', total: 60 })
    expect(spendingPeriodReference(undefined, '2026-08-14T12:00:00Z', -1, 'statement')).toBe(
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
      expect(result.dailyAverage).toBeCloseTo(50 / 31)
    }
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
        transaction({
          id: 'pending',
          date: '2026-09-02',
          category: 'Dining',
          amount: -25,
          pending: true,
        }),
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
    expect(
      buildSpendingBars(view.transactions, '2026-09-03T12:00:00Z', view.start, view.end, 1).slice(
        0,
        3,
      ),
    ).toEqual([
      {
        from: '2026-09-01',
        to: '2026-09-01',
        value: 100,
        categories: [
          {
            name: 'Shopping',
            value: 100,
            activities: [{ id: 'spending:current', value: 100 }],
          },
        ],
      },
      {
        from: '2026-09-02',
        to: '2026-09-02',
        value: 25,
        categories: [
          {
            name: 'Dining',
            value: 25,
            activities: [{ id: 'spending:pending', value: 25 }],
          },
        ],
      },
      { from: '2026-09-03', to: '2026-09-03', value: 0, categories: [] },
    ])
    expect(view.trend.at(-1)).toMatchObject({ date: '2026-09-03', previous: 50 })
  })

  it('plots credits below spending without adding them to spending totals', () => {
    const view = buildSpendingView(
      [
        transaction({ id: 'spend', date: '2026-09-01', amount: -30 }),
        transaction({
          id: 'credit',
          merchant: 'Synthetic credit',
          category: 'Credit',
          date: '2026-09-01',
          amount: 10,
          classification: classification('reimbursement'),
        }),
      ],
      '2026-09-03T12:00:00Z',
      1,
    )

    expect(view.total).toBe(30)
    expect(
      buildSpendingBars(view.transactions, '2026-09-03T12:00:00Z', view.start, view.end, 1)[0],
    ).toMatchObject({
      value: 20,
      categories: [
        { name: 'Credits', value: -10, activities: [{ id: 'spending:credit', value: 10 }] },
        { name: 'Shopping', value: 30, activities: [{ id: 'spending:spend', value: 30 }] },
      ],
    })
  })

  it('sums native credit evidence and expired allowances in cents within a range', () => {
    const benefits = {
      accountId: 'card',
      product: 'amex-platinum',
      productName: 'American Express Platinum',
      windows: [],
      useFirst: [],
      activity: [
        benefitActivity('credit', '2026-08-02', 10.1, 'credit'),
        benefitActivity('reversal', '2026-08-03', -0.2, 'reversal'),
        benefitActivity('pending', '2026-08-04', 50, 'credit', true),
        benefitActivity('purchase', '2026-08-05', -80, 'purchase'),
        benefitActivity('outside', '2026-09-02', 70, 'credit'),
      ],
      outcomes: [
        outcome('resy', '2026-08-31', { unused: 39.9, unusedHigh: 89.9 }),
        outcome('lululemon', '2026-08-31', {
          unused: 75,
          unusedHigh: 75,
          engaged: false,
          verdict: 'unclaimed',
        }),
        outcome('hotel', '2026-08-31', {
          state: 'settling',
          unused: null,
          unusedHigh: null,
          verdict: 'settling',
        }),
        outcome('clear', '2026-08-31', {
          state: 'partial-history',
          unused: null,
          unusedHigh: null,
          verdict: 'unknown',
        }),
        outcome('walmart-plus', '2026-08-31', {
          state: 'not-tracked',
          unused: null,
          unusedHigh: null,
          verdict: undefined,
        }),
        outcome('oura', '2026-08-31', { unused: 0.1, unusedHigh: 0.1 }),
        outcome('resy', '2026-09-30', { unused: 100, unusedHigh: 100 }),
      ],
    }
    expect(platinumCreditSummary(benefits, '2026-08-01', '2026-08-31')).toEqual({
      earned: 9.9,
      missed: { low: 40, high: 90 },
      unclaimed: 75,
      settling: 1,
      unknown: 1,
    })
    // Hidden benefits leave every total.
    expect(
      platinumCreditSummary(benefits, '2026-08-01', '2026-08-31', ['resy', 'lululemon']).missed,
    ).toEqual({ low: 0.1, high: 0.1 })
    expect(platinumCreditSummary(undefined, '2026-08-01', '2026-08-31')).toEqual({
      earned: 0,
      missed: { low: 0, high: 0 },
      unclaimed: 0,
      settling: 0,
      unknown: 0,
    })
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

  it("shows Rust's usual spending for the categories in view, zero for the new ones", () => {
    const dining = (id: string, date: string, amount: number) =>
      transaction({ id, date, amount: -amount, category: 'Dining' })
    const history = [
      transaction({ id: 'start', date: '2026-06-01', amount: -1, category: 'Shopping' }),
      dining('june', '2026-06-05', 30),
      dining('july', '2026-07-05', 10),
      dining('august', '2026-08-05', 20),
      // Later in August than the selected month has reached.
      dining('late', '2026-08-20', 50),
      dining('now', '2026-09-03', 25),
      transaction({ id: 'new', date: '2026-09-04', amount: -5, category: 'Travel' }),
    ]
    const view = buildSpendingView(history, '2026-09-10T12:00:00Z', 1, undefined, [
      { kind: 'month', start: '2026-09-01', typical: { Dining: 20, Shopping: 0.33 } },
      { kind: 'month', start: '2026-08-01', typical: { Dining: 99 } },
    ])
    const typicalByName = Object.fromEntries(
      view.categories.map(({ name, typical }) => [name, typical]),
    )
    expect(typicalByName).toEqual({ Dining: 20, Travel: 0 })

    // A period Rust has nothing usual for shows none.
    const unjudged = buildSpendingView(history, '2026-09-10T12:00:00Z', 1)
    expect(unjudged.categories.every(({ typical }) => typical === null)).toBe(true)
  })

  it('uses the computer calendar day instead of UTC month boundaries', () => {
    const localReference = new Date(2026, 8, 1, 0, 30).toISOString()
    const view = buildSpendingView(
      [transaction({ date: '2026-09-01', amount: -25 })],
      localReference,
      1,
    )
    expect(view.end).toBe('2026-09-01')
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
})

const monthlyWindow = (id: string, overrides: Partial<BenefitWindow>) =>
  benefitWindow({
    id,
    name: id,
    cap: 10,
    cadence: 'monthly',
    remainingAmount: 10,
    remainingRange: { low: 10, high: 10 },
    windowStart: '2026-09-01',
    windowEnd: '2026-09-30',
    daysRemaining: 16,
    reset: 'Sep 30',
    ...overrides,
  })

describe('benefitWindowSummary', () => {
  it('sums current windows as ranges and points at the soonest reset with credit left', () => {
    expect(
      benefitWindowSummary([
        monthlyWindow('dining', {
          creditedAmount: 7.5,
          remainingAmount: 2.5,
          remainingRange: { low: 2.5, high: 2.5 },
          awaitingCreditAmount: 2.5,
        }),
        monthlyWindow('hotel', {
          cadence: 'semiannual',
          remainingAmount: 300,
          remainingRange: { low: 300, high: 300 },
          windowEnd: '2026-12-31',
          reset: 'Dec 31',
          daysRemaining: 108,
        }),
        monthlyWindow('used', {
          creditedAmount: 10,
          remainingAmount: 0,
          remainingRange: { low: 0, high: 0 },
          windowEnd: '2026-09-15',
        }),
        monthlyWindow('renewal', { cadence: 'renewal', creditedAmount: 99, remainingAmount: 50 }),
        monthlyWindow('uncertain', {
          periodUncertain: true,
          creditedAmount: 40,
          remainingAmount: null,
          remainingRange: { low: 0, high: 40 },
        }),
        monthlyWindow('untracked', { remainingAmount: null, remainingRange: undefined }),
      ]),
    ).toEqual({
      captured: 57.5,
      remaining: { low: 302.5, high: 342.5 },
      awaiting: 2.5,
      nextReset: { label: 'Sep 30', daysRemaining: 16 },
    })
  })

  it("lists benefits to use first in Rust's order", () => {
    // Rust orders them; a hidden benefit (here `hidden`) is left out of what is shown.
    const ordered = benefitsToUseFirst(
      [monthlyWindow('hotel', {}), monthlyWindow('uber', {}), monthlyWindow('streaming', {})],
      ['streaming', 'hidden', 'uber', 'hotel'],
    )
    expect(ordered.map(({ id }) => id)).toEqual(['streaming', 'uber', 'hotel'])
  })
})

const ids = (scope: { transactions: Transaction[] }) => scope.transactions.map(({ id }) => id)

const account = (id: string, type: string, currency = 'USD') => ({
  id,
  name: id,
  institution: 'Bank',
  type,
  value: 0,
  currency,
})

const purchase = (id: string, accountId: string, kind: 'expense' | 'transfer' = 'expense') => ({
  id,
  account: accountId,
  accountId,
  merchant: 'Example',
  category: 'Shopping',
  amount: -10,
  classification: classification(kind),
  pending: false,
  date: '2026-09-10',
})

it('scopes spending to the saved card by default, one chosen account, or every USD account', () => {
  const accounts = [
    account('all', 'combined'),
    account('card', 'credit'),
    account('other-card', 'credit'),
    account('checking', 'cash'),
    account('euro', 'cash', 'EUR'),
    account('idle', 'cash'),
  ]
  const transactions = [
    purchase('a', 'card'),
    purchase('b', 'other-card'),
    purchase('c', 'checking'),
    purchase('d', 'euro'),
    purchase('e', 'idle', 'transfer'),
  ]

  const saved = spendingScope(accounts, transactions, 'card')
  expect(saved.selected?.id).toBe('card')
  expect(ids(saved)).toEqual(['a'])
  expect(saved.options.map(({ id }) => id)).toEqual(['card', 'other-card', 'checking'])

  expect(ids(spendingScope(accounts, transactions, 'card', 'checking'))).toEqual(['c'])
  expect(ids(spendingScope(accounts, transactions, 'card', allSpendingAccounts))).toEqual([
    'a',
    'b',
    'c',
  ])

  // Without a saved card the view is every account; no other card stands in for it.
  const unsaved = spendingScope(accounts, transactions, '')
  expect(unsaved.card).toBeUndefined()
  expect(unsaved.selected).toBeUndefined()
  expect(ids(unsaved)).toEqual(['a', 'b', 'c'])
  // An unknown or non-USD request falls back to the saved card.
  expect(spendingScope(accounts, transactions, 'card', 'euro').selected?.id).toBe('card')
})
