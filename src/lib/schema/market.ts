/** Live market data: quotes, the valuation projection they update, price history, and holding chart events. */
import { z } from 'zod'

import { chartChangesSchema, rollupSchema } from './insights'
import { financeSnapshotSchema, performanceSchema } from './snapshot'

const marketSnapshotSchema = z.object({
  symbol: z.string(),
  price: z.number(),
  previousClose: z.number(),
  previousCloseAsOf: z.string().nullable(),
  dailyChangePct: z.number(),
  weeklyChangePct: z.number().nullable(),
  weeklyReferencePrice: z.number().positive().nullable(),
  weeklyReferenceDate: z.string().nullable(),
  asOf: z.string(),
})

export const marketProjectionSchema = financeSnapshotSchema
  .pick({
    revision: true,
    updatedAt: true,
    netWorth: true,
    netWorthIncomplete: true,
    accounts: true,
    holdings: true,
  })
  .extend({
    revision: z.number().int().nonnegative(),
    brokeragePerformance: z.array(performanceSchema.pick({ accountId: true, currentValue: true })),
    /** Account totals at these live values. */
    rollup: rollupSchema,
  })
export type MarketProjection = z.infer<typeof marketProjectionSchema>
const marketPointSchema = z.object({ time: z.number().int(), value: z.number() })

export const marketSnapshotsSchema = z.object({
  cached: z.boolean().optional(),
  snapshots: z.record(z.string(), marketSnapshotSchema),
  session: z.enum(['Regular market', 'Pre-market', 'After hours', 'Overnight', 'Market closed']),
  feed: z.enum(['iex', 'sip', 'delayed_sip', 'boats', 'overnight']),
  delayMinutes: z.number().int().nonnegative(),
  asOf: z.string().nullable(),
  nextTransitionAt: z.string().nullable(),
  pollIntervalMs: z.number().int().positive().nullable(),
  historyFeed: z.enum(['iex', 'sip', 'boats']),
  historyDelayMinutes: z.number().int().nonnegative(),
  chartSeries: z.record(z.string(), z.array(marketPointSchema)).optional(),
  chartPoint: z.record(z.string(), marketPointSchema).optional(),
  projection: marketProjectionSchema.optional(),
  /** Each value chart's change at these live values. */
  chartChanges: chartChangesSchema.optional(),
})
export type MarketSnapshots = z.infer<typeof marketSnapshotsSchema>
/** The portfolio's intraday week, sent once the stream's bars arrive. */
export const marketHistorySchema = z.record(z.string(), z.array(marketPointSchema))
export type MarketHistory = z.infer<typeof marketHistorySchema>

export const priceBarSchema = z.object({
  time: z.number().int(),
  endTime: z.number().int(),
  open: z.number().positive(),
  high: z.number().positive(),
  low: z.number().positive(),
  close: z.number().positive(),
  volume: z.number().nonnegative(),
  trades: z.number().int().nonnegative(),
  feed: z.enum(['sip', 'boats']),
})
export const holdingPriceHistorySchema = z.object({
  symbol: z.string(),
  range: z.number().int().nonnegative(),
  resolution: z.number().positive(),
  start: z.number().int(),
  end: z.number().int(),
  fetchedAt: z.number().int(),
  calendar: z
    .array(
      z
        .object({
          date: z.iso.date(),
          open: z.number().int().min(14400).max(72000),
          close: z.number().int().min(14400).max(72000),
        })
        .refine((day) => day.close > day.open),
    )
    .nullish(),
  bars: z.array(priceBarSchema),
  feeds: z.array(z.enum(['sip', 'boats'])),
  delayMinutes: z.number().nonnegative(),
  adjustment: z.literal('split'),
  cached: z.boolean(),
})
export type PriceBar = z.infer<typeof priceBarSchema>
export type HoldingPriceHistory = z.infer<typeof holdingPriceHistorySchema>
const holdingEventIdentity = { requestId: z.string(), symbol: z.string() }
export const holdingChartEventSchema = z.discriminatedUnion('kind', [
  z.object({
    ...holdingEventIdentity,
    kind: z.literal('history'),
    history: holdingPriceHistorySchema,
    startedAt: z.number(),
    warning: z.string().nullable().optional(),
  }),
  z.object({
    ...holdingEventIdentity,
    kind: z.literal('bar'),
    bar: priceBarSchema,
    receivedAt: z.number(),
  }),
  z.object({
    ...holdingEventIdentity,
    kind: z.literal('quote'),
    price: z.number().positive(),
    time: z.number(),
    feed: z.string(),
    indicative: z.boolean(),
  }),
  z.object({
    ...holdingEventIdentity,
    kind: z.literal('status'),
    status: z.enum([
      'connecting',
      'subscribing',
      'subscribed',
      'reconnecting',
      'closed',
      'unavailable',
      'rest',
    ]),
    session: z.string(),
    feed: z.string(),
    delayMinutes: z.number().nonnegative(),
  }),
  z.object({ ...holdingEventIdentity, kind: z.literal('invalidate'), feed: z.string() }),
  z.object({
    ...holdingEventIdentity,
    kind: z.enum(['error', 'history-error']),
    message: z.string(),
  }),
])
export type HoldingChartEvent = z.infer<typeof holdingChartEventSchema>
