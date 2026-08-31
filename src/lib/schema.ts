import { z } from 'zod'

const accountSchema = z.object({
  id: z.string(),
  name: z.string(),
  institution: z.string(),
  type: z.string(),
  value: z.number(),
})

const holdingSchema = z.object({
  ticker: z.string(),
  name: z.string(),
  accountId: z.string(),
  shares: z.number(),
  price: z.number(),
  value: z.number(),
  dailyChangePct: z.number(),
  totalChangePct: z.number(),
  afterHoursPrice: z.number(),
  color: z.string(),
  summary: z.string(),
})

const allocationSchema = z.object({
  name: z.string(),
  value: z.number(),
  percent: z.number(),
  color: z.string(),
})

export const financeSnapshotSchema = z.object({
  updatedAt: z.string(),
  currency: z.string(),
  netWorth: z.number(),
  netWorthChange: z.number(),
  netWorthChangePct: z.number(),
  investedAssets: z.number(),
  cash: z.number(),
  debt: z.number(),
  totalReturn: z.number(),
  totalReturnPct: z.number(),
  accounts: z.array(accountSchema),
  netWorthHistory: z.array(z.object({ date: z.string(), value: z.number() })),
  holdings: z.array(holdingSchema),
  allocation: z.array(allocationSchema),
  dividends: z.array(z.object({ month: z.string(), value: z.number() })),
  spending: z.object({
    statementBalance: z.number(),
    statementDueDate: z.string(),
    monthTotal: z.number(),
    monthChangePct: z.number(),
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
    }),
  ),
  credits: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      used: z.number(),
      total: z.number(),
      deadline: z.string(),
      status: z.enum(['used', 'available', 'missed']),
    }),
  ),
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
export type Holding = FinanceSnapshot['holdings'][number]
export type Transaction = FinanceSnapshot['transactions'][number]
