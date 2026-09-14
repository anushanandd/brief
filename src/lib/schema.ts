import { z } from 'zod'

const accountSchema = z.object({
  id: z.string(),
  name: z.string(),
  institution: z.string(),
  type: z.string(),
  value: z.number().nullable(),
  knownCostBasis: z.number().nullable().optional(),
  knownUnrealizedGain: z.number().nullable().optional(),
  knownUnrealizedGainPct: z.number().nullable().optional(),
  investmentIncomeYtd: z.number().nullable().optional(),
  saleProceedsYtd: z.number().nullable().optional(),
  salesYtd: z.number().int().nonnegative().optional(),
  estimatedRealizedGainYtd: z.number().nullable().optional(),
  realizedGainCoverage: z.enum(['complete', 'partial', 'unavailable']).optional(),
  costBasisCoverage: z.enum(['complete', 'partial', 'unavailable']).optional(),
  currency: z.string().nullable().optional(),
  balanceAsOf: z.string().nullable().optional(),
  balanceFetchedAt: z.string().nullable().optional(),
  positionsAsOf: z.string().nullable().optional(),
  activityAsOf: z.string().nullable().optional(),
})

const holdingSchema = z
  .object({
    ticker: z.string(),
    name: z.string(),
    accountId: z.string(),
    shares: z.number().nullable(),
    price: z.number().nullable(),
    value: z.number().nullable(),
    costBasis: z.number().nullable().optional(),
    unrealizedGain: z.number().nullable().optional(),
    dailyChangePct: z.number().nullable(),
    weeklyChangePct: z.number().nullable().default(null),
    weeklyReferencePrice: z.number().positive().nullable().default(null),
    weeklyReferenceDate: z.string().nullable().default(null),
    totalChangePct: z.number().nullable(),
    instrumentKind: z.string().optional(),
    currency: z.string().nullable().optional(),
    quoteEligible: z.boolean().optional(),
    valuationNote: z.string().nullable().optional(),
    marketAsOf: z.string().nullable().optional(),
    color: z.string(),
  })
  .transform((holding) => ({
    ...holding,
    costBasis:
      holding.costBasis !== undefined
        ? holding.costBasis
        : holding.value !== null && holding.totalChangePct !== null && holding.totalChangePct > -100
          ? holding.value / (1 + holding.totalChangePct / 100)
          : null,
  }))

const allocationSchema = z.object({
  name: z.string(),
  value: z.number(),
  percent: z.number(),
  color: z.string(),
})

const performanceSchema = z.object({
  accountId: z.string(),
  name: z.string(),
  institution: z.string(),
  currentValue: z.number(),
  historySource: z
    .enum(['reported', 'provider-estimated', 'estimated', 'transaction-derived', 'unavailable'])
    .optional(),
  historyStart: z.string().nullable().optional(),
  performanceMethod: z
    .enum(['value-only', 'value-with-comparisons', 'time-weighted', 'modified-dietz'])
    .optional(),
  points: z.array(
    z.object({
      date: z.string(),
      value: z.number(),
      netDeposits: z.number().nullable(),
      sp500: z.number().nullable(),
      marketChange: z.number().nullable().optional(),
      marketChangePct: z.number().nullable().optional(),
    }),
  ),
})

