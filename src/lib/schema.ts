import { z } from 'zod'

const accountSchema = z.object({
  id: z.string(),
  name: z.string(),
  institution: z.string(),
  type: z.string(),
  value: z.number(),
})

const holdingSchema = z
  .object({
    ticker: z.string(),
    name: z.string(),
    accountId: z.string(),
    shares: z.number(),
    price: z.number(),
    value: z.number(),
    costBasis: z.number().optional(),
    dailyChangePct: z.number(),
    totalChangePct: z.number(),
    color: z.string(),
  })
  .transform((holding) => ({
    ...holding,
    costBasis:
      holding.costBasis ??
      (holding.totalChangePct > -100
        ? holding.value / (1 + holding.totalChangePct / 100)
        : holding.value),
  }))

const allocationSchema = z.object({
  name: z.string(),
  value: z.number(),
  percent: z.number(),
  color: z.string(),
})

const investmentActivitySchema = z.object({
  accountId: z.string(),
  accountName: z.string(),
  date: z.string(),
  type: z.string(),
  amount: z.number(),
  description: z.string(),
  symbol: z.string().optional(),
})

export const financeSnapshotSchema = z.object({
  updatedAt: z.string(),
  netWorth: z.number(),
  accounts: z.array(accountSchema),
  netWorthHistory: z.array(z.object({ date: z.string(), value: z.number() })),
  netWorthHistoryEstimated: z.boolean().default(false),
  benchmarkHistory: z.array(z.object({ date: z.string(), value: z.number() })).default([]),
  brokeragePerformance: z
    .array(
      z.object({
        accountId: z.string(),
        name: z.string(),
        institution: z.string(),
        currentValue: z.number(),
        points: z.array(
          z.object({
            date: z.string(),
            value: z.number(),
            netDeposits: z.number(),
            sp500: z.number().nullable(),
          }),
        ),
      }),
    )
    .default([]),
  holdings: z.array(holdingSchema),
  spending: z.object({
    monthTotal: z.number(),
    categories: z.array(allocationSchema),
  }),
  transactions: z.array(
    z.object({
      id: z.string(),
      merchant: z.string(),
      category: z.string(),
      date: z.string(),
      amount: z.number(),
      account: z.string(),
      pending: z.boolean(),
      logoUrl: z.string().optional(),
      website: z.string().optional(),
      logoName: z.string().optional(),
    }),
  ),
  investmentActivities: z.array(investmentActivitySchema).default([]),
  providers: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      description: z.string(),
      status: z.enum(['ready', 'syncing', 'error', 'local']),
      lastSync: z.string(),
    }),
  ),
})

export type FinanceSnapshot = z.infer<typeof financeSnapshotSchema>
export type Account = FinanceSnapshot['accounts'][number]
export type Transaction = FinanceSnapshot['transactions'][number]
export type InvestmentActivity = FinanceSnapshot['investmentActivities'][number]

const marketSnapshotSchema = z.object({
  symbol: z.string(),
  price: z.number(),
  previousClose: z.number(),
  dailyChangePct: z.number(),
  asOf: z.string(),
})

export const marketSnapshotsSchema = z.record(z.string(), marketSnapshotSchema)
export type MarketSnapshot = z.infer<typeof marketSnapshotSchema>
