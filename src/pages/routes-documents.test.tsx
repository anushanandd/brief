// Documents.

import { expect, it } from 'vitest'

import { document as record } from '../data/fixtures/records'
import type { BriefDocument } from '../lib/schema'
import { renderRoute } from './routes.harness'
import { scenario } from './routes.scenario'

const id = (letter: string) => letter.repeat(64)

function document(overrides: Partial<BriefDocument>): BriefDocument {
  return record({
    id: id('a'),
    fileName: 'Payslip.pdf',
    importedAt: '2026-09-03T00:00:00Z',
    kind: 'payslip',
    ...overrides,
  })
}

it('lists a year’s documents beside what the year should bring', async () => {
  scenario.documents = [
    document({
      form: 'Pay statement',
      issuer: 'Example Services Group Inc',
      taxYear: 2026,
      date: '2026-08-28',
      fields: [
        { key: 'net', label: 'Net pay', amount: 2_937.35, ytd: 20_882.84 },
        { key: 'pretax401k', label: '401(k) pre-tax', amount: 240.38, uncertain: true },
      ],
    }),
    document({
      id: id('b'),
      kind: 'statement',
      form: 'Statement',
      issuer: 'Example Bank',
      taxYear: 2026,
      date: '2026-08-31',
      accountId: 'other',
      statementRows: 4,
      statementCsv: 'Date,Description,Amount\n',
      fields: [{ key: 'endingBalance', label: 'Ending balance', amount: 1_482.89 }],
    }),
    document({
      id: id('c'),
      kind: 'taxForm',
      form: 'W-2',
      issuer: 'Older Employer',
      taxYear: 2025,
    }),
  ]
  scenario.insights = {
    documents: {
      years: [
        {
          year: 2026,
          predictions: [],
          income: {
            wages: 29_471.25,
            wagesFrom: 'payStatements',
            selfEmployment: 1_500,
            selfEmploymentTax: 211.95,
          },
        },
      ],
      pay: [],
      keep: [],
      toReview: 1,
      outdated: [],
      evidence: [],
    },
  }
  const html = (await renderRoute('/documents')).replaceAll('<!-- -->', '')
  expect(html).toContain('Pay statement · Example Services Group Inc')
  expect(html).toContain('1 to check')
  // Statements an account can take are offered together; last year's W-2 stays in 2025.
  expect(html).toContain('Add 1 to Other card')
  expect(html).not.toContain('Older Employer')
  expect(html).toContain('2 in 2026<span class="is-attention">1 to check</span>')
  expect(html).toContain('documents-badge is-attention')
  expect(html).toContain('aria-label="Search documents"')
  expect(html).not.toContain('data-command="documents.read-all"')
  // Each year's mark counts its documents.
  expect(html).toContain('aria-label="2 documents"')
  // Coverage marks the months each monthly kind arrived in.
  expect(html).toContain('aria-label="August: 1 pay statement"')
  expect(html).toContain('aria-label="August: 1 statement"')
  expect(html).toContain('Wages<small>Pay statements</small>')
  expect(html).toContain('Self-employment tax<small>Nothing withheld</small>')
  expect(html).toContain('href="#settings/tax-filing-status"')

  const opened = (await renderRoute(`/documents?document=${id('a')}`)).replaceAll('<!-- -->', '')
  expect(opened).toContain('aria-label="401(k) pre-tax this period"')
  expect(opened).toContain('1 figure to compare with the original')
  expect(opened).toContain('title="Compare with the original">Check</small>')
  expect(opened).toContain('data-command="documents.view"')
  expect(opened).toContain('Open in Preview')
  expect(opened).toContain('Mark checked')
  expect(opened).not.toContain('Wages<small>')
})

it('shows a summary only with its source, and offers to read again what an older reader read', async () => {
  scenario.documents = [
    document({ taxYear: 2026, summary: 'A pay statement from Example Services for August.' }),
  ]
  scenario.insights = {
    documents: { years: [], pay: [], keep: [], toReview: 0, outdated: [id('a')], evidence: [] },
  }
  const opened = (await renderRoute(`/documents?document=${id('a')}`)).replaceAll('<!-- -->', '')
  expect(opened).toContain('A pay statement from Example Services for August.')
  expect(opened).toContain('Apple Intelligence, on this Mac')
  const html = (await renderRoute('/documents')).replaceAll('<!-- -->', '')
  expect(html).toContain('data-command="documents.read-all"')
  expect(html).toContain('Reread 1')
})

it('says how to start when there are no documents', async () => {
  scenario.documents = []
  const html = await renderRoute('/documents')
  expect(html).toContain('read on this Mac and kept encrypted')
  expect(html).toContain('data-command="documents.add-folder"')
})
