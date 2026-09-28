import { z } from 'zod'

const accountSchema = z.object({
  id: z.string(),
  name: z.string(),
  institution: z.string(),
  type: z.string(),
  value: z.number().nullable(),
  cashValue: z.number().nullable().optional(),
  investedValue: z.number().nullable().optional(),
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
  balanceSource: z.enum(['reported', 'cash-and-positions', 'unavailable']).nullable().optional(),
  reportedBalance: z.number().nullable().optional(),
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

export const transactionClassificationSchema = z.object({
  mark: z.enum([
    'transfer',
    'interest',
    'dividend',
    'income',
    'refund',
    'fee',
    'payment',
    'cash',
    'initial',
  ]),
  kind: z.enum([
    'income',
    'dividend',
    'interest',
    'fee',
    'tax',
    'reimbursement',
    'transfer',
    'expense',
    'other',
  ]),
  spending: z.boolean(),
  credit: z.boolean(),
  zelle: z.boolean(),
  brokerageIncomeTransfer: z.boolean(),
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
  netWorthProvisional: z.boolean().optional(),
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
      classification: transactionClassificationSchema,
      description: z.string().nullable().optional(),
      category: z.string(),
      date: z.string(),
      occurredOn: z.string().nullable().optional(),
      postedOn: z.string().nullable().optional(),
      location: z
        .object({
          address: z.string().optional(),
          city: z.string().optional(),
          region: z.string().optional(),
          postalCode: z.string().optional(),
          country: z.string().optional(),
        })
        .optional(),
      paymentChannel: z.string().optional(),
      amount: z.number(),
      account: z.string(),
      accountId: z.string().optional(),
      pending: z.boolean(),
      benefitConfirmed: z.boolean().optional(),
      logoUrl: z.string().optional(),
      website: z.string().optional(),
      logoName: z.string().optional(),
      categoryDetail: z.string().optional(),
      categoryConfidence: z.string().optional(),
      counterpartyType: z.string().optional(),
      transactionCode: z.string().optional(),
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

export const healthReportSchema = z.object({
  computedAt: z.string(),
  revision: z.number().int().nonnegative(),
  overallStatus: z.enum(['healthy', 'info', 'warning', 'error', 'critical']),
  counts: z.object({
    critical: z.number().int().nonnegative(),
    error: z.number().int().nonnegative(),
    warning: z.number().int().nonnegative(),
    info: z.number().int().nonnegative(),
  }),
  issues: z.array(
    z.object({
      id: z.string(),
      severity: z.enum(['info', 'warning', 'error', 'critical']),
      category: z.string(),
      title: z.string(),
      explanation: z.string(),
      evidence: z.array(z.string()),
      affectedItems: z.array(z.object({ id: z.string(), name: z.string() })),
      action: z.object({ label: z.string(), route: z.string() }),
    }),
  ),
})
export type HealthReport = z.infer<typeof healthReportSchema>

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
})
export type MarketSnapshot = z.infer<typeof marketSnapshotSchema>
export type MarketSnapshots = z.infer<typeof marketSnapshotsSchema>

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

export const marketNewsSchema = z.array(
  z.object({
    headline: z.string(),
    summary: z.string(),
    source: z.string(),
    url: z.string().url().startsWith('https://'),
    createdAt: z.string().datetime({ offset: true }),
    symbols: z.array(z.string()),
    relevanceScore: z.number().min(0).max(1).nullish(),
    sentimentScore: z.number().min(-1).max(1).nullish(),
    sentimentLabel: z
      .enum(['Bearish', 'Somewhat-Bearish', 'Neutral', 'Somewhat-Bullish', 'Bullish'])
      .nullish(),
  }),
)
export type MarketNewsArticle = z.infer<typeof marketNewsSchema>[number]
export const marketNewsResultSchema = z.object({
  articles: marketNewsSchema,
  savedAt: z.string().datetime({ offset: true }).nullable(),
  warning: z.string().nullable(),
  requestsRemaining: z.number().int().min(0).max(20),
  canRefresh: z.boolean(),
})
export type MarketNewsResult = z.infer<typeof marketNewsResultSchema>

export const earningsEventSchema = z.object({
  symbol: z.string(),
  name: z.string(),
  reportDate: z.string(),
  fiscalDateEnding: z.string().nullable(),
  estimate: z.number().nullable(),
  currency: z.string().nullable(),
})
export type EarningsEvent = z.infer<typeof earningsEventSchema>

const plaidRecurringAmountSchema = z.object({
  amount: z.number().finite(),
  currency: z.string().nullable(),
})
export const plaidRecurringReportSchema = z.object({
  fetchedAt: z.string().datetime({ offset: true }),
  connections: z.array(
    z.object({
      itemId: z.string(),
      name: z.string(),
      error: z.string().nullable(),
      streams: z.array(
        z.object({
          streamId: z.string(),
          accountId: z.string(),
          direction: z.enum(['inflow', 'outflow']),
          description: z.string(),
          merchantName: z.string().nullable(),
          category: z.string().nullable(),
          frequency: z.string(),
          status: z.string(),
          isActive: z.boolean(),
          firstDate: z.string(),
          lastDate: z.string(),
          predictedNextDate: z.string().nullable(),
          averageAmount: plaidRecurringAmountSchema,
          lastAmount: plaidRecurringAmountSchema,
          transactionCount: z.number().int().nonnegative(),
        }),
      ),
    }),
  ),
})
export type PlaidRecurringReport = z.infer<typeof plaidRecurringReportSchema>
