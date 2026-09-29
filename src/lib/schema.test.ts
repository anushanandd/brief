import { describe, expect, it } from 'vitest'

import empty from '../data/empty.json'
import nativeFinance from '../data/fixtures/native-finance.json'
import nativeNews from '../data/fixtures/native-news.json'
import {
  marketNewsSchema,
  marketNewsResultSchema,
  plaidRecurringReportSchema,
  syncRunSchema,
} from './schema'
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
  expect(marketNewsSchema.safeParse([{ ...article, createdAt: 'not-a-date' }]).success).toBe(false)
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

it('preserves Plaid recurring evidence across IPC', () => {
  const report = {
    fetchedAt: '2026-09-21T12:00:00Z',
    connections: [
      {
        itemId: 'item',
        name: 'Example Bank',
        error: null,
        streams: [
          {
            streamId: 'stream',
            accountId: 'plaid:account',
            direction: 'outflow',
            description: 'Example membership',
            merchantName: 'Example',
            category: 'ENTERTAINMENT',
            frequency: 'MONTHLY',
            status: 'MATURE',
            isActive: true,
            firstDate: '2026-06-01',
            lastDate: '2026-09-01',
            predictedNextDate: '2026-10-01',
            averageAmount: { amount: 12.5, currency: 'USD' },
            lastAmount: { amount: 13, currency: 'USD' },
            transactionCount: 4,
          },
        ],
      },
    ],
  } as const

  expect(plaidRecurringReportSchema.parse(report)).toEqual(report)
  expect(
    plaidRecurringReportSchema.safeParse({
      ...report,
      connections: [{ ...report.connections[0], streams: [{ direction: 'guess' }] }],
    }).success,
  ).toBe(false)
})
