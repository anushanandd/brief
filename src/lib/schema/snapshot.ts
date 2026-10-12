/** The committed finance snapshot: accounts, holdings, transactions, trades, histories, and settings, as Rust projects them. */
import { z } from 'zod'

import { insightsSchema } from './insights'

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
  balanceMode: z.enum(['live', 'cached']).optional(),
  liability: z
    .object({
      lastStatementDate: z.iso.date().nullable(),
      lastStatementBalance: z.number().nullable(),
      minimumPayment: z.number().nullable(),
      nextPaymentDueDate: z.iso.date().nullable(),
      lastPaymentDate: z.iso.date().nullable(),
      lastPaymentAmount: z.number().nullable(),
      purchaseApr: z.number().nullable(),
      isOverdue: z.boolean().nullable(),
      aprs: z
        .array(
          z.object({
            type: z.string(),
            percentage: z.number().nullable(),
            balanceSubjectToApr: z.number().nullable(),
            interestCharged: z.number().nullable(),
          }),
        )
        .optional(),
    })
    .optional(),
  availableBalance: z.number().optional(),
  creditLimit: z.number().optional(),
  officialName: z.string().optional(),
  subtype: z.string().optional(),
  holderCategory: z.string().optional(),
  taxTreatment: z.enum(['taxable', 'taxDeferred', 'taxFree']).optional(),
  /** The institution's registered domain, from its provider-reported website. */
  domain: z
    .string()
    .regex(/^[a-z0-9-]+\.[a-z0-9-]+$/)
    .optional()
    .catch(undefined),
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
    instrumentSubtype: z.string().optional(),
    sector: z.string().optional(),
    industry: z.string().optional(),
    vestedShares: z.number().optional(),
    vestedValue: z.number().optional(),
    identifiers: z
      .object({
        isin: z.string().optional(),
        cusip: z.string().optional(),
        figi: z.string().optional(),
        exchange: z.string().optional(),
      })
      .optional(),
    option: z
      .object({
        contractType: z.string().nullable(),
        expirationDate: z.iso.date().nullable(),
        strikePrice: z.number().nullable(),
        underlyingTicker: z.string().nullable(),
      })
      .optional(),
    fixedIncome: z
      .object({
        yieldPercent: z.number().nullable(),
        yieldType: z.string().nullable(),
        maturityDate: z.iso.date().nullable(),
        issueDate: z.iso.date().nullable(),
        faceValue: z.number().nullable(),
      })
      .optional(),
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

export const performanceSchema = z.object({
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
    'owed',
    'other',
  ]),
  spending: z.boolean(),
  credit: z.boolean(),
  zelle: z.boolean(),
})

export const financeSettingsSchema = z.object({
  spendingAccountId: z.string().optional(),
  accountNames: z.record(z.string(), z.string()).default({}),
  accountStartDates: z.record(z.string(), z.iso.date()).default({}),
  hiddenBenefitIds: z.array(z.string()).default([]),
  plaidLiabilities: z.boolean().default(false),
  backgroundRefresh: z.boolean().default(false),
  notifications: z.boolean().default(false),
  benefitCards: z.record(z.string(), z.string()).optional(),
  accountTaxTreatments: z
    .record(z.string(), z.enum(['taxable', 'taxDeferred', 'taxFree']))
    .optional(),
  taxProfile: z
    .object({
      filingStatus: z.enum(['single', 'marriedJoint', 'marriedSeparate', 'headOfHousehold']),
      taxableIncome: z.number().nonnegative(),
      stateRate: z.number().min(0).max(20).optional(),
    })
    .optional(),
  plaidInvestmentTransactions: z.boolean().default(false),
  plaidRecurring: z.boolean().default(false),
})
export type FinanceSettings = z.infer<typeof financeSettingsSchema>

export const categoryRuleSchema = z.object({
  id: z.string(),
  pattern: z.string(),
  match: z.enum(['exact', 'contains']),
  accountId: z.string().optional(),
  category: z.string(),
  createdAt: z.string(),
})
export type CategoryRule = z.infer<typeof categoryRuleSchema>

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
  calendarDate: z.string().optional(),
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
        estimatedShortTermGain: z.number().optional(),
        estimatedLongTermGain: z.number().optional(),
        splitRatio: z.number().optional(),
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
      subcategory: z.string().optional(),
      pendingTransactionId: z.string().optional(),
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
          lat: z.number().optional(),
          lon: z.number().optional(),
          storeNumber: z.string().optional(),
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
      categorySource: z.enum(['provider', 'rule', 'manual']).optional(),
      providerCategory: z.string().optional(),
      ruleId: z.string().optional(),
      reviewed: z.boolean().optional(),
      recurringId: z.string().optional(),
      tags: z.array(z.string()).optional(),
      splits: z.array(z.object({ category: z.string(), amount: z.number() })).optional(),
      refundOf: z.string().optional(),
      /** Someone else's money: a purchase made for them, or what they paid you back. */
      other: z.boolean().optional(),
      merchantEntityId: z.string().optional(),
      dividendType: z.enum(['qualified', 'nonQualified']).optional(),
      originalDescription: z.string().optional(),
      checkNumber: z.string().optional(),
      accountOwner: z.string().optional(),
      counterparties: z
        .array(
          z.object({
            name: z.string(),
            type: z.string().optional(),
            confidence: z.string().optional(),
            website: z.string().optional(),
          }),
        )
        .optional(),
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
  historyCheck: z
    .object({
      comparedDays: z.number().int().nonnegative(),
      divergentDays: z.number().int().nonnegative(),
      largestGap: z.number().nullable(),
      largestGapDate: z.string().nullable(),
    })
    .optional(),
  dividendHistory: z
    .array(
      z.object({
        symbol: z.string(),
        exDate: z.string(),
        payableDate: z.string().nullable(),
        rate: z.number().nullable(),
        special: z.boolean(),
      }),
    )
    .optional(),
  recurringStreams: z
    .array(
      z.object({
        streamId: z.string(),
        accountId: z.string(),
        direction: z.string(),
        description: z.string(),
        merchantName: z.string().nullable(),
        frequency: z.string(),
        status: z.string(),
        isActive: z.boolean(),
        lastDate: z.string().nullable(),
        predictedNextDate: z.string().nullable(),
        averageAmount: z.number().nullable(),
        transactionIds: z.array(z.string()),
      }),
    )
    .optional(),
  accountLinks: z.record(z.string(), z.string()).default({}),
  settings: financeSettingsSchema.optional(),
  rules: z.array(categoryRuleSchema).optional(),
  insights: insightsSchema.optional(),
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
