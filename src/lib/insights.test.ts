import { describe, expect, it } from 'vitest'

import seed from '../data/seed.json'
import {
  briefingEvidence,
  fallbackWeeklyBriefing,
  homeInsightSections,
  weeklyBriefingCandidate,
} from './insights'
import { financeSnapshotSchema } from './schema'

describe('weeklyBriefingCandidate', () => {
  it('treats a large weekly move as significant even for a small position', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.lastChange = undefined
    data.holdings.forEach((holding) => (holding.weeklyChangePct = null))
    data.holdings[0].weeklyChangePct = 9

    expect(weeklyBriefingCandidate(data)?.stock?.ticker).toBe('VTI')

    data.holdings[0].weeklyChangePct = 5
    data.holdings[0].value = 10
    expect(weeklyBriefingCandidate(data)).toBeUndefined()
  })

  it('detects a material net-worth change and includes newly observed evidence', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.updatedAt = '2026-09-06T20:00:00Z'
    data.transactions[0].amount = -1_000
    data.lastChange = {
      observedAt: data.updatedAt,
      previousUpdatedAt: '2026-08-30T20:00:00Z',
      previousNetWorth: data.netWorth - 1_000,
      netWorthChange: 1_000,
      comparisonComplete: true,
      accountChanges: [{ accountId: 'chase', name: 'Checking', change: 1_000 }],
      newTransactionIds: ['tx-1'],
    }

    const candidate = weeklyBriefingCandidate(data)
    expect(candidate?.key.startsWith('2026-08-31:')).toBe(true)
    expect(candidate?.netWorth?.change).toBe(1_000)
    expect(candidate?.transactions[0]?.id).toBe('tx-1')
    expect(briefingEvidence(candidate!)).toContain('possible contributors')
    expect(briefingEvidence(candidate!)).not.toMatch(/[$%]/)
    expect(fallbackWeeklyBriefing(candidate!)).toContain('United Airlines')
  })

  it('treats a card payment as notable weekly activity', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.updatedAt = '2026-09-09T20:00:00Z'
    data.lastChange = undefined
    data.holdings.forEach((holding) => (holding.weeklyChangePct = null))
    data.transactions = [
      {
        ...data.transactions[0],
        id: 'amex-payment',
        merchant: 'American Express Payment',
        category: 'Loan Payments',
        date: 'Sep 8, 2026',
        amount: -300,
      },
    ]

    const candidate = weeklyBriefingCandidate(data)
    expect(candidate?.transactions[0]?.merchant).toBe('American Express Payment')
    expect(fallbackWeeklyBriefing(candidate!)).toContain('-$300.00')
  })

  it('adds available news without claiming a catalyst', () => {
    const data = financeSnapshotSchema.parse(seed)
    data.lastChange = undefined
    data.holdings[0].weeklyChangePct = 9
    data.transactions[0].postedOn = data.updatedAt.slice(0, 10)
    data.transactions[0].pending = false
    const candidate = weeklyBriefingCandidate(data)!
    const evidence = briefingEvidence(candidate, [
      {
        headline: 'Company reports results',
        summary: 'Revenue increased.',
        source: 'Example',
        url: 'https://example.com/news',
        createdAt: '2026-08-31T12:00:00Z',
      },
    ])

    expect(evidence).toContain('Company reports results')
    expect(briefingEvidence(candidate)).toContain('news is unavailable')
    expect(fallbackWeeklyBriefing(candidate)).toContain(
      "This week's holding highlight was VTI, which moved +9.00%",
    )
    const sections = homeInsightSections(data, candidate, 'Recent results may be relevant.')
    expect(sections.map(({ title }) => title)).toEqual([
      'Key changes',
      'Daily summary',
      'Weekly summary',
    ])
    expect(sections[1].lines[0]).toContain('Today included')
    expect(sections[2].lines[0]).toContain('The last seven days included')
    for (const section of sections) {
      expect(
        [...new Intl.Segmenter('en', { granularity: 'sentence' }).segment(section.lines.join(' '))]
          .length,
      ).toBeLessThanOrEqual(2)
    }
    for (const refusal of ['NO_CONTEXT', "I can't add context without repeating values."]) {
      expect(
        homeInsightSections(data, candidate, refusal).flatMap(({ lines }) => lines),
      ).not.toContain(refusal)
    }
  })
})
