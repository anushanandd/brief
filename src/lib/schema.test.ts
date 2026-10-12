import { describe, expect, it } from 'vitest'

import empty from '../data/empty.json'
import nativeFinance from '../data/fixtures/native-finance.json'
import nativeNews from '../data/fixtures/native-news.json'
import nativeSummary from '../data/fixtures/native-summary.json'
import seedInsights from '../data/seed-insights.json'
import {
  investmentSummarySchema,
  marketNewsSchema,
  marketNewsResultSchema,
  syncRunSchema,
} from './schema'
import { financeSnapshotSchema, insightsSchema } from './schema'

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

it('preserves news tags and rejects unknown tags and unsafe links', () => {
  const article = {
    headline: 'Synthetic news',
    summary: '',
    source: 'Example',
    url: 'https://example.com/story',
    createdAt: '2026-09-21T12:00:00Z',
    symbols: ['TEST'],
    scope: 'company',
    tags: ['earnings'],
    relevance: 90,
  }
  expect(marketNewsSchema.parse([article])[0]).toEqual(article)
  expect(marketNewsSchema.safeParse([{ ...article, relevance: 101 }]).success).toBe(false)
  expect(marketNewsSchema.safeParse([{ ...article, tags: ['sentiment'] }]).success).toBe(false)
  expect(marketNewsSchema.safeParse([{ ...article, scope: 'other' }]).success).toBe(false)
  expect(marketNewsSchema.safeParse([{ ...article, url: 'http://example.com' }]).success).toBe(
    false,
  )
  expect(marketNewsSchema.safeParse([{ ...article, createdAt: 'not-a-date' }]).success).toBe(false)
})

it('preserves native saved news, relevance, and filings across IPC', () => {
  expect(marketNewsResultSchema.parse(nativeNews)).toEqual(nativeNews)
  const filing = nativeNews.filings[0]
  expect(
    marketNewsResultSchema.safeParse({
      ...nativeNews,
      filings: [{ ...filing, url: 'https://example.com/not-sec' }],
    }).success,
  ).toBe(false)
})

it('validates bounded refresh diagnostics at the IPC boundary', () => {
  const run = {
    id: 'run',
    startedAt: '2026-09-21T12:00:00Z',
    finishedAt: '2026-09-21T12:00:02Z',
    outcome: 'failed',
    warnings: [],
    errorCode: 'provider_refresh_failed',
    details: {
      phase: 'provider',
      warningCount: 0,
      providers: [
        {
          provider: 'Plaid',
          endpoint: '/transactions/sync',
          kind: 'provider',
          httpStatus: 429,
          errorCode: 'INSTITUTION_RATE_LIMIT',
          requestRef: '0123456789ab',
          attempts: 1,
          retryable: true,
          retryAt: '2026-09-21T13:00:00Z',
        },
      ],
    },
  } as const

  expect(syncRunSchema.parse(run)).toEqual(run)
  expect(
    syncRunSchema.safeParse({
      ...run,
      details: { ...run.details, providers: [{ ...run.details.providers[0], attempts: 9 }] },
    }).success,
  ).toBe(false)
})

it('preserves native insights computed by Rust from the synthetic seed', () => {
  const parsed = insightsSchema.parse(seedInsights.insights)
  expect(parsed).toEqual(seedInsights.insights)
  expect(parsed.benefitOptions.length).toBeGreaterThan(0)
  expect(parsed.benefits?.accountId).toBe(seedInsights.spendingAccountId)
})

it('preserves a native investment summary and rejects unsafe links and unknown states', () => {
  expect(investmentSummarySchema.parse(nativeSummary)).toEqual(nativeSummary)
  const event = nativeSummary.events[0]
  const withEvent = (changes: Record<string, unknown>) => ({
    ...nativeSummary,
    events: [{ ...event, ...changes }],
  })
  expect(investmentSummarySchema.safeParse(withEvent({ url: 'http://example.com' })).success).toBe(
    false,
  )
  expect(investmentSummarySchema.safeParse(withEvent({ family: 'rumor' })).success).toBe(false)
  // Rank components are bounded buckets, never a free score.
  expect(
    investmentSummarySchema.safeParse(withEvent({ rank: { ...event.rank, zBucket: 9 } })).success,
  ).toBe(false)
  // A headline links only over https.
  const brief = nativeSummary.brief
  expect(
    investmentSummarySchema.safeParse({
      ...nativeSummary,
      brief: {
        ...brief,
        headlines: [{ ...brief.headlines[0], url: 'javascript:alert(1)' }],
      },
    }).success,
  ).toBe(false)
  // Reported quarters link only to SEC EDGAR.
  expect(
    investmentSummarySchema.safeParse({
      ...nativeSummary,
      fundamentals: { ...nativeSummary.fundamentals, url: 'https://example.com/report' },
    }).success,
  ).toBe(false)
})
