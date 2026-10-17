import { describe, expect, it } from 'vitest'

import {
  reconciliationSummary,
  reviewCount,
  statementCheckLabel,
  statementCountsLabel,
  statementDateRange,
  statementProvider,
} from './statement-import'

const check = {
  status: 'verified' as const,
  totalMatches: true,
  balanceRows: 0,
  balanceExplained: 0,
}
const counts = { added: 0, covered: 0, repeated: 0, pending: 0, missing: 0 }

describe('statement import copy', () => {
  it('states what the statement was checked against', () => {
    expect(statementCheckLabel(check)).toBe('Total matches')
    expect(statementCheckLabel({ ...check, balanceRows: 4, balanceExplained: 4 })).toBe(
      'Total and balances match',
    )
    expect(
      statementCheckLabel({ ...check, totalMatches: null, balanceRows: 4, balanceExplained: 4 }),
    ).toBe('Balances match')
    expect(
      statementCheckLabel({
        status: 'partial',
        totalMatches: null,
        balanceRows: 38,
        balanceExplained: 35,
      }),
    ).toBe('Balances match for 35 of 38 rows')
    expect(statementCheckLabel({ ...check, status: 'unverified', totalMatches: null })).toBe(
      'No total or balances to check',
    )
    expect(statementCheckLabel({ ...check, status: 'failed', totalMatches: false })).toBe(
      'Total doesn’t match the rows',
    )
  })

  it('names what an import adds beside the provider and earlier statements', () => {
    expect(statementProvider('plaid:cash')).toBe('Plaid')
    expect(statementProvider('snaptrade:brokerage')).toBe('SnapTrade')
    expect(
      statementCountsLabel(
        { added: 23, covered: 16, repeated: 0, pending: 1, missing: 2 },
        'plaid:cash',
        '2026-09-07',
      ),
    ).toBe(
      'Adds 23 activities before Sep 7, 2026 · 16 already from Plaid · 1 pending skipped · 2 Plaid never reported',
    )
    expect(statementCountsLabel({ ...counts, added: 1 }, 'plaid:cash')).toBe('Adds 1 activity')
    expect(statementCountsLabel({ ...counts, covered: 5 }, 'snaptrade:brokerage')).toBe(
      'Adds nothing; SnapTrade covers these dates',
    )
    expect(statementCountsLabel({ ...counts, repeated: 5 }, 'plaid:cash')).toBe(
      'Adds nothing new · 5 in earlier statements',
    )
  })

  it('sums what reconciliation left to review and otherwise says how far it reaches', () => {
    const reconciled = {
      accountId: 'plaid:card',
      from: '2026-05-09',
      through: '2026-09-21',
      matched: ['a', 'b'],
      amountDiffers: [],
      missing: [
        {
          fileId: 'f',
          rowId: 'r1',
          date: '2026-08-09',
          amount: -8,
          description: 'X',
          confirmed: true,
        },
      ],
      extra: [],
    }
    expect(reviewCount(reconciled)).toBe(0)
    expect(reconciliationSummary(reconciled)).toEqual({
      headline: 'Reconciled through Sep 21, 2026',
      detail: '2 matched since May 9, 2026',
      review: 0,
    })
    const open = {
      ...reconciled,
      missing: [{ ...reconciled.missing[0], confirmed: false }],
      extra: ['c'],
    }
    expect(reconciliationSummary(open).headline).toBe('2 to review')
  })

  it('formats statement date ranges as calendar days', () => {
    expect(statementDateRange('2026-01-16', '2026-08-25')).toBe('Jan 16, 2026 – Aug 25, 2026')
    expect(statementDateRange('2026-01-16', '2026-01-16')).toBe('Jan 16, 2026')
    expect(statementDateRange(null, null)).toBeUndefined()
  })
})
