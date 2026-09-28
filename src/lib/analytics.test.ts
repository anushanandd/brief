import { expect, it } from 'vitest'

import empty from '../data/empty.json'
import { classification } from '../data/fixtures/classification'
import {
  analyticsEntries,
  analyticsBarStack,
  analyticsGroupColors,
  analyticsReport,
  analyticsSearch,
  analyticsTotal,
  analyticsWindow,
  analyticsAccountMatches,
  toggleAnalyticsAccount,
  noAnalyticsAccounts,
} from './analytics'
import { homeOverviewMetrics } from './home-overview'
import { cashFlowBreakdown } from './money'
import { financeSnapshotSchema, type Transaction } from './schema'

const transaction = (
  id: string,
  amount: number,
  patch: Partial<Transaction> = {},
): Transaction => ({
  id,
  amount,
  merchant: 'Example payroll',
  category: 'Income',
  date: '2026-09-03',
  accountId: 'cash',
  account: 'Cash',
  classification: classification('income'),
  pending: false,
  ...patch,
})
const snapshot = () => ({
  ...financeSnapshotSchema.parse(empty),
  updatedAt: '2026-09-18T12:00:00Z',
  accounts: [
    { id: 'cash', name: 'Cash', institution: 'Test', type: 'cash', value: 1000, currency: 'USD' },
    { id: 'card', name: 'Card', institution: 'Test', type: 'credit', value: -100, currency: 'USD' },
    {
      id: 'foreign',
      name: 'Foreign',
      institution: 'Test',
      type: 'cash',
      value: 100,
      currency: 'EUR',
    },
  ],
})

it('reconciles Income to Home and scopes dated posted USD evidence exactly', () => {
  const data = snapshot()
  data.transactions = [
    transaction('pay', 100.1),
    transaction('interest', 0.2, {
      merchant: 'Interest',
      classification: classification('interest'),
    }),
    transaction('dividend', 0.3, {
      merchant: 'Dividend',
      classification: classification('dividend'),
    }),
    transaction('pending', 100, { pending: true }),
    transaction('foreign', 50, { accountId: 'foreign' }),
    transaction('future', 50, { postedOn: '2026-09-19' }),
    transaction('posted-before', 30, { postedOn: '2026-08-31' }),
    transaction('transfer', 20, {
      merchant: 'Transfer',
      category: 'Transfer',
      classification: classification('transfer'),
    }),
    transaction('zelle', 10, {
      merchant: 'Zelle receipt',
      category: 'Transfer',
      classification: classification('income', { zelle: true }),
    }),
  ]
  const window = analyticsWindow(data, { from: '2026-09-01', to: '2026-09-18' })
  const report = analyticsReport(analyticsEntries(data, 'income', 'card'), window)
  expect(report.total).toBe(110.6)
  expect(report.entries.map(({ id }) => id)).toEqual([
    'spending:pay',
    'spending:interest',
    'spending:dividend',
    'spending:zelle',
  ])
  expect(analyticsTotal(report.buckets)).toBe(report.total)
  expect(
    homeOverviewMetrics(
      data,
      'card',
      { start: Date.parse('2026-09-01T12:00:00Z') / 1000, end: Date.parse(data.updatedAt) / 1000 },
      [],
    ).income,
  ).toBe(report.total)
  expect(analyticsEntries(data, 'income', 'card', 'card')).toEqual([])
})

it('uses the saved Amex account, includes reversals and excludes refunds, pending and Uber Cash', () => {
  const data = snapshot()
  data.transactions = [
    transaction('credit', 25, {
      merchant: 'Digital entertainment credit',
      accountId: 'card',
      classification: classification('income'),
    }),
    transaction('reversal', -10, {
      merchant: 'Digital entertainment credit reversal',
      accountId: 'card',
      classification: classification('income'),
    }),
    transaction('refund', 25, {
      merchant: 'Digital entertainment refund',
      accountId: 'card',
      classification: classification('income'),
    }),
    transaction('pending', 25, {
      merchant: 'Resy credit',
      accountId: 'card',
      pending: true,
      classification: classification('income'),
    }),
    transaction('other-account', 100, {
      merchant: 'Resy credit',
      classification: classification('income'),
    }),
    transaction('uber', 15, {
      merchant: 'Uber Cash credit',
      accountId: 'card',
      classification: classification('income'),
    }),
    transaction('retired', 30, {
      merchant: 'Saks credit',
      accountId: 'card',
      classification: classification('income'),
    }),
  ]
  const entries = analyticsEntries(data, 'amex-credits', 'card')
  expect(analyticsTotal(entries)).toBe(45)
  expect(entries.map(({ id }) => id).toSorted()).toEqual([
    'spending:credit',
    'spending:retired',
    'spending:reversal',
  ])
  expect(analyticsEntries(data, 'amex-credits', '')).toEqual([])
  expect(analyticsEntries(data, 'amex-credits', 'card', 'cash')).toEqual([])
})