export const financeSnapshotSchema = z.object({
  calculationVersion: z.number().int().positive().optional(),
  revision: z.number().int().nonnegative().optional(),
  syncWarnings: z.array(z.string()).optional(),
  providerStatus: z
    .record(
      z.string(),
      z.object({ updatedAt: z.string().nullable(), error: z.string().nullable() }),
    )
    .optional(),
  observedNetWorthHistory: z.array(z.object({ date: z.string(), value: z.number() })).optional(),
  updatedAt: z.string(),
  netWorth: z.number(),
  netWorthIncomplete: z.boolean().optional(),
  recovery: z.object({ message: z.string(), canRestore: z.boolean() }).optional(),
  accounts: z.array(accountSchema),
  netWorthHistory: z.array(z.object({ date: z.string(), value: z.number() })),
  netWorthHistoryEstimated: z.boolean().default(false),
  benchmarkHistory: z.array(z.object({ date: z.string(), value: z.number() })).default([]),
  accountBalanceHistory: z.array(performanceSchema).default([]),
  brokeragePerformance: z.array(performanceSchema).default([]),
  holdings: z.array(holdingSchema),
  trades: z
    .array(
      z.object({
        id: z.string(),
        type: z.string(),
        date: z.string(),
        amount: z.number(),
        account: z.string(),
        accountId: z.string(),
        ticker: z.string().nullable().optional(),
        description: z.string().nullable().optional(),
        units: z.number().nullable().optional(),
        price: z.number().nullable().optional(),
        realizedCostBasis: z.number().nullable().optional(),
        estimatedRealizedGain: z.number().nullable().optional(),
        estimatedRealizedGainPct: z.number().nullable().optional(),
        realizedGainMethod: z.literal('estimated-fifo').optional(),
      }),
    )
    .default([]),
  spending: z.object({
    monthTotal: z.number(),
    categories: z.array(allocationSchema),
  }),
  transactions: z.array(
    z.object({
      id: z.string(),
      merchant: z.string(),
      description: z.string().nullable().optional(),
      category: z.string(),
      date: z.string(),
      occurredOn: z.string().nullable().optional(),
      postedOn: z.string().nullable().optional(),
      amount: z.number(),
      account: z.string(),
      accountId: z.string().optional(),
      pending: z.boolean(),
      benefitConfirmed: z.boolean().optional(),
      logoUrl: z.string().optional(),
      website: z.string().optional(),
      logoName: z.string().optional(),
    }),
  ),
  accountMovements: z
    .array(
      z.object({
        id: z.string(),
        observedAt: z.string(),
        accountId: z.string(),
        name: z.string(),
        change: z.number(),
      }),
    )
    .default([]),
  possibleDuplicateAccounts: z
    .array(
      z.object({
        plaidAccountId: z.string(),
        snaptradeAccountId: z.string(),
        description: z.string(),
      }),
    )
    .default([]),
  accountLinks: z.record(z.string(), z.string()).default({}),
  provenance: z
    .object({
      calculation: z.literal('rust'),
      benchmark: z.object({ provider: z.string(), adjustment: z.string() }),
      stockPlanHistory: z.object({ provider: z.string(), adjustment: z.string() }),
    })
    .optional(),
  lastChange: z
    .object({
      observedAt: z.string(),
      previousUpdatedAt: z.string(),
      previousNetWorth: z.number(),
      netWorthChange: z.number(),
      comparisonComplete: z.boolean().optional(),
      accountChanges: z.array(
        z.object({
          accountId: z.string(),
          name: z.string(),
          change: z.number(),
        }),
      ),
      newTransactionIds: z.array(z.string()),
    })
    .optional(),
})

export type FinanceSnapshot = z.infer<typeof financeSnapshotSchema>
export type Account = FinanceSnapshot['accounts'][number]
export type Transaction = FinanceSnapshot['transactions'][number]
export type Trade = FinanceSnapshot['trades'][number]
export type AccountMovement = FinanceSnapshot['accountMovements'][number]
export type SnapshotChange = NonNullable<FinanceSnapshot['lastChange']>

const marketSnapshotSchema = z.object({
  symbol: z.string(),
  price: z.number(),
  previousClose: z.number(),
  dailyChangePct: z.number(),
  weeklyChangePct: z.number().nullable(),
  weeklyReferencePrice: z.number().positive().nullable(),
  weeklyReferenceDate: z.string().nullable(),
  asOf: z.string(),
})

export const marketSnapshotsSchema = z.object({
  snapshots: z.record(z.string(), marketSnapshotSchema),
  session: z.enum(['Regular market', 'Pre-market', 'After hours', 'Overnight', 'Market closed']),
  feed: z.enum(['iex', 'delayed_sip', 'overnight']),
  delayMinutes: z.number().int().nonnegative(),
  asOf: z.string().nullable(),
  nextTransitionAt: z.string().nullable(),
  pollIntervalMs: z.number().int().positive().nullable(),
  financeSnapshot: financeSnapshotSchema.optional(),
})
export type MarketSnapshot = z.infer<typeof marketSnapshotSchema>
export type MarketSnapshots = z.infer<typeof marketSnapshotsSchema>

export const marketNewsSchema = z.array(
  z.object({
    headline: z.string(),
    summary: z.string(),
    source: z.string(),
    url: z.string().url().startsWith('https://'),
    createdAt: z.string(),
  }),
)
export type MarketNewsArticle = z.infer<typeof marketNewsSchema>[number]
