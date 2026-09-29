import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { expect, it, vi } from 'vitest'

import empty from '../data/empty.json'
import { LogsPage } from './logs'

const market = vi.hoisted(() => ({ state: 'active', saved: false }))
vi.mock('../components/workspace-header', () => ({ WorkspaceHeader: () => <h1>Diagnostics</h1> }))
vi.mock('../hooks/use-refresh-finance', () => ({ useFinanceRefreshState: () => false }))
vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
    data: { ...empty, revision: 1 },
    integrationStatus: { plaid: true, snaptrade: true, alpaca: true, alphaVantage: true },
    marketSymbols: ['TEST'],
    runtimeLog: [],
  }),
}))
vi.mock('../hooks/use-live-finance', () => ({
  useLiveFinance: () => ({
    data: { ...empty, holdings: [{ ticker: 'TEST', marketAsOf: '2026-09-18T20:00:00Z' }] },
    marketPriceState: market.state,
    marketIsSaved: market.saved,
    marketPriceMessage: market.state === 'error' ? 'Synthetic market failure' : undefined,
  }),
}))

function renderLogs(syncRuns: unknown[] = []) {
  const client = new QueryClient()
  client.setQueryData(['sync-runs', 1], syncRuns)
  const html = renderToString(
    <QueryClientProvider client={client}>
      <LogsPage />
    </QueryClientProvider>,
  )
  client.clear()
  return html
}

it('reads current market status and observation times and counts every configured provider', () => {
  market.state = 'active'
  market.saved = false
  const html = renderLogs()
  expect(html).toContain('<strong>Live</strong>')
  expect(html).toContain('dateTime="2026-09-18T20:00:00Z"')
  expect(html.replaceAll('<!-- -->', '')).toContain('4/4 configured')
})

it('keeps a failed market refresh visible when saved observations remain available', () => {
  market.state = 'error'
  market.saved = true
  const html = renderLogs()
  expect(html).toContain('Saved · unavailable')
  expect(html).toContain('Synthetic market failure')
})

it('shows a compact provider failure without exposing raw provider payloads', () => {
  const html = renderLogs([
    {
      id: 'run',
      startedAt: '2026-09-18T20:00:00Z',
      finishedAt: '2026-09-18T20:00:01Z',
      outcome: 'failed',
      warnings: [],
      errorCode: 'provider_refresh_failed',
      details: {
        phase: 'provider',
        warningCount: 0,
        providers: [
          {
            provider: 'Plaid',
            endpoint: '/accounts/balance/get',
            kind: 'provider',
            httpStatus: 429,
            errorCode: 'INSTITUTION_RATE_LIMIT',
            attempts: 1,
            retryable: true,
            retryAt: '2026-09-18T21:00:00Z',
          },
        ],
      },
    },
  ])

  expect(html).toContain('Plaid · INSTITUTION_RATE_LIMIT')
  expect(html).toContain('retry after')
  expect(html).not.toContain('/accounts/balance/get')
})