it('keeps unsupported sale estimates unavailable without hiding known losses in other buckets', () => {
  const data = snapshot()
  data.trades = [
    {
      id: 'known',
      type: 'SELL',
      date: '2026-07-02',
      account: 'Cash',
      accountId: 'cash',
      amount: 80,
      estimatedRealizedGain: -20,
    },
    {
      id: 'unknown',
      type: 'SELL',
      date: '2026-08-02',
      account: 'Cash',
      accountId: 'cash',
      amount: 50,
    },
    {
      id: 'buy',
      type: 'BUY',
      date: '2026-09-01',
      account: 'Cash',
      accountId: 'cash',
      amount: -100,
    },
  ]
  const report = analyticsReport(
    analyticsEntries(data, 'realized', ''),
    analyticsWindow(data, { from: '2026-07-01', to: '2026-09-18' }),
  )
  expect(report.total).toBeNull()
  expect(report.buckets.map(({ value }) => value)).toEqual([-20, null, 0])
  expect(report.entries).toHaveLength(2)
})

it('reconciles fee refunds and reimbursements with the cash-flow Sankey', () => {
  const data = snapshot()
  data.transactions = [
    transaction('pay', 100),
    transaction('fee', -10, {
      merchant: 'Bank fee',
      category: 'Fees',
      classification: classification('fee'),
    }),
    transaction('reversed-fee', 4, {
      merchant: 'Bank fee reversal',
      category: 'Fees',
      classification: classification('fee', { mark: 'refund' }),
    }),
    transaction('purchase', -20, {
      merchant: 'Cafe',
      category: 'Dining',
      classification: classification('expense'),
    }),
    transaction('repayment', 30, {
      merchant: 'Shared expense repayment',
      category: 'Reimbursement',
      classification: classification('reimbursement'),
    }),
  ]
  expect(analyticsTotal(analyticsEntries(data, 'fees', ''))).toBe(6)
  const flow = cashFlowBreakdown(data.transactions)
  const net =
    flow.sources.reduce((sum, item) => sum + item.value, 0) -
    flow.purchases.reduce((sum, item) => sum + item.value, 0) -
    flow.fees -
    flow.taxes
  expect(analyticsTotal(analyticsEntries(data, 'cash-flow', ''))).toBe(104)
  expect(net).toBe(104)
})

it('compares preceding equal-length periods and does not extrapolate absent history', () => {
  const data = snapshot()
  data.transactions = [transaction('old', 50, { date: '2026-07-21' }), transaction('current', 75)]
  const window = analyticsWindow(data, { range: 'month' })
  expect(window).toMatchObject({
    start: '2026-08-20',
    end: '2026-09-18',
    previousStart: '2026-07-21',
    previousEnd: '2026-08-19',
  })
  expect(analyticsWindow(data, { range: 'quarter' })).toMatchObject({
    start: '2026-06-21',
    previousStart: '2026-03-23',
    previousEnd: '2026-06-20',
  })
  expect(
    analyticsWindow({ ...data, updatedAt: '2024-03-31T12:00:00Z' }, { range: 'month' }),
  ).toMatchObject({ start: '2024-03-02', previousStart: '2024-02-01', previousEnd: '2024-03-01' })
  expect(analyticsReport(analyticsEntries(data, 'income', ''), window)).toMatchObject({
    total: 75,
    previous: 50,
    percent: 50,
  })
  expect(
    analyticsReport(
      analyticsEntries({ ...data, transactions: [data.transactions[1]] }, 'income', ''),
      window,
    ).previous,
  ).toBeNull()
  const custom = analyticsWindow(data, { from: '2026-08-10', to: '2026-08-12' })
  expect(custom).toMatchObject({
    start: '2026-08-10',
    end: '2026-08-12',
    previousStart: '2026-08-07',
    previousEnd: '2026-08-09',
  })
})

it('extends the month chart through today without treating days after a stale snapshot as zero', () => {
  const data = { ...snapshot(), updatedAt: '2026-09-21T12:00:00Z' }
  data.transactions = [transaction('saved', 75, { date: '2026-09-21' })]
  const window = analyticsWindow(data, { range: 'month' }, '2026-09-25')
  const report = analyticsReport(analyticsEntries(data, 'income', ''), window)
  expect(window).toMatchObject({ start: '2026-08-27', end: '2026-09-25', savedEnd: '2026-09-21' })
  expect(report.buckets.at(-5)).toMatchObject({ from: '2026-09-21', value: 75 })
  expect(report.buckets.slice(-4).map(({ value }) => value)).toEqual([null, null, null, null])
  expect(report.previous).toBeNull()
})

