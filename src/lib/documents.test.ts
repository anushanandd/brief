import { describe, expect, it } from 'vitest'

import { insights } from '../data/fixtures/insights'
import { document } from '../data/fixtures/records'
import { documentTitle, transactionDocuments } from './documents'
import type { BriefDocument, FinanceSnapshot } from './schema'

const statement: BriefDocument = document({
  id: 'a'.repeat(64),
  fileName: 'statement.pdf',
  importedAt: '2026-09-03T00:00:00Z',
  kind: 'statement',
  issuer: 'EXAMPLE BANK',
  periodStart: '2026-08-01',
  periodEnd: '2026-08-31',
})

describe('documents', () => {
  it('names a document by what it is and who issued it', () => {
    expect(documentTitle(statement)).toBe('Statement · Example Bank')
    expect(documentTitle({ ...statement, issuer: undefined, form: 'W-2' })).toBe('W-2')
  })

  it('links a transaction only to the documents Rust matched it to', () => {
    const data: Pick<FinanceSnapshot, 'documents' | 'insights'> = {
      documents: [statement],
      insights: insights({
        documents: {
          years: [],
          pay: [],
          keep: [],
          toReview: 0,
          outdated: [],
          evidence: [
            { transactionId: 'coffee', documentId: statement.id, kind: 'statement' },
            // A document since deleted links nothing.
            { transactionId: 'coffee', documentId: 'b'.repeat(64), kind: 'pay' },
          ],
        },
      }),
    }
    expect(transactionDocuments(data, 'coffee')).toEqual([
      { kind: 'statement', document: statement },
    ])
    expect(transactionDocuments(data, 'rent')).toEqual([])
  })
})
