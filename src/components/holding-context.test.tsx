import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { HoldingNews } from './holding-news'

vi.mock('../lib/api', () => ({
  isTauri: () => true,
  getMarketNews: vi.fn(),
  getEarningsCalendar: vi.fn(),
  getFoundationModelStatus: vi.fn(),
  generateFoundationExplanation: vi.fn(),
}))
afterEach(() => vi.unstubAllGlobals())

it('shows the selected holding’s earnings date inside News without leaking another ticker’s calendar', () => {
  vi.stubGlobal('window', {})
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(['earnings-calendar', ['AAA']], [{ symbol: 'AAA', reportDate: '2026-10-22' }])
  client.setQueryData(['earnings-calendar', ['BBB']], [{ symbol: 'BBB', reportDate: '2026-11-05' }])
  client.setQueryData(['holding-news', 'alpha-vantage', 'AAA'], {
    articles: [],
    savedAt: null,
    warning: null,
    requestsRemaining: 20,
    canRefresh: true,
  })
  try {
    const html = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="AAA" connected />
      </QueryClientProvider>,
    )
    expect(html).toContain('<h2>News</h2>')
    expect(html).toContain('dateTime="2026-10-22"')
    expect(html).not.toContain('2026-11-05')
    client.setQueryData(['earnings-calendar', ['AAA']], [])
    const empty = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="AAA" connected />
      </QueryClientProvider>,
    )
    expect(empty).toContain('<span class="muted">—</span>')
  } finally {
    client.clear()
  }
})

it('shows provider ticker scores without requesting a model or thesis', () => {
  vi.stubGlobal('window', {})
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(['holding-news', 'alpha-vantage', 'TEST'], {
    articles: [
      {
        headline: 'Synthetic news',
        summary: '',
        source: 'Example',
        url: 'https://example.com/story',
        createdAt: '2026-09-21T12:00:00Z',
        symbols: ['TEST'],
        relevanceScore: 0.91,
        sentimentScore: -0.3,
        sentimentLabel: 'Somewhat-Bearish',
      },
    ],
    savedAt: '2026-09-21T12:00:00Z',
    warning: 'Request budget exhausted',
    requestsRemaining: 0,
    canRefresh: false,
  })
  try {
    const html = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="TEST" connected />
      </QueryClientProvider>,
    )
    expect(html).toContain('91%')
    expect(html).toContain('Synthetic news')
    expect(html).toContain('Request budget exhausted')
    expect(html).toContain('Refresh news (uses one API request)')
    expect(html).toContain('Somewhat Bearish')
    expect(html).toContain('Average sentiment -0.30, across 1 scored stories')
    expect(client.getQueryCache().find({ queryKey: ['foundation-model-status'] })).toBeUndefined()
    expect(client.getQueryCache().find({ queryKey: ['holding-thesis'] })).toBeUndefined()
  } finally {
    client.clear()
  }
})