it('validates incoming chart/date state and preserves account scope', () => {
  const search = analyticsSearch({
    chart: 'interest',
    range: 'year',
    from: '2026-02-30',
    to: 'bad',
    account: 'cash',
  })
  expect(search).toEqual({
    chart: 'interest',
    range: 'year',
    account: 'cash',
    from: undefined,
    to: undefined,
  })
  expect(analyticsWindow(snapshot(), { from: '2026-09-18', to: '2026-08-01' }).valid).toBe(false)
})

it('keeps account checkbox totals and bucket evidence exact through multiple and empty selections', () => {
  const data = snapshot()
  data.transactions = [
    transaction('cash-income', 100),
    transaction('card-income', 25, { accountId: 'card' }),
    transaction('foreign-income', 80, { accountId: 'foreign' }),
    transaction('pending-income', 50, { pending: true }),
    transaction('earlier-income', 60, { date: '2026-08-01' }),
  ]
  const available = ['cash', 'card']
  let scope = toggleAnalyticsAccount(undefined, 'cash', available)
  expect(analyticsAccountMatches(scope, 'cash')).toBe(false)
  expect(analyticsAccountMatches(scope, 'card')).toBe(true)
  expect(analyticsTotal(analyticsEntries(data, 'income', '', scope))).toBe(25)
  scope = toggleAnalyticsAccount(scope, 'card', available)
  expect(scope).toBe(noAnalyticsAccounts)
  expect(analyticsEntries(data, 'income', '', scope)).toEqual([])
  scope = toggleAnalyticsAccount(scope, 'cash', available)
  scope = toggleAnalyticsAccount(scope, 'card', available)
  const report = analyticsReport(
    analyticsEntries(data, 'income', '', scope),
    analyticsWindow(data, { range: 'month' }),
  )
  expect(report.total).toBe(125)
  const evidence = report.buckets.flatMap(({ entries }) => entries)
  expect(evidence.map(({ id }) => id)).toEqual(['spending:cash-income', 'spending:card-income'])
  expect(analyticsTotal(evidence)).toBe(report.total)
})

it('uses seven saved calendar days for W and preserves incoming YTD dates', () => {
  expect(analyticsWindow(snapshot(), { range: 'week' })).toMatchObject({
    start: '2026-09-12',
    end: '2026-09-18',
    previousStart: '2026-09-05',
    previousEnd: '2026-09-11',
  })
  expect(analyticsWindow(snapshot(), { range: 'year' })).toMatchObject({
    start: '2026-01-01',
    end: '2026-09-18',
  })
})

it('derives overview metrics from scoped records, retaining signs and unavailable estimates', () => {
  const window = analyticsWindow(snapshot(), { from: '2026-09-01', to: '2026-09-03' })
  const entries = [
    { id: 'a', date: '2026-09-01', value: 10.1, group: 'Source' },
    { id: 'b', date: '2026-09-01', value: -30.4, group: 'Source' },
    { id: 'c', date: '2026-09-03', value: 5.3, group: 'Source' },
    { id: 'outside', date: '2026-08-01', value: 1000, group: 'Source' },
  ]
  expect(analyticsReport(entries, window)).toMatchObject({
    average: -5,
    largest: -30.4,
    activeDays: 2,
  })
  expect(
    analyticsReport(
      [...entries, { id: 'unknown', date: '2026-09-02', value: null, group: 'Sale' }],
      window,
    ),
  ).toMatchObject({ average: null, largest: null, activeDays: 3 })
  expect(analyticsReport([], window)).toMatchObject({ average: null, largest: null, activeDays: 0 })
  expect(
    analyticsReport([{ id: 'zero', date: '2026-09-01', value: 0, group: 'Sale' }], window),
  ).toMatchObject({ average: 0, largest: 0, activeDays: 1 })
})

it('stacks separate activities by group without netting inflows against outflows', () => {
  const entries = [
    { id: 'b', group: 'B', date: '2026-09-01', value: 10.1 },
    { id: 'out', group: 'A', date: '2026-09-01', value: -12.3 },
    { id: 'a', group: 'A', date: '2026-09-01', value: 5.2 },
    { id: 'out2', group: 'B', date: '2026-09-01', value: -2.1 },
  ]
  const stack = analyticsBarStack(entries)
  expect(stack.positive).toBe(15.3)
  expect(stack.negative).toBe(-14.4)
  expect(stack.segments.map(({ id, start, end }) => ({ id, start, end }))).toEqual([
    { id: 'a', start: 0, end: 5.2 },
    { id: 'out', start: 0, end: -12.3 },
    { id: 'b', start: 5.2, end: 15.3 },
    { id: 'out2', start: -12.3, end: -14.4 },
  ])
  expect(
    analyticsBarStack([...entries, { id: 'unknown', group: 'A', date: '2026-09-01', value: null }]),
  ).toEqual({ positive: 0, negative: 0, segments: [] })
  expect(analyticsBarStack([])).toEqual({ positive: 0, negative: 0, segments: [] })
})

