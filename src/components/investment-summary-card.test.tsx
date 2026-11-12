import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import nativeSummary from '../data/fixtures/native-summary.json'
import { seedSnapshot } from '../data/fixtures/records'
import { getInvestmentSummary } from '../lib/api'
import { summaryStart } from '../lib/investment-summary'
import { fromRust } from '../lib/rust-json'
import type { InvestmentSummary } from '../lib/schema'
import { EarningsReport } from './earnings-report'
import { InvestmentSummaryCard, summaryKey } from './investment-summary-card'

vi.mock('../lib/api', () => ({
  isTauri: () => true,
  getInvestmentSummary: vi.fn(),
  warmInvestmentSummaries: vi.fn(),
}))
afterEach(() => vi.unstubAllGlobals())

// A holding's summary as Rust serializes it; a Rust test keeps this fixture in sync.
const summary = fromRust<InvestmentSummary>(nativeSummary)

/** The holding's page as it opens on the summary Rust answered with from what it saved. */
function render(value: InvestmentSummary) {
  vi.stubGlobal('window', {})
  const data = seedSnapshot()
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(summaryKey(summaryStart(data, 'year'), 'TEST', data), value)
  try {
    return renderToString(
      <QueryClientProvider client={client}>
        <InvestmentSummaryCard data={data} range="year" ticker="TEST" />
        <EarningsReport data={data} ticker="TEST" range="year" />
      </QueryClientProvider>,
    ).replaceAll('<!-- -->', '')
  } finally {
    client.clear()
  }
}

it('shows a saved summary at once, with no loading state and no request', () => {
  const html = render({ ...summary, refreshing: false })
  expect(html).not.toContain('Gathering the day')
  expect(html).not.toContain('skeleton')
  expect(html).not.toContain('Updating')
  expect(html).toContain('class="investment-brief"')
  // The cached answer stands; nothing asks Rust again on render.
  expect(getInvestmentSummary).not.toHaveBeenCalled()
})

it('marks an older saved summary as updating, and keeps its figures out of the other cards', () => {
  const html = render({ ...summary, refreshing: true })
  const card = html.slice(
    html.indexOf('investment-summary-card'),
    html.indexOf('holding-earnings-card'),
  )
  expect(card).not.toContain('Gathering the day')
  expect(card).toContain('role="status">Updating</span>')
  expect(card).toContain('class="investment-brief"')
  // The earnings card waits for the current summary rather than showing the older one's.
  const earnings = html.slice(html.indexOf('holding-earnings-card'))
  expect(earnings).toContain('Reading reports…')
  expect(earnings).not.toContain('<q>')
})
