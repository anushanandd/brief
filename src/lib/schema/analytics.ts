/** Money's and Investments' reports, computed in Rust over a window and accounts on request. */
import { z } from 'zod'

import { isoDate } from './common'

export const reportCharts = [
  'cash-flow',
  'income',
  'interest',
  'fees',
  'amex-credits',
  'dividends',
  'realized',
] as const
const chartSchema = z.enum(reportCharts)
const rangeSchema = z.enum(['day', 'week', 'month', 'quarter', 'year', 'all'])

const summarySchema = z.object({
  chart: chartSchema,
  /** Null when any entry has no value, such as a sale without a known cost. */
  total: z.number().nullable(),
  /** Null when the history doesn't reach the window before. */
  previous: z.number().nullable(),
  change: z.number().nullable(),
  percent: z.number().nullable(),
})

const flowItemSchema = z.object({
  id: z.string(),
  label: z.string(),
  value: z.number(),
  children: z.array(
    z.object({ id: z.string(), label: z.string(), value: z.number(), date: isoDate }),
  ),
})

export const reportsSchema = z.object({
  window: z.object({
    start: isoDate,
    end: isoDate,
    /** The snapshot's day: later days have no figures yet. */
    savedEnd: isoDate,
    previousStart: isoDate,
    previousEnd: isoDate,
    range: rangeSchema,
    custom: z.boolean(),
    valid: z.boolean(),
  }),
  /** Every report over the window and accounts. */
  summaries: z.array(summarySchema),
  /** The requested report in full. */
  report: summarySchema
    .extend({
      entries: z.array(
        z.object({
          id: z.string(),
          date: isoDate,
          value: z.number().nullable(),
          accountId: z.string().optional(),
          group: z.string(),
        }),
      ),
      average: z.number().nullable(),
      largest: z.number().nullable(),
      activeDays: z.number().int(),
      breakdown: z.array(
        z.object({ label: z.string(), value: z.number().nullable(), count: z.number().int() }),
      ),
      buckets: z.array(
        z.object({
          from: isoDate,
          to: isoDate,
          value: z.number().nullable(),
          count: z.number().int(),
          entryIds: z.array(z.string()),
          /** Gains and losses apart, for the chart's scale. */
          positive: z.number(),
          negative: z.number(),
          groups: z.array(
            z.object({
              name: z.string(),
              negative: z.boolean(),
              total: z.number(),
              entryIds: z.array(z.string()),
            }),
          ),
        }),
      ),
      unit: z.enum(['day', 'month', 'year']),
    })
    .optional(),
  /** The requested report's total in each account. */
  accounts: z.array(z.object({ accountId: z.string(), total: z.number().nullable() })).default([]),
  /** The requested report's groups across every account and day. */
  groups: z.array(z.string()).default([]),
  /** The recurring series behind the requested report's entries on any day. */
  recurringIds: z.array(z.string()).default([]),
  flow: z
    .object({
      sources: z.array(flowItemSchema),
      purchases: z.array(flowItemSchema),
      fees: z.number(),
      taxes: z.number(),
      inflow: z.number(),
      outflow: z.number(),
      /** More went out than came in. */
      gap: z.number(),
      /** More came in than went out. */
      retained: z.number(),
    })
    .optional(),
})
export type Reports = z.infer<typeof reportsSchema>
export type Report = NonNullable<Reports['report']>
export type ReportSummary = Reports['summaries'][number]
export type ReportEntry = Report['entries'][number]
export type CashFlow = NonNullable<Reports['flow']>
export type CashFlowItem = CashFlow['sources'][number]
