import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { HoldingEarnings } from './holding-earnings'
import { HoldingNews } from './holding-news'

vi.mock('../lib/api', () => ({
  isTauri: () => true,
  getMarketNews: vi.fn(),
  getEarningsCalendar: vi.fn(),
  getFoundationModelStatus: vi.fn(),
  generateFoundationExplanation: vi.fn(),
}))
afterEach(() => vi.unstubAllGlobals())

it('shows the selected holding’s next earnings date without leaking another ticker’s calendar', () => {
  vi.stubGlobal('window', {})
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(['earnings-calendar', ['AAA']], [{ symbol: 'AAA', reportDate: '2026-10-22' }])
  client.setQueryData(['earnings-calendar', ['BBB']], [{ symbol: 'BBB', reportDate: '2026-11-05' }])
  try {
    const html = renderToString(
      <QueryClientProvider client={client}>
        <HoldingEarnings symbols={['AAA']} connected />
      </QueryClientProvider>,
    )
    expect(html).toContain('<span>Next earnings</span>')
    expect(html).toContain('dateTime="2026-10-22"')
    expect(html).not.toContain('2026-11-05')
    client.setQueryData(['earnings-calendar', ['AAA']], [])
    const empty = renderToString(
      <QueryClientProvider client={client}>
        <HoldingEarnings symbols={['AAA']} connected />
      </QueryClientProvider>,
    )
    expect(empty).toContain('aria-label="Next earnings date unavailable"')
  } finally {
    client.clear()
  }
})

it('shows filings among company stories, and folds away stories that only mention it', () => {
  vi.stubGlobal('window', {})
  const client = new QueryClient({ defaultOptions: { queries: { gcTime: Infinity } } })
  client.setQueryData(['holding-news', 'alpaca', 'TEST'], {
    articles: [
      {
        headline: 'TEST Announces Synthetic Acquisition',
        summary: 'Synthetic evidence from the company announcement.',
        source: 'Example',
        url: 'https://example.com/story',
        createdAt: '2026-09-21T12:00:00Z',
        symbols: ['TEST'],
        scope: 'company',
        tags: ['deal'],
        relevance: 100,
      },
      {
        headline: 'Synthetic Bank Raises Target on PEER, Citing TEST',
        summary: '',
        source: 'Example',
        url: 'https://example.com/peer',
        createdAt: '2026-09-21T13:00:00Z',
        symbols: ['PEER', 'BANK', 'TEST'],
        scope: 'company',
        tags: [],
        relevance: 30,
      },
      {
        headline: 'Stock Market Today: Synthetic Index Climbs',
        summary: '',
        source: 'Example',
        url: 'https://example.com/recap',
        createdAt: '2026-09-21T11:00:00Z',
        symbols: ['TEST'],
        scope: 'market',
        tags: [],
        relevance: 25,
      },
    ],
    savedAt: '2026-09-21T12:00:00Z',
    warning: 'Synthetic refresh failure',
    canRefresh: false,
    company: 'Synthetic Corp',
    filings: [
      {
        form: '8-K',
        filedAt: '2026-09-20T20:05:00Z',
        url: 'https://www.sec.gov/Archives/edgar/data/1/1/synthetic.htm',
        items: [{ code: '2.02', label: 'Earnings results' }],
        title: 'Earnings results',
      },
    ],
    filingsSavedAt: '2026-09-21T12:00:00Z',
    filingsWarning: null,
  })
  try {
    const html = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="TEST" connected secConnected />
      </QueryClientProvider>,
    )
    expect(html).toContain('TEST Announces Synthetic Acquisition')
    expect(html).not.toContain('Synthetic evidence from the company announcement.')
    expect(html).toContain('Deal')
    expect(html).not.toContain('SEC filings')
    // The filing sits among company stories, newest first, each story with its relevance.
    const company = html.slice(html.indexOf('news-list'), html.indexOf('other mentions'))
    expect(company.indexOf('TEST Announces')).toBeLessThan(company.indexOf('Earnings results'))
    expect(company).toContain('SEC 8-K')
    expect(company).toContain('SEC filing')
    expect(company).toContain('aria-label="Relevance 100 of 100"')
    expect(company).not.toContain('Synthetic Bank Raises Target')
    // A broker note on a peer and a roundup only mention the company, so they fold away.
    const mentions = html.slice(html.indexOf('2 other mentions'))
    expect(mentions.indexOf('Synthetic Bank Raises Target')).toBeLessThan(
      mentions.indexOf('Stock Market Today'),
    )
    expect(mentions).toContain('news-relevance news-relevance-low')
    expect(html).toContain('Synthetic refresh failure')
    expect(html).toContain('aria-label="Refresh news"')
    expect(client.getQueryCache().find({ queryKey: ['foundation-model-status'] })).toBeUndefined()
    const disconnected = renderToString(
      <QueryClientProvider client={client}>
        <HoldingNews ticker="TEST" connected={false} secConnected={false} />
      </QueryClientProvider>,
    )
    expect(disconnected).toContain('TEST Announces Synthetic Acquisition')
    expect(disconnected).toContain('Connect Alpaca in Settings to refresh.')
    // Saved filings stay visible after the contact is removed.
    expect(disconnected).toContain('Earnings results')
  } finally {
    client.clear()
  }
})
