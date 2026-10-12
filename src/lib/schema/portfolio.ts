/** The portfolio's day, its period drivers, the investment summary, sector exposure, and earnings. */
import { z } from 'zod'

import { isoDate } from './common'
import { periodReportSchema } from './report'

const benchmarkSchema = z.object({
  symbol: z.string(),
  name: z.string(),
  changePct: z.number(),
})
const positionTodaySchema = z.object({
  ticker: z.string(),
  name: z.string(),
  changePct: z.number(),
  change: z.number(),
  sector: benchmarkSchema.nullable(),
})
export const portfolioTodaySchema = z.object({
  change: z.number(),
  changePct: z.number().nullable(),
  market: benchmarkSchema.nullable(),
  vsMarket: z.enum(['ahead', 'behind', 'inline']).nullable(),
  incomplete: z.boolean(),
  positions: z.array(positionTodaySchema),
  notable: z.boolean(),
  asOf: z.string().nullable(),
})
export const portfolioNewsSchema = z.object({
  refreshed: z.boolean(),
  stories: z.number().int().nonnegative(),
  filings: z.number().int().nonnegative(),
})
const moveKindSchema = z.enum(['quiet', 'withSector', 'withMarket', 'ownMove'])
// What a layer figure was measured with, so the surface can caveat it.
const fitSchema = z.object({
  marketBeta: z.number(),
  fund: z.string().nullable(),
  groupBeta: z.number().nullable(),
  rSquared: z.number().min(0).max(1),
  window: z.number().int().positive(),
  shrunk: z.boolean(),
})
// A holding's market, group, and own layers summed over the period's sessions.
const holdingLayersSchema = z.object({
  marketPct: z.number(),
  groupPct: z.number(),
  ownPct: z.number(),
  ownZ: z.number().nullable(),
  typicalPct: z.number().nonnegative().nullable(),
  fit: fitSchema.nullable(),
  sessions: z.number().int().nonnegative(),
})
const storyLinkSchema = z.object({
  title: z.string(),
  url: z.string().url().startsWith('https://'),
  publisher: z.string(),
  relevance: z.number().int().min(0).max(100).nullable(),
  publishedAt: z.string().nullable(),
})

// The day's brief, written in Rust. React styles each span and never joins words of its own.
const spanSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('text'), text: z.string() }),
  z.object({
    kind: z.literal('figure'),
    text: z.string(),
    tone: z.enum(['up', 'down', 'flat']),
    sensitive: z.boolean(),
  }),
  z.object({ kind: z.literal('subject'), text: z.string(), ticker: z.string().nullable() }),
])
export type BriefSpan = z.infer<typeof spanSchema>
// A headline that explains the day, quoted as published and credited.
const headlineSchema = z.object({
  // The day's movers it names; none for a market wrap.
  tickers: z.array(z.string()),
  title: z.string(),
  publisher: z.string(),
  url: z.string().url().startsWith('https://'),
  publishedAt: z.string(),
})
export const briefSchema = z.object({
  lines: z.array(z.array(spanSchema)),
  headlines: z.array(headlineSchema),
})
export type Brief = z.infer<typeof briefSchema>

const eventStatusSchema = z.enum(['scheduled', 'confirmed', 'estimated', 'revised', 'retracted'])
// One event as the summary lists it: where it came from, its three times, and its rank's
// components, never a score.
export const listedEventSchema = z.object({
  id: z.string(),
  family: z.enum(['ledger', 'corporateAction', 'disclosure', 'macro', 'news', 'statistical']),
  kind: z.string(),
  subtype: z.string().nullable(),
  ticker: z.string().nullable(),
  session: isoDate,
  at: z.string().nullable(),
  publishedAt: z.string().nullable(),
  ingestedAt: z.string(),
  title: z.string(),
  url: z.string().url().startsWith('https://').nullable(),
  publisher: z.string(),
  sourceLabel: z.string(),
  reaction: z.string().nullable(),
  provenance: z.enum(['sourced', 'observed', 'reported']),
  status: eventStatusSchema,
  effect: z.number().nullable(),
  z: z.number().nullable(),
  rank: z.object({
    dollarBucket: z.number().int().min(0).max(4),
    zBucket: z.number().int().min(0).max(3),
    prior: z.number().int().min(1).max(3),
    novel: z.boolean(),
  }),
  chain: z.string().nullable(),
  coverage: z.array(storyLinkSchema),
  flags: z.array(z.string()),
  relevance: z.number().int().min(0).max(100).nullable(),
})
export type ListedEvent = z.infer<typeof listedEventSchema>

