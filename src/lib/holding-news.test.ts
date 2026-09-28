import { expect, it } from 'vitest'

import { rankHoldingNews } from './holding-news'
import type { MarketNewsArticle } from './schema'

const article = (overrides: Partial<MarketNewsArticle> = {}): MarketNewsArticle => ({
  headline: 'TEST raises earnings guidance after strong demand',
  summary: 'The company raised its guidance, according to its announcement.',
  source: 'Synthetic publisher',
  url: 'https://example.com/earnings',
  createdAt: '2026-09-20T11:00:00Z',
  symbols: ['TEST'],
  ...overrides,
})

it('shows newest stories first, groups near-duplicates, and preserves different figures', () => {
  const groups = rankHoldingNews(
    [
      article({
        headline: 'Markets open higher',
        url: 'https://example.com/market',
        createdAt: '2026-09-20T08:00:00Z',
        symbols: ['TEST', 'OTHER'],
      }),
      article(),
      article(),
      article({ url: 'https://example.com/syndicated' }),
      article({
        headline: 'TEST raises earnings guidance by 5 percent',
        url: 'https://example.com/five',
        createdAt: '2026-09-20T10:00:00Z',
      }),
      article({
        headline: 'TEST raises earnings guidance by 10 percent',
        url: 'https://example.com/ten',
        createdAt: '2026-09-20T09:00:00Z',
      }),
      article({ symbols: ['OTHER'], url: 'https://example.com/unrelated' }),
      article({ createdAt: 'invalid', url: 'https://example.com/invalid' }),
    ],
    'TEST',
  )
  expect(groups[0].article.url).toBe('https://example.com/earnings')
  expect(groups[0].related.map(({ url }) => url)).toEqual(['https://example.com/syndicated'])
  expect(groups.map(({ article: item }) => item.url)).toEqual([
    'https://example.com/earnings',
    'https://example.com/five',
    'https://example.com/ten',
    'https://example.com/market',
  ])
})

it('keeps chronology predictable instead of allowing provider scores to reorder the feed', () => {
  const groups = rankHoldingNews(
    [
      article({
        headline: 'Older high-relevance story',
        relevanceScore: 0.95,
        createdAt: '2026-09-20T10:00:00Z',
      }),
      article({
        headline: 'Newer low-relevance story',
        relevanceScore: 0,
        createdAt: '2026-09-20T12:00:00Z',
        url: 'https://example.com/newer',
      }),
    ],
    'TEST',
  )
  expect(groups.map(({ article: item }) => item.url)).toEqual([
    'https://example.com/newer',
    'https://example.com/earnings',
  ])
})
