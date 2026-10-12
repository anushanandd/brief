import { describe, expect, it } from 'vitest'

import { statementPreviewSchema, statementSummarySchema } from './schema'
import {
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
const counts = { added: 0, covered: 0, repeated: 0, pending: 0 }

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
        { added: 23, covered: 16, repeated: 0, pending: 1 },
        'plaid:cash',
        '2026-09-07',
      ),
    ).toBe('Adds 23 activities before Sep 7, 2026 · 16 already from Plaid · 1 pending skipped')
    expect(statementCountsLabel({ ...counts, added: 1 }, 'plaid:cash')).toBe('Adds 1 activity')
    expect(statementCountsLabel({ ...counts, covered: 5 }, 'snaptrade:brokerage')).toBe(
      'Adds nothing; SnapTrade covers these dates',
    )
    expect(statementCountsLabel({ ...counts, repeated: 5 }, 'plaid:cash')).toBe(
      'Adds nothing new · 5 in earlier statements',
    )
  })

  it('formats statement date ranges as calendar days', () => {
    expect(statementDateRange('2026-01-16', '2026-08-25')).toBe('Jan 16, 2026 – Aug 25, 2026')
    expect(statementDateRange('2026-01-16', '2026-01-16')).toBe('Jan 16, 2026')
    expect(statementDateRange(null, null)).toBeUndefined()
  })
})

describe('statement import payloads', () => {
  it('accept the nulls Rust sends for absent values', () => {
    const preview = statementPreviewSchema.parse({
      draftId: 'a'.repeat(64),
      fileName: 'statement.csv',
      label: null,
      firstDate: '2026-01-16',
      lastDate: '2026-08-25',
      rows: 3,
      trades: 0,
      moneyIn: 250,
      moneyOut: 35.75,
      check: { ...check, totalMatches: null, status: 'unverified' },
      datesAssumed: true,
      suggestedAccountId: null,
      importedAccountId: null,
      accounts: [{ accountId: 'plaid:cash', counts, providerStart: null }],
    })
    expect(preview.accounts[0].providerStart).toBeNull()
    const summary = statementSummarySchema.parse({
      id: 'a'.repeat(64),
      fileName: 'statement.csv',
      accountId: 'plaid:cash',
      importedAt: '2026-09-09T11:00:00Z',
      firstDate: null,
      lastDate: null,
      rows: 0,
      counts,
      check: null,
    })
    expect(summary.check).toBeNull()
  })
})
