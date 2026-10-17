import { describe, expect, it } from 'vitest'

import { evidenceFixture } from '../data/fixtures/evidence'
import { insights } from '../data/fixtures/insights'
import { emptySnapshot } from '../data/fixtures/records'
import { chapterSpan, emptyEvidence, explanationSpan, nowLine, sourceDue } from './evidence'
import { needView } from './needs'
import { objectQuery } from './objects'
import type { AttentionItem, FinanceSnapshot } from './schema'

describe('evidence words', () => {
  it('names where you are now only from open chapters', () => {
    expect(nowLine(emptyEvidence)).toBe('')
    expect(
      nowLine({
        ...emptyEvidence,
        now: { work: 'work:ms', workTitle: 'Morgan Stanley', home: 'California', month: 16 },
      }),
    ).toBe('Morgan Stanley · California · month 16')
    // Without a job, a month count would have nothing to count from.
    expect(nowLine({ ...emptyEvidence, now: { home: 'California' } })).toBe('California')
  })

  it('writes spans as dates', () => {
    expect(chapterSpan({ start: '2026-04-20' })).toBe('Since 2026-04-20')
    expect(chapterSpan({ start: '2025-01-01', end: '2026-03-31' })).toBe('2025-01-01 to 2026-03-31')
    expect(explanationSpan({ from: '2026-05-01', to: '2026-08-31' })).toBe(
      '2026-05-01 to 2026-08-31',
    )
    expect(explanationSpan({ from: '2026-07-15' })).toBe('From 2026-07-15')
    expect(explanationSpan({})).toBeUndefined()
  })

  it('reads a source again a month after its last read', () => {
    const now = new Date('2026-10-17T12:00:00Z')
    expect(sourceDue(undefined, now)).toBe(true)
    expect(sourceDue('2026-09-18T12:00:00Z', now)).toBe(false)
    expect(sourceDue('2026-09-17T12:00:00Z', now)).toBe(true)
  })
})

describe('rules and chapters elsewhere', () => {
  it('phrases a rule note with its rule and opens Rules', () => {
    const says = 'Deposits were under $5,000 in September 2026; the $55 monthly fee may apply.'
    const need = {
      id: 'program:waiver:watch:2026-09',
      severity: 'attention',
      kind: 'program-note',
      program: 'waiver',
      says,
      subject: 'account:plaid:cashplus',
    } as AttentionItem
    const data: FinanceSnapshot = {
      ...emptySnapshot(),
      insights: insights({
        evidence: {
          ...emptyEvidence,
          programs: [{ ...evidenceFixture.programs[0], id: 'waiver', name: 'Employee fee waiver' }],
        },
      }),
    }
    const view = needView(need, data)
    expect(view.title).toBe(says)
    expect(view.detail).toBe('Employee fee waiver')
    expect(view.amount).toBeUndefined()
    expect(view.link).toEqual({ to: '/insights', search: { view: 'rules' } })
  })

  it('limits search to chapters or rules by their prefix', () => {
    expect(objectQuery('@chapter morgan')).toEqual({ kinds: ['chapter'], text: 'morgan' })
    expect(objectQuery('rule waiver')).toEqual({ kinds: ['program'], text: 'waiver' })
  })
})
