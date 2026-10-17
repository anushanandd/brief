import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { benefitWindow } from '../data/fixtures/insights'
import type {
  Account,
  BenefitWindow,
  SpendingPeriodDetail,
  SpendingPeriodJudgment,
  Transaction,
} from './schema'
import {
  allSpendingAccounts,
  benefitsToUseFirst,
  currentSpendingPeriod,
  formatActivityDate,
  monthlySpendingHistory,
  resolveSpendingAccount,
  sortTransactionsByRecency,
  spendingCategoryColor,
  spendingChartBars,
  spendingChartPace,
  spendingFigures,
  spendingHeadline,
  spendingPeriodKind,
  spendingPeriodLabel,
  spendingScope,
  spendingView,
  transactionDateKey,
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

/** A period as Rust sums and judges it; only the fields a test names matter. */
const rustPeriod = (overrides: Partial<SpendingPeriodJudgment>): SpendingPeriodJudgment => ({
  kind: 'month',
  start: '2026-09-01',
  end: '2026-09-03',
  total: 0,
  previous: null,
  change: null,
  changePct: null,
  daily: 0,
  pending: 0,
  categories: [],
  ...overrides,
})

const dining = {
  name: 'Dining',
  total: 25,
  previous: 20,
  change: 5,
  changePct: 25,
  daily: 2.5,
  share: 80,
  typical: 20,
  aboveTypical: 5,
}

const pace = {
  start: '2026-09-01',
  end: '2026-09-30',
  current: [10, 35],
  previous: [5, 5, 40],
  projected: 400,
  days: [
    {
      index: 1,
      date: '2026-09-02',
      total: 25,
      purchases: [
        { id: 'dinner', value: 20 },
        { id: 'coffee', value: 5 },
      ],
    },
  ],
}

/** A period in full as Rust reads it on demand. */
const detail = (overrides: Partial<SpendingPeriodDetail> = {}): SpendingPeriodDetail => ({
  ...rustPeriod({ total: 35, categories: [dining] }),
  earlier: true,
  transactionIds: ['dinner', 'coffee', 'book'],
  merchants: [],
  pace,
  bars: [],
  focus: {
    Dining: {
      transactionIds: ['dinner', 'coffee'],
      merchants: [],
      pace: { ...pace, current: [0, 25] },
    },
  },
  ...overrides,
})

describe('spending periods', () => {
  it('names each period and maps each choice to the period Rust computes', () => {
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
    expect((['statement', 'week', 1, 3, 12, 0] as const).map(spendingPeriodKind)).toEqual([
      'statement',
      'week',
      'month',
      'quarter',
      'year',
      'all',
    ])
  })

  it("shows Rust's figures for the period, or for the one category in focus", () => {
    const period = rustPeriod({ total: 30, previous: 40, change: -10, categories: [dining] })
    expect(spendingFigures(period)).toBe(period)
    expect(spendingFigures(period, 'Dining')).toBe(dining)
    // A category without spending this period, or a period Rust lacks, spent nothing.
    expect(spendingFigures(period, 'Travel')).toMatchObject({ total: 0, change: null })
    expect(spendingFigures(undefined)).toMatchObject({ total: 0, changePct: null })
  })

  it("lists and paces a category from Rust's view of it, and nothing for one without spending", () => {
    const period = detail()
    expect(spendingView(period)).toBe(period)
    expect(spendingView(period, 'Dining').transactionIds).toEqual(['dinner', 'coffee'])
    expect(spendingView(period, 'Travel')).toMatchObject({
      transactionIds: [],
      merchants: [],
      pace: { start: '2026-09-01', end: '2026-09-30', current: [], projected: null },
    })
  })

  it("draws Rust's pace and bars, naming purchases as the page's lists do", () => {
    const chart = spendingChartPace(pace)
    expect(chart.days[0]).toEqual({
      index: 1,
      date: '2026-09-02',
      total: 25,
      activities: [
        { id: 'spending:dinner', value: 20 },
        { id: 'spending:coffee', value: 5 },
      ],
    })
    const bars = [
      {
        from: '2026-08-01',
        to: '2026-08-31',
        value: 20,
        categories: [
          { name: 'Credits', value: -10, purchases: [{ id: 'credit', value: 10 }] },
          { name: 'Dining', value: 30, purchases: [{ id: 'dinner', value: 30 }] },
        ],
      },
      { from: '2026-09-01', to: '2026-09-03', value: 0, categories: [] },
    ]
    expect(spendingChartBars(bars)[0]).toMatchObject({
      value: 20,
      categories: [
        { name: 'Credits', activities: [{ id: 'spending:credit', value: 10 }] },
        { name: 'Dining', activities: [{ id: 'spending:dinner', value: 30 }] },
      ],
    })
    // A focus keeps its own category, whose value Rust already summed.
    expect(
      spendingChartBars(bars, 'Dining').map(({ value, categories }) => [value, categories.length]),
    ).toEqual([
      [30, 1],
      [0, 0],
    ])
  })

  it("reads a scope's current period and recent months from the snapshot", () => {
    const insights = {
      spending: [
        {
          scope: 'card',
          owner: 'me' as const,
          periods: [
            rustPeriod({ total: 10 }),
            rustPeriod({ kind: 'week', start: '2026-08-28', total: 99 }),
          ],
          months: [
            { month: '2026-08', total: 20 },
            { month: '2026-09', total: 10 },
          ],
        },
      ],
    }
    expect(currentSpendingPeriod(insights, 'card', 'month')?.total).toBe(10)
    // A statement without closing dates is the month.
    expect(currentSpendingPeriod(insights, 'card', 'statement')?.total).toBe(10)
    expect(currentSpendingPeriod(insights, 'card', 'year')).toBeUndefined()
    expect(monthlySpendingHistory(insights, 'card')).toEqual([
      { month: '2026-08', value: 20 },
      { month: '2026-09', value: 10 },
    ])
    expect(monthlySpendingHistory(insights, 'cash')).toEqual([])
  })

  it("heads Money with the statement balance, or the period's spending and its change", () => {
    const accounts: Account[] = [
      { id: 'card', name: 'Card', institution: 'Bank', type: 'credit', value: -50 },
    ]
    const insights = {
      spending: [
        {
          scope: 'card',
          owner: 'me' as const,
          periods: [
            rustPeriod({ kind: 'statement', total: 30, balance: 50, change: 4 }),
            rustPeriod({ total: 20, change: -6 }),
          ],
          months: [],
        },
      ],
    }
    const data = { accounts, transactions: [transaction({ accountId: 'card' })], insights }
    expect(spendingHeadline(data, 'card', 'statement')).toEqual({
      hasScope: true,
      value: 50,
      change: 4,
    })
    expect(spendingHeadline(data, 'card', 1)).toEqual({ hasScope: true, value: 20, change: -6 })
    // All time has nothing to change from.
    expect(spendingHeadline(data, 'card', 0).change).toBeNull()
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

  it('keys a transaction by its ISO date, or an instant by its UTC day', () => {
    expect(transactionDateKey('2026-08-31')).toBe('2026-08-31')
    expect(transactionDateKey('2026-08-31T23:30:00-07:00')).toBe('2026-08-31')
    expect(transactionDateKey('Tue, 1 Sep 2026 23:30:00 -0700')).toBe('2026-09-02')
    expect(transactionDateKey('unknown')).toBe('')
  })

  it('formats activity dates with the year last', () => {
    expect(formatActivityDate('2026-09-05')).toBe('Sep 5, 2026')
  })

  it('sorts recent transactions across statement months', () => {
    const sorted = sortTransactionsByRecency([
      transaction({ id: 'july', date: '2026-07-30' }),
      transaction({ id: 'september', date: '2026-09-01' }),
      transaction({ id: 'august', date: '2026-08-31' }),
    ])

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

describe('benefits to use first', () => {
  it("lists benefits to use first in Rust's order", () => {
    // Rust orders them; a hidden benefit (here `hidden`) is left out of what is shown.
    const ordered = benefitsToUseFirst(
      [monthlyWindow('hotel', {}), monthlyWindow('uber', {}), monthlyWindow('streaming', {})],
      ['streaming', 'hidden', 'uber', 'hotel'],
    )
    expect(ordered.map(({ id }) => id)).toEqual(['streaming', 'uber', 'hotel'])
  })
})

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
  expect(saved.options.map(({ id }) => id)).toEqual(['card', 'other-card', 'checking'])
  expect(spendingScope(accounts, transactions, 'card', 'checking').selected?.id).toBe('checking')
  expect(
    spendingScope(accounts, transactions, 'card', allSpendingAccounts).selected,
  ).toBeUndefined()

  // Without a saved card the view is every account; no other card stands in for it.
  const unsaved = spendingScope(accounts, transactions, '')
  expect(unsaved.card).toBeUndefined()
  expect(unsaved.selected).toBeUndefined()
  // An unknown or non-USD request falls back to the saved card.
  expect(spendingScope(accounts, transactions, 'card', 'euro').selected?.id).toBe('card')
})
