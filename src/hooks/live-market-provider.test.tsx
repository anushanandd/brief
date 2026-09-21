import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { expect, it, vi } from 'vitest'

import empty from '../data/empty.json'
import { financeSnapshotSchema, type MarketSnapshots } from '../lib/schema'
import { LiveMarketProvider } from './live-market-provider'
import { useLiveFinance } from './use-live-finance'

vi.mock('../lib/api', () => ({ isTauri: () => true, getSavedMarket: vi.fn() }))
vi.mock('./use-finance', () => ({
  useFinance: () => ({
    data: { ...financeSnapshotSchema.parse(empty), revision: 4 },
    marketSymbols: ['TEST'],
    liveMarketEnabled: true,
    isLoading: false,
    integrationStatusLoading: false,
    appendRuntimeLog: vi.fn(),
  }),
}))

function Values() {
  const finance = useLiveFinance()
  return (
    <p>
      {JSON.stringify({
        value: finance.data?.netWorth,
        saved: finance.marketIsSaved,
        state: finance.marketPriceState,
        pending: finance.startupPending,
        note: finance.marketPriceNote,
        analysisReady: finance.analysisReady,
      })}
    </p>
  )
}

function renderSaved(revision: number) {
  const base = { ...financeSnapshotSchema.parse(empty), revision: 4 }
  const market: MarketSnapshots = {
    cached: true,
    snapshots: {},
    session: 'Market closed',
    feed: 'iex',
    delayMinutes: 0,
    asOf: '2026-09-18T20:00:00Z',
    nextTransitionAt: null,
    pollIntervalMs: 300000,
    historyFeed: 'iex',
    historyDelayMinutes: 0,
    projection: { ...base, revision, netWorth: 123, brokeragePerformance: [] },
  }
  const client = new QueryClient()
  client.setQueryData(['startup-market', 4, base.updatedAt], market)
  const html = renderToString(
    <QueryClientProvider client={client}>
      <LiveMarketProvider>
        <Values />
      </LiveMarketProvider>
    </QueryClientProvider>,
  )
  client.clear()
  return html.replaceAll('&quot;', '"')
}

it('uses a compatible saved valuation without presenting it as live or waiting for the network', () => {
  const html = renderSaved(4)
  expect(html).toContain('"value":123')
  expect(html).toContain('"saved":true')
  expect(html).toContain('"state":"loading"')
  expect(html).toContain('"pending":false')
  expect(html).toContain('Saved market observations')
  expect(html).toContain('"analysisReady":false')
})

it('discards a saved valuation from a different committed revision', () => {
  const html = renderSaved(3)
  expect(html).toContain('"value":0')
  expect(html).toContain('"saved":false')
})
