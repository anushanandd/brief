// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { emptySnapshot } from '../data/fixtures/records'
import { useRuntimeLog, type RuntimeLogEntry } from '../lib/runtime-log'
import { FinanceProvider } from './finance-provider'
import { LiveMarketProvider } from './live-market-provider'
import { setFinanceSnapshot, useFinance } from './use-finance'
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

let log: readonly RuntimeLogEntry[] = []
function LogProbe() {
  log = useRuntimeLog()
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
        <LogProbe />
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

it('renders the app once for a new snapshot, with its log line kept outside', async () => {
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
  const renders = seen.finance.length
  await act(async () => {
    setFinanceSnapshot(client, { ...emptySnapshot(), revision: 4 })
    await vi.waitFor(() => expect(latestData()).toMatchObject({ revision: 4 }))
    await new Promise((resolve) => setTimeout(resolve, 20))
  })
  expect(seen.finance.length).toBe(renders + 1)
  expect(log.at(0)?.message).toBe('Finance snapshot loaded')
  await act(async () => root.unmount())
  client.clear()
})

function latestData() {
  const latest = seen.finance.at(-1)
  return latest && typeof latest === 'object' && 'data' in latest ? latest.data : undefined
}
