import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import nativeSummary from '../data/fixtures/native-summary.json'
import { seedSnapshot } from '../data/fixtures/records'
import { summaryStart } from '../lib/investment-summary'
import { fromRust } from '../lib/rust-json'
import type { InvestmentSummary } from '../lib/schema'
import { EarningsReport } from './earnings-report'
import { summaryKey } from './investment-summary-card'

vi.mock('../lib/api', () => ({ isTauri: () => true, getInvestmentSummary: vi.fn() }))
afterEach(() => vi.unstubAllGlobals())

// A holding's summary as Rust serializes it; a Rust test keeps this fixture in sync.
const summary = fromRust<InvestmentSummary>(nativeSummary)
const [digest] = summary.earnings
if (!digest) throw new Error('The native summary has a report')

function render(value: InvestmentSummary) {
  vi.stubGlobal('window', {})
  const data = seedSnapshot()
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(summaryKey(summaryStart(data, 'year'), 'TEST', data), value)
  try {
    return renderToString(
      <QueryClientProvider client={client}>
        <EarningsReport data={data} ticker="TEST" range="year" />
      </QueryClientProvider>,
    ).replaceAll('<!-- -->', '')
  } finally {
    client.clear()
  }
}

it('tells the latest report in Rust’s words and quotes the release and the call', () => {
  const html = render(summary)
  expect(html.replace(/<[^>]+>/g, '')).toContain(digest.line.map((span) => span.text).join(''))
  expect(html).toContain('The options market priced in a move of 7.9% either way.')
  expect(html).toContain(`<q>${digest.revenue}</q>`)
  expect(html).toContain(`<q>${digest.guidance[0]}</q>`)
  // The call's guidance is credited to the local model that chose it.
  expect(html).toContain('On the call')
  expect(html).toContain(
    'title="Chosen by example on this Mac from management’s remarks on the call"',
  )
  // Analysts' expectation is theirs, with how it moved.
  expect(html).toContain(
    'Analysts expect $0.45 a share for the quarter ending Nov 30, up 4.7% in 30 days (6 raised, 1 cut).',
  )
  expect(html).toContain('The options market prices in 8.4% either way around the Dec 17 report.')
  expect(html).toContain(`href="${digest.releaseUrl}"`)
})

it('says plainly when there is no report or the release isn’t read', () => {
  expect(render({ ...summary, earnings: [] })).toContain(
    'No earnings report in the last four months.',
  )
  const unread = render({
    ...summary,
    earnings: [{ ...digest, guidance: [], releaseRead: false, callGuidance: [] }],
  })
  expect(unread).toContain('The release hasn’t been read yet.')
  expect(unread).not.toContain('On the call')
})
