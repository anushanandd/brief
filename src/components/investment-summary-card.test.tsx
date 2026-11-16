// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import nativeSummary from '../data/fixtures/native-summary.json'
import { seedSnapshot } from '../data/fixtures/records'
import { getInvestmentSummary } from '../lib/api'
import { setGraphEnd } from '../lib/graph-preferences'
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
  expect(html).not.toContain('updating')
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
  expect(card).toContain(' · updating</small>')
  expect(card).toContain('class="investment-brief"')
  // The earnings card waits for the current summary rather than showing the older one's.
  const earnings = html.slice(html.indexOf('holding-earnings-card'))
  expect(earnings).toContain('Updating…')
  expect(earnings).not.toContain('<q>')
})

it('describes today’s period while the page is scrolled back, from the same query', async () => {
  const data = seedSnapshot()
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(summaryKey(summaryStart(data, 'year'), 'TEST', data), {
    ...summary,
    refreshing: false,
  })
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <InvestmentSummaryCard data={data} range="year" ticker="TEST" />
      </QueryClientProvider>,
    ),
  )
  const today = host.innerHTML
  await act(async () => setGraphEnd('2026-01-15'))
  expect(host.innerHTML).toBe(today)
  expect(host.innerHTML).toContain('class="investment-brief"')
  expect(getInvestmentSummary).not.toHaveBeenCalled()
  expect(client.getQueryCache().findAll({ queryKey: ['investment-summary'] })).toHaveLength(1)
  await act(async () => root.unmount())
  setGraphEnd(undefined)
  client.clear()
})
