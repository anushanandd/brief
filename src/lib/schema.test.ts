import { describe, expect, it } from 'vitest'

import empty from '../data/empty.json'
import nativeFinance from '../data/fixtures/native-finance.json'
import nativeNews from '../data/fixtures/native-news.json'
import { marketNewsSchema, marketNewsResultSchema } from './schema'
import { financeSnapshotSchema } from './schema'

describe('finance snapshot migrations', () => {
  it('derives cost basis for snapshots saved before the field existed', () => {
    const holding = {
      ticker: 'TEST',
      name: 'Example',
      accountId: 'example',
      shares: 1,
      price: 120,
      value: 120,
      dailyChangePct: 0,
      totalChangePct: 20,
      color: '#000',
    }
    const parse = (overrides: Record<string, unknown>) =>
      financeSnapshotSchema.parse({
        ...empty,
        holdings: [{ ...holding, ...overrides }],
      }).holdings[0].costBasis
    expect(parse({})).toBe(100)
    expect(parse({ costBasis: 80 })).toBe(80)
    expect(parse({ costBasis: null })).toBeNull()
    expect(parse({ totalChangePct: -100 })).toBeNull()
    expect(parse({ value: null })).toBeNull()
  })
})

it('accepts the native serialized contract without losing fields or changing values', () => {
  expect(financeSnapshotSchema.parse(nativeFinance)).toEqual(nativeFinance)
})

it('preserves Alpha Vantage ticker scores and rejects values outside their provider scales', () => {
  const article = {
    headline: 'Synthetic news',
    summary: '',
    source: 'Example',
    url: 'https://example.com/story',
    createdAt: '2026-09-21T12:00:00Z',
    symbols: ['TEST'],
    relevanceScore: 0.91,
    sentimentScore: -0.3,
    sentimentLabel: 'Somewhat-Bearish',
  }
  expect(marketNewsSchema.parse([article])[0]).toEqual(article)
  expect(marketNewsSchema.safeParse([{ ...article, relevanceScore: 2 }]).success).toBe(false)
  expect(
    marketNewsSchema.safeParse([{ ...article, sentimentScore: null, sentimentLabel: null }])
      .success,
  ).toBe(true)
})

it('preserves native saved news, sentiment and quota status across IPC', () => {
  expect(marketNewsResultSchema.parse(nativeNews)).toEqual(nativeNews)
  expect(marketNewsResultSchema.safeParse({ ...nativeNews, requestsRemaining: -1 }).success).toBe(
    false,
  )
})
