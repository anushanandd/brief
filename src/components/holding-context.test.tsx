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

it('shows concise ticker headlines and scores without requesting a model', () => {
  vi.stubGlobal('window', {})
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(['holding-news', 'alpha-vantage', 'TEST'], {
    articles: [
      {
        headline: 'Synthetic news',
        summary: 'Synthetic evidence from the company announcement.',
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
    expect(html).toContain('Relevance')
    expect(html).toContain('Sentiment')
    expect(html).not.toContain('Alpha Vantage relevance')
    expect(html).not.toContain('Alpha Vantage sentiment')
    expect(html).toContain('Synthetic news')
    expect(html).not.toContain('Synthetic evidence from the company announcement.')
    expect(html).toContain('Request budget exhausted')
    expect(html).toContain('Refresh news (uses one API request)')
    expect(html).not.toContain('local news requests left')
    expect(html).not.toContain('Saved Sep')
    expect(html).toContain('Somewhat Bearish')
    expect(html).not.toContain('Avg sentiment')
    expect(client.getQueryCache().find({ queryKey: ['foundation-model-status'] })).toBeUndefined()
    const disconnected = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="TEST" connected={false} />
      </QueryClientProvider>,
    )
    expect(disconnected).toContain('Synthetic news')
    expect(disconnected).toContain(
      'Connect Alpha Vantage in Settings to refresh. Saved stories remain available.',
    )
  } finally {
    client.clear()
  }
})
