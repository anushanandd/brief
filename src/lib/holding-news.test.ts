import { expect, it } from 'vitest'

import { averageNewsSentiment, rankHoldingNews } from './holding-news'
import type { MarketNewsArticle } from './schema'

const now = Date.parse('2026-09-20T12:00:00Z')
const article = (overrides: Partial<MarketNewsArticle> = {}): MarketNewsArticle => ({
  headline: 'TEST raises earnings guidance after strong demand',
  summary: 'The company raised its guidance, according to its announcement.',
  source: 'Synthetic publisher',
  url: 'https://example.com/earnings',
  createdAt: '2026-09-20T11:00:00Z',
  symbols: ['TEST'],
  ...overrides,
})

it('ranks material company news, groups near-duplicates, and preserves different figures', () => {
  const groups = rankHoldingNews(
    [
      article({
        headline: 'Markets open higher',
        url: 'https://example.com/market',
        symbols: ['TEST', 'OTHER'],
      }),
      article(),
      article(),
      article({ url: 'https://example.com/syndicated' }),
      article({
        headline: 'TEST raises earnings guidance by 5 percent',
        url: 'https://example.com/five',
      }),
      article({
        headline: 'TEST raises earnings guidance by 10 percent',
        url: 'https://example.com/ten',
      }),
      article({ symbols: ['OTHER'], url: 'https://example.com/unrelated' }),
    ],
    'TEST',
    now,
  )
  expect(groups[0].article.url).toBe('https://example.com/earnings')
  expect(groups[0].related.map(({ url }) => url)).toEqual(['https://example.com/syndicated'])
  expect(groups).toHaveLength(4)
  expect(groups.at(-1)?.article.headline).toBe('Markets open higher')
})

it('prioritizes provider ticker relevance while keeping neutral scores available', () => {
  const groups = rankHoldingNews(
    [
      article({ headline: 'Lower relevance', relevanceScore: 0, sentimentScore: 0 }),
      article({
        headline: 'Direct company announcement',
        relevanceScore: 0.95,
        url: 'https://example.com/direct',
      }),
    ],
    'TEST',
    now,
  )
  expect(groups[0].article.relevanceScore).toBe(0.95)
  expect(groups[1].article.sentimentScore).toBe(0)
})

it('averages scored primary stories without treating missing values as neutral or counting related copies', () => {
  const groups = [
    { article: article({ sentimentScore: 0.6 }), related: [article({ sentimentScore: -1 })] },
    { article: article({ sentimentScore: 0 }), related: [] },
    { article: article({ sentimentScore: -0.3 }), related: [] },
    { article: article({ sentimentScore: null }), related: [] },
  ]
  const average = averageNewsSentiment(groups)
  expect(average.value).toBeCloseTo(0.1)
  expect(average.count).toBe(3)
  expect(averageNewsSentiment([])).toEqual({ value: null, count: 0 })
})
