// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { emptySnapshot } from '../data/fixtures/records'
import { FinanceProvider } from './finance-provider'
import { LiveMarketProvider } from './live-market-provider'
import { useFinance } from './use-finance'
import { useLiveFinance } from './use-live-finance'

vi.mock('../lib/api', () => ({
  isTauri: () => false,
  getFinanceSnapshot: vi.fn(async () => ({ ...emptySnapshot(), revision: 3 })),
  getIntegrationStatus: vi.fn(async () => ({ plaid: false, alpaca: false })),
  getSavedMarket: vi.fn(),
  getSpendingPeriod: vi.fn(),
}))
vi.mock('../lib/annotations', () => ({
  hasLegacyAnnotations: () => false,
  loadTransactionAnnotations: async () => ({}),
}))
vi.mock('../lib/finance-settings', () => ({
  hasLegacySettings: () => false,
  importLegacyFinanceSettings: async () => undefined,
}))

const seen: { finance: unknown[]; live: unknown[] } = { finance: [], live: [] }

function Probe() {
  seen.finance.push(useFinance())
  seen.live.push(useLiveFinance())
  return null
}

let rerender: () => void = () => undefined

function Parent() {
  const [count, setCount] = useState(0)
  rerender = () => setCount(count + 1)
  return (
    <FinanceProvider>
      <LiveMarketProvider>
        <Probe />
      </LiveMarketProvider>
    </FinanceProvider>
  )
}

it('keeps the context values when a render above the providers changes nothing they read', async () => {
  const client = new QueryClient()
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <Parent />
      </QueryClientProvider>,
    ),
  )
  await act(async () => {
    await vi.waitFor(() => expect(latestData()).toBeDefined())
  })
  const finance = seen.finance.at(-1)
  const live = seen.live.at(-1)
  const renders = seen.finance.length
  await act(async () => rerender())
  expect(seen.finance.length).toBeGreaterThan(renders)
  expect(seen.finance.at(-1)).toBe(finance)
  expect(seen.live.at(-1)).toBe(live)
  await act(async () => root.unmount())
  client.clear()
})

function latestData() {
  const latest = seen.finance.at(-1)
  return latest && typeof latest === 'object' && 'data' in latest ? latest.data : undefined
}