export const investmentSummarySchema = z.object({
  start: isoDate,
  // The latest session that had opened; outside regular hours today is that session's close.
  end: isoDate,
  session: z.object({
    date: isoDate,
    today: isoDate,
    state: z.enum(['preMarket', 'open', 'afterHours', 'closed']),
    closesAt: z.string().datetime({ offset: true }),
  }),
  // Two freshness times: the prices' and the summary's own.
  pricesAsOf: z.string().datetime({ offset: true }).nullable(),
  generatedAt: z.string().datetime({ offset: true }),
  ticker: z.string().nullable(),
  // The period reconciled in Rust; on a holding's page its attribution names only it.
  report: periodReportSchema.nullable(),
  notable: z.array(
    z.object({
      ticker: z.string(),
      changePct: z.number().nullable(),
      layers: holdingLayersSchema,
    }),
  ),
  periodMove: z
    .object({
      changePct: z.number(),
      market: benchmarkSchema.nullable(),
      sector: benchmarkSchema.nullable(),
      layers: holdingLayersSchema.nullable(),
      kind: moveKindSchema.nullable(),
    })
    .nullable(),
  // The portfolio's period in layers: each session's market, group, and own dollars summed.
  periodLayers: z
    .object({
      sessions: z.number().int().nonnegative(),
      missing: z.number().int().nonnegative(),
      market: z.number(),
      groups: z.array(
        z.object({
          fund: z.string(),
          name: z.string().nullable(),
          change: z.number(),
          members: z.array(z.string()),
          flagged: z.number().int().nonnegative(),
        }),
      ),
      own: z.array(
        z.object({
          ticker: z.string(),
          change: z.number(),
          ownPct: z.number(),
          z: z.number().nullable(),
          fit: fitSchema.nullable(),
        }),
      ),
      unmodeled: z.number(),
    })
    .nullable(),
  activity: z.object({
    buys: z.number().int().nonnegative(),
    sells: z.number().int().nonnegative(),
    trades: z.array(
      z.object({
        date: isoDate,
        side: z.enum(['buy', 'sell']),
        ticker: z.string(),
        units: z.number().nullable(),
        amount: z.number(),
        realizedGain: z.number().nullable(),
      }),
    ),
    realized: z.number().nullable(),
    dividends: z.number(),
    dividendPayments: z.number().int().nonnegative(),
  }),
  // The period's events from the event log, ranked in Rust by visible components.
  events: z.array(listedEventSchema),
  calendar: z.array(
    z.object({
      session: isoDate,
      kind: z.string(),
      title: z.string(),
      at: z.string().datetime({ offset: true }).nullable(),
      url: z.string().url().startsWith('https://').nullable(),
    }),
  ),
  ahead: z.array(
    z.object({
      date: isoDate,
      kind: z.string(),
      ticker: z.string().nullable(),
      title: z.string(),
      status: eventStatusSchema,
      amount: z.number().nullable(),
      url: z.string().url().startsWith('https://').nullable(),
    }),
  ),
  quiet: z
    .object({
      portfolio: z.boolean(),
      z: z.number().nullable(),
      holdings: z.array(z.string()),
    })
    .nullable(),
  fundamentals: z
    .object({
      frame: z.string(),
      quarterEnd: isoDate,
      revenue: z.number(),
      revenueGrowthPct: z.number().nullable(),
      netMarginPct: z.number().nullable(),
      form: z.string(),
      filed: z.string(),
      url: z.string().url().startsWith('https://www.sec.gov/'),
    })
    .nullable(),
  insiders: z.array(
    z.object({
      date: isoDate,
      filedAt: z.string().datetime({ offset: true }),
      name: z.string(),
      role: z.string(),
      side: z.enum(['buy', 'sell']),
      shares: z.number().positive(),
      price: z.number().nullable(),
      value: z.number().nullable(),
      planned: z.boolean(),
      url: z.string().url().startsWith('https://www.sec.gov/'),
    }),
  ),
  today: z
    .object({
      asOf: z.string().nullable(),
      changePct: z.number().nullable(),
      change: z.number().nullable(),
      market: benchmarkSchema.nullable(),
      sector: benchmarkSchema.nullable(),
      kind: moveKindSchema.nullable(),
      positions: z.array(
        z.object({
          ticker: z.string(),
          changePct: z.number(),
          change: z.number(),
          kind: moveKindSchema.nullable(),
        }),
      ),
      sectors: z.array(
        z.object({ name: z.string(), weightPct: z.number(), changePct: z.number() }),
      ),
    })
    .nullable(),
  coverage: z.array(
    z.object({
      ticker: z.string(),
      stories: z.number().int().nonnegative(),
      topRelevance: z.number().int().min(0).max(100).nullable(),
    }),
  ),
  // The latest session in a few lines, with the headlines that explain it.
  brief: briefSchema.nullable(),
  // Parts that couldn't load or were left out, each with the symbols affected (none: all).
  unavailable: z.array(
    z.object({
      part: z.enum(['prices', 'fundamentals', 'history']),
      symbols: z.array(z.string()),
    }),
  ),
})
export type InvestmentSummary = z.infer<typeof investmentSummarySchema>
export type PortfolioToday = z.infer<typeof portfolioTodaySchema>

const sectorSliceSchema = z.object({
  fund: z.string(),
  label: z.string(),
  value: z.number(),
  percent: z.number(),
})
export const sectorExposureSchema = z.object({
  total: z.number(),
  sectors: z.array(sectorSliceSchema),
  funds: z.number(),
  unclassified: z.number(),
  concentrated: sectorSliceSchema.nullable(),
  needsContact: z.boolean(),
})
export type SectorExposure = z.infer<typeof sectorExposureSchema>

export const earningsEventSchema = z.object({
  symbol: z.string(),
  name: z.string(),
  reportDate: z.string(),
  fiscalDateEnding: z.string().nullable(),
  estimate: z.number().nullable(),
  currency: z.string().nullable(),
})
export type EarningsEvent = z.infer<typeof earningsEventSchema>
