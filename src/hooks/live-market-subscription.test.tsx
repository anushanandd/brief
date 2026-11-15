// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { emptySnapshot } from '../data/fixtures/records'
import { subscribeLiveMarket } from '../lib/live-market'
import { LiveMarketProvider } from './live-market-provider'

const finance = vi.hoisted(() => ({ revision: 1, symbols: ['TEST'] }))

vi.mock('../lib/api', () => ({ isTauri: () => true, getSavedMarket: vi.fn() }))
vi.mock('../lib/live-market', async (actual) => ({
  ...(await actual<typeof import('../lib/live-market')>()),
  subscribeLiveMarket: vi.fn(() => () => undefined),
}))
vi.mock('./use-finance', () => ({
  useFinance: () => ({
    data: { ...emptySnapshot(), revision: finance.revision },
    marketSymbols: finance.symbols,
    liveMarketEnabled: true,
    isLoading: false,
    integrationStatusLoading: false,
  }),
}))

it('a refresh keeps the market stream and only new symbols restart it', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  const client = new QueryClient()
  const root = createRoot(document.createElement('div'))
  const render = () =>
    act(async () =>
      root.render(
        <QueryClientProvider client={client}>
          <LiveMarketProvider>{null}</LiveMarketProvider>
        </QueryClientProvider>,
      ),
    )
  await render()
  expect(subscribeLiveMarket).toHaveBeenCalledTimes(1)
  finance.revision = 2
  await render()
  expect(subscribeLiveMarket).toHaveBeenCalledTimes(1)
  finance.symbols = ['TEST', 'OTHER']
  await render()
  expect(subscribeLiveMarket).toHaveBeenCalledTimes(2)
  await act(async () => root.unmount())
  client.clear()
  vi.unstubAllGlobals()
})
