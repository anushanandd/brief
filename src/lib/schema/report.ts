/** A period reconciled in Rust: the statement identity, two labeled returns, the waterfall, risk, exposure, and the return split among the holdings. */
import { z } from 'zod'

import { isoDate } from './common'

export const statementSchema = z.object({
  window: z.enum(['day', 'week', 'month', 'quarter', 'year-to-date', 'year', 'all']).nullable(),
  start: isoDate,
  end: isoDate,
  startValue: z.number(),
  endValue: z.number(),
  deposits: z.number(),
  withdrawals: z.number(),
  netFlows: z.number(),
  investmentGain: z.number(),
  // How your investments did, and how your money did.
  timeWeightedPct: z.number(),
  moneyWeightedPct: z.number().nullable(),
  // Only for a year or more.
  annualized: z
    .object({ timeWeightedPct: z.number(), moneyWeightedPct: z.number().nullable() })
    .nullable(),
  benchmarkPct: z.number().nullable(),
  activePct: z.number().nullable(),
  waterfall: z.object({
    start: z.number(),
    deposits: z.number(),
    withdrawals: z.number(),
    realized: z.number().nullable(),
    unrealized: z.number().nullable(),
    priceGain: z.number(),
    dividends: z.number(),
    interest: z.number(),
    fees: z.number(),
    taxes: z.number(),
    end: z.number(),
  }),
  risk: z.object({
    drawdownPct: z.number().nullable(),
    volatilityPct: z.number().nullable(),
    beta: z
      .object({ beta: z.number(), rSquared: z.number(), sessions: z.number().int().positive() })
      .nullable(),
  }),
})
export type Statement = z.infer<typeof statementSchema>

export const exposureSchema = z.object({
  holdings: z.number().int().nonnegative(),
  effectiveHoldings: z.number(),
  topFivePct: z.number(),
})

const contributionSchema = z.object({
  // A ticker, or `cash` for cash and every holding without prices.
  key: z.string(),
  dollars: z.number(),
  contributionPct: z.number(),
  returnPct: z.number().nullable(),
  startWeightPct: z.number(),
  endWeightPct: z.number(),
  traded: z.boolean(),
  // The weight change split into what prices did and what trades did (none for cash).
  driftPct: z.number().nullable(),
  tradedPct: z.number().nullable(),
  buyAndHold: z.number().nullable(),
})

export const attributionSchema = z.object({
  contributions: z.array(contributionSchema),
  // Return drivers by dollars and top movers by their own return, chosen in Rust.
  gains: z.array(z.string()),
  losses: z.array(z.string()),
  rest: z.number(),
  risers: z.array(z.string()),
  fallers: z.array(z.string()),
  topThreeSharePct: z.number().nullable(),
  buyAndHold: z.number().nullable(),
  trading: z.number().nullable(),
  unpriced: z.array(z.string()),
})

export const periodReportSchema = z.object({
  statement: statementSchema.nullable(),
  exposure: exposureSchema.nullable(),
  attribution: attributionSchema.nullable(),
})
