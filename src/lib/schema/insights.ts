/** Everything Rust derives from the snapshot for the renderer to phrase: expectations, benefits, taxes, lots, credit, trips, and the rest. */
import { z } from 'zod'

import { isoDate } from './common'
import { exposureSchema, statementSchema } from './report'

const benefitActivitySchema = z.object({
  id: z.string(),
  merchant: z.string(),
  description: z.string().optional(),
  amount: z.number(),
  pending: z.boolean(),
  accountId: z.string().optional(),
  date: isoDate,
  benefitId: z.string(),
  benefitName: z.string(),
  kind: z.enum(['credit', 'reversal', 'purchase', 'unverified']),
  pairedId: z.string().optional(),
  creditWindow: isoDate.optional(),
  alternateCreditWindow: isoDate.optional(),
  credit: z
    .discriminatedUnion('state', [
      z.object({ state: z.literal('credited'), amount: z.number() }),
      z.object({ state: z.literal('expected'), amount: z.number(), by: isoDate }),
      z.object({ state: z.literal('late'), amount: z.number(), by: isoDate }),
      z.object({ state: z.literal('not-credited') }),
      z.object({ state: z.literal('over-allowance') }),
    ])
    .optional(),
})
const amountRangeSchema = z.object({ low: z.number(), high: z.number() })
const benefitWindowSchema = z.object({
  id: z.string(),
  name: z.string(),
  limit: z.string(),
  cap: z.number(),
  cadence: z.enum(['monthly', 'quarterly', 'semiannual', 'annual', 'renewal', 'purchase']),
  instruction: z.string(),
  url: z.string(),
  deadlineNote: z.string().optional(),
  periodUncertain: z.boolean(),
  snapshotBeforeWindow: z.boolean(),
  postingDelayDays: z.number().int(),
  creditLagDays: z.number().int(),
  creditLagLearned: z.boolean(),
  renewalCheck: z.string().optional(),
  status: z.enum(['external', 'credited', 'matched', 'unused']),
  engaged: z.boolean(),
  creditedAmount: z.number(),
  uncertainCreditAmount: z.number(),
  awaitingCreditAmount: z.number(),
  estimatedCreditAmount: z.number().nullable(),
  remainingAmount: z.number().nullable(),
  remainingRange: amountRangeSchema.optional(),
  unusableRemainder: z.number().optional(),
  activity: z.array(benefitActivitySchema),
  windowStart: isoDate,
  windowEnd: isoDate,
  windowLabel: z.string(),
  daysRemaining: z.number().int(),
  lastCredit: z.object({ amount: z.number(), date: z.string() }).optional(),
  reset: z.string(),
})
const expectedEventSchema = z.object({
  id: z.string(),
  recurringId: z.string(),
  title: z.string(),
  kind: z.enum(['credit', 'income', 'payment', 'subscription']),
  date: isoDate,
  amount: z.number(),
  amountLow: z.number(),
  amountHigh: z.number(),
  frequency: z.string(),
  observations: z.number().int().positive(),
  lastSeen: isoDate,
  state: z.enum(['expected', 'pending', 'awaiting-post']),
  accountId: z.string().optional(),
  matchedTransactionId: z.string().optional(),
  sourceTransactionId: z.string(),
})
const attentionItemSchema = z.intersection(
  z.object({ id: z.string(), severity: z.enum(['urgent', 'attention']) }),
  z.discriminatedUnion('kind', [
    z.object({
      kind: z.literal('payment-due'),
      accountId: z.string(),
      due: isoDate,
      days: z.number().int(),
      overdue: z.boolean(),
      statementBalance: z.number().nullable().optional(),
      minimumPayment: z.number().nullable().optional(),
    }),
    z.object({
      kind: z.literal('coverage-short'),
      cash: z.number(),
      expectedOutflows: z.number(),
      shortfall: z.number(),
    }),
    z.object({
      kind: z.literal('credit-expiring'),
      accountId: z.string(),
      benefitId: z.string(),
      name: z.string(),
      remainingLow: z.number(),
      remainingHigh: z.number(),
      windowEnd: isoDate,
      days: z.number().int(),
    }),
    z.object({
      kind: z.literal('cash-low'),
      date: isoDate,
      expected: z.number(),
      low: z.number(),
    }),
    z.object({
      kind: z.literal('utilization-high'),
      accountId: z.string(),
      utilization: z.number(),
      balance: z.number(),
      closesOn: isoDate,
      closesEstimated: z.boolean(),
      days: z.number().int(),
    }),
    z.object({
      kind: z.literal('possible-duplicate'),
      accountId: z.string(),
      merchant: z.string(),
      amount: z.number(),
      date: isoDate,
      transactionIds: z.tuple([z.string(), z.string()]),
    }),
    z.object({
      kind: z.literal('wash-window'),
      ticker: z.string(),
      accountId: z.string(),
      soldOn: isoDate,
      until: isoDate,
      loss: z.number(),
    }),
  ]),
)
const nextDividendSchema = z.object({
  exDate: isoDate,
  payableDate: isoDate.optional(),
  rate: z.number(),
  amount: z.number(),
  special: z.boolean(),
})
const taxYearSchema = z.object({
  year: z.number().int(),
  interest: z.number(),
  dividends: z.object({
    total: z.number(),
    qualified: z.number(),
    nonQualified: z.number(),
    unclassified: z.number(),
  }),
  realized: z.object({
    shortTerm: z.number(),
    longTerm: z.number(),
    washDisallowed: z.number(),
    sales: z.number().int().nonnegative(),
    unestimatedSales: z.number().int().nonnegative(),
    unestimatedProceeds: z.number(),
  }),
  washSales: z.array(
    z.object({
      saleId: z.string(),
      ticker: z.string(),
      accountId: z.string(),
      date: isoDate,
      loss: z.number(),
      disallowed: z.number(),
      replacementTradeIds: z.array(z.string()),
      permanent: z.boolean(),
    }),
  ),
  estimate: z
    .object({
      low: z.number(),
      high: z.number(),
      ordinary: z.number(),
      capitalGains: z.number(),
      niit: z.number(),
      state: z.number(),
      marginalRate: z.number(),
      capitalGainsRate: z.number(),
      lossDeduction: z.number(),
      lossCarryover: z.number(),
    })
    .optional(),
  tablesAvailable: z.boolean(),
  partial: z.boolean(),
})
const cashDaySchema = z.object({
  date: isoDate,
  expected: z.number(),
  low: z.number(),
  high: z.number(),
})
const allocationSliceSchema = z.object({
  key: z.string(),
  label: z.string(),
  value: z.number(),
  percent: z.number(),
})
const changeWindowSchema = z.enum(['week', 'month', 'quarter', 'year', 'all'])
const periodRecapSchema = z.object({
  window: changeWindowSchema,
  start: isoDate,
  end: isoDate,
  partial: z.boolean(),
  spending: z.number(),
  previousSpending: z.number().optional(),
  income: z.number(),
  net: z.number(),
  categories: z.array(z.object({ category: z.string(), spent: z.number(), previous: z.number() })),
})
const cardBenefitsSchema = z.object({
  accountId: z.string(),
  product: z.string(),
  productName: z.string(),
  fee: z
    .object({
      annualFee: z.number(),
      recovered: z.number(),
      /** Credits on purchases made for someone else: used, but not your saving. */
      forOthers: z.number(),
      since: isoDate,
      basis: z.enum(['fee-charge', 'calendar-year']),
    })
    .optional(),
  activity: z.array(benefitActivitySchema),
  windows: z.array(benefitWindowSchema),
  /** Windows with allowance left, by ID, in the order to use them. */
  useFirst: z.array(z.string()),
  outcomes: z.array(
    z.object({
      windowEnd: isoDate,
      benefitId: z.string(),
      state: z.enum(['settled', 'settling', 'partial-history', 'not-visible', 'not-tracked']),
      unused: z.number().nullable(),
      unusedHigh: z.number().nullable(),
      engaged: z.boolean(),
      /** What the unused allowance counts as; absent when it counts as nothing. */
      verdict: z.enum(['missed', 'unclaimed', 'settling', 'unknown']).optional(),
    }),
  ),
})

