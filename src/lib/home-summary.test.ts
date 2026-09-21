import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import seed from '../data/seed.json'
import { buildHomeSummary, homeSummaryParts, homeSummaryTiming } from './home-summary'
import { financeSnapshotSchema } from './schema'

const savedSnapshot = () => {
  const data = financeSnapshotSchema.parse(seed)
  data.updatedAt = '2026-09-20T12:00:00Z'
  data.accountMovements = []
  data.lastChange = undefined
  data.trades = []
  data.holdings = []
  data.transactions = []
  data.netWorthIncomplete = false
  return data
}

it('ranks a recent saved account movement above ordinary activity', () => {
  const data = financeSnapshotSchema.parse(seed)
  const summary = buildHomeSummary(data, 7 * 86400, undefined, data.updatedAt.slice(0, 10))
  expect(summary.context).toMatchObject({
    id: '2026-08-31T21:42:00Z:fidelity',
    kind: 'account-movement',
    score: 95,
  })
  expect(summary.sentences[0]).toBe(
    'Individual brokerage (Fidelity · brokerage) changed by +$1,294.22.',
  )
  expect(summary.sentences[0]).not.toContain('2026-08-31')
})

it('rewards new, recent transactions without letting a larger old transaction always win', () => {
  const data = savedSnapshot()
  const template = financeSnapshotSchema.parse(seed).transactions[1]
  data.transactions = [
    {
      ...template,
      id: 'large-old',
      merchant: 'Large Old Purchase',
      amount: -500,
      postedOn: '2026-09-01',
      date: '2026-09-01',
    },
    {
      ...template,
      id: 'small-new',
      merchant: 'Recent Purchase',
      amount: -25,
      postedOn: '2026-09-19',
      date: '2026-09-19',
    },
  ]
  data.lastChange = {
    observedAt: data.updatedAt,
    previousUpdatedAt: '2026-09-19T12:00:00Z',
    previousNetWorth: data.netWorth,
    netWorthChange: 0,
    accountChanges: [],
    newTransactionIds: ['small-new'],
  }
  const summary = buildHomeSummary(data, 30 * 86400, undefined, '2026-09-20')
  expect(summary.context).toMatchObject({ id: 'small-new', kind: 'transaction' })
  expect(summary.sentences[0]).toContain('$25.00')
})

it('excludes equal-and-opposite movements and transactions for an internal transfer', () => {
  const data = savedSnapshot()
  const template = financeSnapshotSchema.parse(seed).transactions[1]
  data.transactions = [
    {
      ...template,
      id: 'checking-out',
      accountId: 'checking',
      merchant: 'Transfer to savings',
      amount: -2200,
      postedOn: '2026-09-18',
      date: '2026-09-18',
      classification: classification('transfer'),
    },
    {
      ...template,
      id: 'savings-in',
      accountId: 'savings',
      merchant: 'Transfer from checking',
      amount: 2200,
      postedOn: '2026-09-18',
      date: '2026-09-18',
      classification: classification('transfer'),
    },
    {
      ...template,
      id: 'groceries',
      accountId: 'checking',
      merchant: 'Example Grocery Market',
      amount: -40,
      postedOn: '2026-09-19',
      date: '2026-09-19',
      classification: classification('expense'),
    },
  ]
  data.accountMovements = [
    {
      id: 'movement-checking',
      observedAt: '2026-09-18T12:00:00Z',
      accountId: 'checking',
      name: 'Max-Rate Checking',
      change: -2200,
    },
    {
      id: 'movement-savings',
      observedAt: '2026-09-18T12:00:00Z',
      accountId: 'savings',
      name: 'Savings',
      change: 2200,
    },
  ]

  const summary = buildHomeSummary(data, 7 * 86400, undefined, '2026-09-20')
  expect(summary.context).toMatchObject({ id: 'groceries', kind: 'transaction' })
  expect(summary.sentences[0]).not.toContain('$2,200.00')
})

