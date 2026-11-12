import { describe, expect, it } from 'vitest'

import { emptySnapshot, transaction } from '../data/fixtures/records'
import { buildActivities } from './activity'
import { confidenceLabel, inferredNeedSubjects, needView } from './needs'
import type { AttentionItem, FinanceSnapshot } from './schema'

const pay = {
  id: 'pay-differs:pay-12',
  severity: 'attention',
  kind: 'pay-differs',
  recurringId: 'pay',
  title: 'Example Employer Payroll',
  accountId: 'plaid:checking',
  amount: 2_400,
  low: 2_000,
  high: 2_000,
  date: '2026-09-11',
  transactionId: 'pay-12',
  confidence: 10 / 12,
  evidence: ['pay-10', 'pay-11', 'pay-12'],
} satisfies AttentionItem

const fees = {
  id: 'fees:fee-b',
  severity: 'attention',
  kind: 'fees-charged',
  since: '2026-08-17',
  count: 2,
  amount: -15,
  transactionId: 'fee-b',
  confidence: 0.9,
  evidence: ['fee-a', 'fee-b'],
} satisfies AttentionItem

const missed = {
  id: 'missed:rent:2026-09-01',
  severity: 'attention',
  kind: 'missed-bill',
  recurringId: 'rent',
  title: 'Example Property',
  expected: '2026-09-01',
  days: 15,
  amount: -1_500,
  event: 'payment',
  sourceTransactionId: 'rent-08',
} satisfies AttentionItem

const row = (id: string, amount: number) =>
  transaction({ id, date: '2026-09-11', amount, accountId: 'plaid:checking' })

describe('inferred needs', () => {
  it('phrase themselves with how sure Rust is and open their transaction', () => {
    const view = needView(pay, emptySnapshot())
    expect(view.title).toBe('Example Employer Payroll paid more than usual')
    expect(view.detail).toBe('Usually $2,000.00')
    expect(view.confidence).toBeCloseTo(0.833)
    expect(view.evidence).toEqual(['pay-10', 'pay-11', 'pay-12'])
    expect(view.link).toEqual({ to: '/activities', search: { open: 'pay-12' } })
    expect(confidenceLabel(view.confidence!)).toBe('83% sure')
    // A need read from records carries no confidence.
    expect(needView(missed, emptySnapshot()).confidence).toBeUndefined()
  })

  it('belong to the transactions they are about, not their whole history', () => {
    const subjects = inferredNeedSubjects([pay, fees, missed])
    expect([...subjects.keys()]).toEqual(['pay-12', 'fee-a', 'fee-b'])
    expect(subjects.get('pay-10')).toBeUndefined()
  })

  it('flag their rows for the review queue', () => {
    const base = emptySnapshot()
    const data: FinanceSnapshot = {
      ...base,
      transactions: [row('pay-12', 2_400), row('pay-11', 2_000), row('fee-a', -3)],
      insights: base.insights && {
        ...base.insights,
        brief: { needs: [pay, fees] },
      },
    }
    const flags = Object.fromEntries(
      buildActivities(data).map(({ id, flag }) => [id.replace('spending:', ''), flag]),
    )
    expect(flags).toEqual({ 'pay-12': 'pay', 'pay-11': undefined, 'fee-a': 'fee' })
  })
})