const spendingPeriodSchema = z.object({
  kind: z.enum(['statement', 'week', 'month', 'quarter', 'year']),
  start: isoDate,
  /** Usual spending by this point, by category, when history covers two earlier periods. */
  typical: z.record(z.string(), z.number()).optional(),
  /** The current period carried to its end at its pace so far. */
  projected: z
    .object({ total: z.number(), categories: z.record(z.string(), z.number()) })
    .optional(),
  /** The saved card's statement balance when the period opened. */
  opening: z.number().optional(),
})
export const insightsSchema = z.object({
  today: isoDate,
  computedAt: z.string(),
  expected: z.array(expectedEventSchema),
  statement: z
    .object({
      source: z.enum(['issuer', 'autopay']),
      boundaries: z.array(isoDate),
      autopays: z.array(z.object({ date: isoDate, amount: z.number() })),
      lastClose: isoDate.optional(),
      nextClose: isoDate.optional(),
    })
    .optional(),
  benefits: cardBenefitsSchema.optional(),
  otherCardBenefits: z.array(cardBenefitsSchema).optional(),
  benefitProducts: z.array(z.object({ id: z.string(), name: z.string() })),
  benefitOptions: z.array(z.object({ id: z.string(), name: z.string() })),
  benefitRulesVersion: z.string(),
  benefitTermsReviewed: z.string(),
  refundSuggestions: z.array(z.object({ refundId: z.string(), purchaseId: z.string() })),
  plan: z.object({
    monthStart: isoDate,
    daysElapsed: z.number().int(),
    daysInMonth: z.number().int(),
    spent: z.number(),
    categories: z.array(
      z.object({
        category: z.string(),
        spent: z.number(),
        projected: z.number(),
        typical: z.number(),
      }),
    ),
    coverage: z.object({
      cash: z.number(),
      cashIncomplete: z.boolean(),
      expectedOutflows: z.number(),
      expectedInflows: z.number(),
      afterOutflows: z.number(),
      horizonDays: z.number().int(),
    }),
    monthlySpending: z.number().optional(),
    runwayMonths: z.number().optional(),
  }),
  attention: z.array(attentionItemSchema),
  brief: z.object({
    needs: z.array(attentionItemSchema),
    coming: z.array(z.string()),
    comingDays: z.number().int().positive(),
  }),
  cash: z.object({
    horizonDays: z.number().int().positive(),
    cash: z.number(),
    pendingChange: z.number().default(0),
    cashIncomplete: z.boolean(),
    days: z.array(cashDaySchema),
    events: z.array(
      z.object({
        date: isoDate,
        recurringId: z.string(),
        title: z.string(),
        amount: z.number(),
        next: z.boolean(),
      }),
    ),
    lowest: cashDaySchema.optional(),
  }),
  subscriptions: z.object({
    monthly: z.number(),
    items: z.array(
      z.object({
        recurringId: z.string(),
        title: z.string(),
        amount: z.number(),
        monthlyAmount: z.number(),
        frequency: z.string(),
        nextDate: isoDate,
        pending: z.boolean(),
        accountId: z.string().optional(),
        sourceTransactionId: z.string(),
        priceChange: z
          .object({
            previous: z.number(),
            latest: z.number(),
            changedOn: isoDate,
            transactionId: z.string(),
          })
          .optional(),
      }),
    ),
  }),
  credit: z
    .object({
      cards: z.array(
        z.object({
          accountId: z.string(),
          balance: z.number(),
          limit: z.number().optional(),
          available: z.number().optional(),
          utilization: z.number().optional(),
          statementUtilization: z.number().optional(),
          closesOn: isoDate.optional(),
          closesEstimated: z.boolean(),
          interestCharged: z.number().optional(),
        }),
      ),
      balance: z.number(),
      limit: z.number(),
      utilization: z.number().optional(),
      cardsWithoutLimit: z.number().int().nonnegative(),
      interestCharged: z.number().optional(),
    })
    .default({ cards: [], balance: 0, limit: 0, cardsWithoutLimit: 0 }),
  anomalies: z
    .object({
      duplicates: z.array(
        z.object({
          id: z.string(),
          accountId: z.string(),
          merchant: z.string(),
          amount: z.number(),
          date: isoDate,
          transactionIds: z.tuple([z.string(), z.string()]),
        }),
      ),
      unusual: z.array(
        z.object({
          transactionId: z.string(),
          merchant: z.string(),
          amount: z.number(),
          usual: z.number(),
          observations: z.number().int().positive(),
        }),
      ),
    })
    .default({ duplicates: [], unusual: [] }),
  trips: z
    .object({
      trips: z.array(
        z.object({
          id: z.string(),
          start: isoDate,
          end: isoDate,
          destination: z.string(),
          abroad: z.boolean(),
          total: z.number(),
          categories: z.array(z.object({ name: z.string(), amount: z.number() })),
          transactionIds: z.array(z.string()),
          ongoing: z.boolean(),
        }),
      ),
      home: z.array(z.string()),
    })
    .default({ trips: [], home: [] }),
  /** Spending's scopes (an account ID, or `all`) and Rust's judgments about their periods. */
  spending: z
    .array(z.object({ scope: z.string(), periods: z.array(spendingPeriodSchema) }))
    .default([]),
  recurringCheck: z
    .object({
      available: z.boolean(),
      ended: z.array(z.string()),
      unforecast: z.array(
        z.object({
          streamId: z.string(),
          accountId: z.string(),
          name: z.string(),
          frequency: z.string(),
          predictedNextDate: z.string().optional(),
          averageAmount: z.number().optional(),
        }),
      ),
    })
    .default({ available: false, ended: [], unforecast: [] }),
  taxes: z
    .object({
      years: z.array(taxYearSchema),
      harvest: z.array(
        z.object({
          ticker: z.string(),
          accountId: z.string(),
          shortTermLoss: z.number(),
          longTermLoss: z.number(),
          taxSavings: z.number().optional(),
          washUntil: isoDate.optional(),
        }),
      ),
      washWindows: z.array(
        z.object({
          saleId: z.string(),
          ticker: z.string(),
          accountId: z.string(),
          soldOn: isoDate,
          until: isoDate,
          loss: z.number(),
        }),
      ),
      hasProfile: z.boolean(),
      tablesVersion: z.string(),
      sources: z.array(z.string()),
    })
    .default({
      years: [],
      harvest: [],
      washWindows: [],
      hasProfile: false,
      tablesVersion: '',
      sources: [],
    }),
  dividends: z
    .object({
      securities: z.array(
        z.object({
          ticker: z.string(),
          shares: z.number(),
          trailingRate: z.number(),
          payments: z.number().int().nonnegative(),
          yieldPercent: z.number().optional(),
          annualIncome: z.number(),
          next: nextDividendSchema.optional(),
        }),
      ),
      annualIncome: z.number(),
      upcoming: z.array(nextDividendSchema.extend({ ticker: z.string() })),
    })
    .default({ securities: [], annualIncome: 0, upcoming: [] }),
  accountsTotal: z
    .object({
      excludedAccountId: z.string(),
      excludedValue: z.number(),
      value: z.number(),
      history: z.array(z.object({ date: z.string(), value: z.number() })),
    })
    .optional(),
  returns: z.array(
    z.object({
      accountId: z.string(),
      windows: z.array(
        z.object({
          window: z.enum(['week', 'month', 'quarter', 'year-to-date', 'year', 'all']),
          start: isoDate,
          end: isoDate,
          timeWeightedPct: z.number(),
          moneyWeightedPct: z.number().optional(),
          benchmarkPct: z.number().optional(),
          netDeposits: z.number(),
          marketGain: z.number(),
          vsBenchmark: z.enum(['ahead', 'behind', 'inline']).optional(),
        }),
      ),
    }),
  ),
  // The combined brokerages' statement for each chart window, reconciled in Rust.
  periods: z.array(statementSchema),
  exposure: exposureSchema.optional(),
  allocation: z
    .object({
      total: z.number(),
      incomplete: z.boolean(),
      cashUnknown: z.boolean(),
      kinds: z.array(allocationSliceSchema),
      securities: z.array(allocationSliceSchema),
      accounts: z.array(allocationSliceSchema),
      largestPercent: z.number().optional(),
      topFivePercent: z.number().optional(),
      concentrated: allocationSliceSchema.optional(),
    })
    .optional(),
  securities: z.array(
    z.object({
      ticker: z.string(),
      name: z.string(),
      accounts: z.number().int().positive(),
      value: z.number().optional(),
      percent: z.number().optional(),
      gain: z.number().optional(),
      gainPercent: z.number().optional(),
    }),
  ),
  recap: z.array(periodRecapSchema),
  changes: z.array(
    z.object({
      window: changeWindowSchema,
      start: isoDate,
      end: isoDate,
      accounts: z.array(z.object({ accountId: z.string(), change: z.number() })),
      incomplete: z.boolean(),
      transactions: z.number().int().nonnegative(),
    }),
  ),
  interest: z.array(
    z.object({
      accountId: z.string(),
      monthlyNetFlow: z.number().nullable(),
      periods: z.array(
        z.object({
          window: changeWindowSchema,
          earned: z.number(),
          annualRate: z.number().nullable(),
        }),
      ),
    }),
  ),
  lots: z.array(
    z.object({
      accountId: z.string(),
      ticker: z.string(),
      coverage: z.enum(['complete', 'partial']),
      untrackedUnits: z.number(),
      lots: z.array(
        z.object({
          acquired: isoDate,
          units: z.number(),
          cost: z.number(),
          value: z.number().optional(),
          gain: z.number().optional(),
          longTerm: z.boolean(),
          longTermOn: isoDate,
          estimatedTax: z.number().optional(),
        }),
      ),
      shortTermGain: z.number().optional(),
      longTermGain: z.number().optional(),
      moneyWeightedPct: z.number().optional(),
      since: isoDate,
      taxDeferred: z.boolean(),
    }),
  ),
  longTermSoon: z.array(
    z.object({
      ticker: z.string(),
      accountId: z.string(),
      date: isoDate,
      units: z.number(),
      gain: z.number().optional(),
    }),
  ),
  investmentAttention: z.object({
    flags: z.array(
      z.discriminatedUnion('kind', [
        z.object({ kind: z.literal('concentrated'), ticker: z.string(), percent: z.number() }),
        z.object({
          kind: z.literal('longTermSoon'),
          ticker: z.string(),
          accountId: z.string(),
          date: isoDate,
          units: z.number(),
          gain: z.number(),
        }),
        z.object({
          kind: z.literal('harvestableLoss'),
          ticker: z.string(),
          accountId: z.string(),
          loss: z.number(),
          percent: z.number(),
        }),
      ]),
    ),
    earningsWatch: z.array(z.string()),
  }),
  movers: z
    .object({
      asOf: z.string().optional(),
      change: z.number(),
      incomplete: z.boolean(),
      positions: z.array(
        z.object({
          ticker: z.string(),
          name: z.string(),
          changePct: z.number(),
          change: z.number(),
        }),
      ),
    })
    .optional(),
})
export type Insights = z.infer<typeof insightsSchema>
export type ExpectedEvent = Insights['expected'][number]
export type BenefitWindow = NonNullable<Insights['benefits']>['windows'][number]
export type BenefitActivity = BenefitWindow['activity'][number]
export type BenefitOutcome = NonNullable<Insights['benefits']>['outcomes'][number]
export type AttentionItem = Insights['attention'][number]
export type CardBenefits = z.infer<typeof cardBenefitsSchema>
export type SpendingPeriodJudgment = z.infer<typeof spendingPeriodSchema>
export type Allocation = NonNullable<Insights['allocation']>
export type ChangeWindow = z.infer<typeof changeWindowSchema>
export type CreditInsight = Insights['credit']
export type Trip = Insights['trips']['trips'][number]
export type Taxes = Insights['taxes']
export type TaxYear = z.infer<typeof taxYearSchema>
export type Subscription = Insights['subscriptions']['items'][number]