it('assigns distinct colors from the complete chart domain, independent of record order', () => {
  const groups = [
    'Income',
    'Purchases',
    'Interest',
    'Dividends',
    'Fees',
    'Taxes',
    'Reimbursements',
    'Other',
    'Extra',
  ]
  const colors = analyticsGroupColors([...groups, 'Income'])
  expect(colors.size).toBe(groups.length)
  expect(new Set(colors.values()).size).toBe(groups.length)
  expect(analyticsGroupColors(groups.toReversed())).toEqual(colors)
})

it('groups income by activity type while retaining individual evidence and totals', () => {
  const data = snapshot()
  data.transactions = [
    transaction('broker-a', 10, {
      merchant: 'TRANSFER MONEY FROM BROKERAGE XXXXX0001 REF A',
      classification: classification('income'),
    }),
    transaction('broker-b', 20, {
      merchant: 'Transfer money from brokerage XXXXX0002 REF B',
      classification: classification('income'),
    }),
    transaction('zelle-a', 30, {
      merchant: 'ZELLE PAYMENT FROM EXAMPLE PERSON A',
      category: 'Transfer',
      classification: classification('income', { zelle: true }),
    }),
    transaction('zelle-b', 40, {
      merchant: 'Zelle payment from example person B',
      category: 'Transfer',
      classification: classification('income', { zelle: true }),
    }),
    transaction('payroll', 50),
    transaction('interest', 2, {
      merchant: 'Interest payment',
      classification: classification('interest'),
    }),
    transaction('dividend', 3, {
      merchant: 'Dividend payment',
      classification: classification('dividend'),
    }),
    transaction('unrelated-a', 4, {
      merchant: 'Example long matching merchant prefix Alpha',
      classification: classification('income'),
    }),
    transaction('unrelated-b', 5, {
      merchant: 'Example long matching merchant prefix Beta',
      classification: classification('income'),
    }),
    transaction('excluded', 100, {
      merchant: 'Transfer money from brokerage XXXXX0003',
      category: 'Transfer',
      classification: classification('transfer'),
    }),
  ]
  const entries = analyticsEntries(data, 'income', 'card')
  expect(entries.map(({ group }) => group)).toEqual([
    'Brokerage transfers',
    'Brokerage transfers',
    'Zelle payments',
    'Zelle payments',
    'Payroll',
    'Interest',
    'Dividends',
    'Example long matching merchant prefix Alpha',
    'Example long matching merchant prefix Beta',
  ])
  expect(analyticsTotal(entries)).toBe(164)
  const report = analyticsReport(entries, analyticsWindow(data, { range: 'month' }))
  expect(report.breakdown).toEqual(
    expect.arrayContaining([
      { label: 'Brokerage transfers', value: 30, count: 2 },
      { label: 'Zelle payments', value: 70, count: 2 },
    ]),
  )
  expect(entries.map(({ id }) => id)).not.toContain('spending:excluded')
  expect(data.transactions[0].merchant).toBe('TRANSFER MONEY FROM BROKERAGE XXXXX0001 REF A')
})

it.each([
  ['interest', 'Interest payment', 2],
  ['dividends', 'Dividend payment', 3],
  ['fees', 'Bank fee', -4],
] as const)(
  'uses saved account names for %s chart groups with a provider-name fallback',
  (chart, merchant, amount) => {
    const data = snapshot()
    data.transactions = [
      transaction('example', amount, {
        merchant,
        category: chart === 'fees' ? 'Bank Fees' : 'Income',
        classification: classification(
          chart === 'fees' ? 'fee' : chart === 'dividends' ? 'dividend' : 'interest',
        ),
        account: 'Provider account',
      }),
    ]
    const entries = analyticsEntries(data, chart, 'card', undefined, { cash: 'Everyday account' })
    expect(entries[0].group).toBe('Everyday account')
    expect(entries[0].accountId).toBe('cash')
    expect(analyticsEntries(data, chart, 'card')[0].group).toBe('Provider account')
    expect(analyticsTotal(entries)).toBe(Math.abs(amount))
  },
)