it('uses saved trades and only states realized return when FIFO evidence is complete', () => {
  const data = savedSnapshot()
  data.trades = [
    {
      id: 'sale',
      type: 'sale',
      date: '2026-09-19',
      amount: 1200,
      account: 'Brokerage',
      accountId: data.accounts[0].id,
      ticker: 'TEST',
      realizedCostBasis: 1000,
      estimatedRealizedGain: 200,
      estimatedRealizedGainPct: 20,
      realizedGainMethod: 'estimated-fifo',
    },
  ]
  const summary = buildHomeSummary(data, 7 * 86400, undefined, '2026-09-20')
  expect(summary.context).toMatchObject({ id: 'sale', kind: 'trade' })
  expect(summary.sentences[0]).toContain('estimated return of +20.00%')
})

it('selects an upcoming recurring event and penalizes repeating its latest transaction', () => {
  const data = savedSnapshot()
  const template = financeSnapshotSchema.parse(seed).transactions[1]
  data.transactions = [
    ...['2026-09-03', '2026-09-10', '2026-09-17'].map((date, index) => ({
      ...template,
      id: `membership-${index}`,
      merchant: 'Example Membership',
      category: 'Subscriptions',
      amount: -12,
      postedOn: date,
      date,
      classification: classification('expense'),
    })),
    {
      ...template,
      id: 'groceries',
      merchant: 'Example Grocery Market',
      amount: -20,
      postedOn: '2026-09-18',
      date: '2026-09-18',
      classification: classification('expense'),
    },
  ]
  const summary = buildHomeSummary(data, 30 * 86400, undefined, '2026-09-20')
  expect(summary.forward).toMatchObject({ kind: 'upcoming' })
  expect(summary.sentences[1]).toBe(
    'Estimated payment of $12.00 for Example Membership in 4 days (9/24).',
  )
  expect(summary.context.id).toBe('groceries')
})

it('uses explicit gaps when no candidate clears the score threshold', () => {
  const data = savedSnapshot()
  data.accounts = []
  const summary = buildHomeSummary(data, 7 * 86400, undefined, '2026-09-20')
  expect(summary.context.kind).toBe('gap')
  expect(summary.forward.kind).toBe('gap')
  expect(summary.sentences).toEqual([
    'No recent financial detail was found in the saved period.',
    'No upcoming estimate or benefit deadline was found in saved data.',
  ])

  data.netWorthIncomplete = true
  expect(buildHomeSummary(data, 7 * 86400, undefined, '2026-09-20').context.kind).toBe('warning')
})

it('emphasizes supported figures and deadlines without assigning spending or dates a financial tone', () => {
  const text = 'Holding +2.00%, benchmark -1.00%, amount $50.00; reset Sep 30, unknown $99.00.'
  const parts = homeSummaryParts(text, '+2.00% -1.00% $50.00 Sep 30')
  expect(parts.map((part) => part.text).join('')).toBe(text)
  expect(parts.filter(({ emphasis }) => emphasis)).toEqual([
    { text: '+2.00%', emphasis: true, tone: 'positive' },
    { text: '-1.00%', emphasis: true, tone: 'negative' },
    { text: '$50.00', emphasis: true, tone: undefined },
    { text: 'Sep 30', emphasis: true, tone: undefined },
  ])
})

it('formats payment timing using calendar days across month, year and daylight-saving boundaries', () => {
  expect(homeSummaryTiming('2026-10-01', '2026-09-20')).toBe('in 11 days (10/1)')
  expect(homeSummaryTiming('2027-01-01', '2026-12-31')).toBe('in 1 day (1/1)')
  expect(homeSummaryTiming('2026-11-02', '2026-11-01')).toBe('in 1 day (11/2)')
  expect(homeSummaryTiming('2026-10-01', '2026-10-01')).toBe('today (10/1)')
})
