/** Everything Rust derives from the snapshot for the renderer to phrase: expectations, benefits, taxes, lots, credit, trips, and the rest. */
import { z } from 'zod'

import { isoDate } from './common'
import { exposureSchema, statementSchema } from './report'

/** Whose spending Spending counts: yours, purchases made for someone else, or both. */
export const spendingOwners = ['me', 'other', 'all'] as const
export type SpendingOwner = (typeof spendingOwners)[number]

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
  /** A subscription spends it on its own; the only thing to check is that it posted. */
  recurring: z.boolean(),
  /** The saved card history starts after the window began, on this day. */
  historyFrom: isoDate.optional(),
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
const benefitPeriodSchema = z.object({
  start: isoDate,
  end: isoDate,
  label: z.string(),
  cap: z.number(),
  credited: z.number(),
  state: z.enum([
    'full',
    'partial',
    'missed',
    'open',
    'settling',
    'future',
    'not-visible',
    'likely-used',
    'before-history',
    'not-tracked',
  ]),
  /** A settled period's usable allowance left unused, a range when credits can't be placed. */
  unused: amountRangeSchema.optional(),
  /** What an open period still has. */
  left: amountRangeSchema.optional(),
  activityIds: z.array(z.string()),
  /** Credits still to come: waiting purchases, then charges the forecast expects. */
  planned: z
    .array(
      z.object({
        title: z.string(),
        date: isoDate,
        amount: z.number(),
        source: z.enum(['purchase', 'recurring']),
        transactionId: z.string().optional(),
      }),
    )
    .optional(),
  /** The planned credits summed. */
  plannedTotal: z.number(),
})
/** A year of benefit periods summed; a benefit estimated from charges adds nothing. */
const benefitYearSchema = z.object({
  year: z.number().int(),
  captured: z.number(),
  planned: z.number(),
  missed: amountRangeSchema,
  left: amountRangeSchema,
})
const benefitHistorySchema = z.object({
  id: z.string(),
  name: z.string(),
  recurring: z.boolean(),
  /** Its use doesn't show on the card, so periods are estimated from charges. */
  external: z.boolean(),
  retired: z.boolean(),
  periods: z.array(benefitPeriodSchema),
  /** Each year with periods, oldest first. */
  years: z.array(benefitYearSchema),
})
const benefitReviewSchema = z.object({
  kind: z.enum(['unverified-credit', 'unassigned-credit', 'no-purchase', 'not-eligible']),
  transactionId: z.string(),
  benefitId: z.string().optional(),
  periodStart: isoDate.optional(),
  /** For a credit without a purchase: the charge that most likely earned it. */
  suggestionId: z.string().optional(),
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
  state: z.enum(['expected', 'pending', 'awaiting-post', 'missed']),
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
      kind: z.literal('benefit-review'),
      accountId: z.string(),
      count: z.number().int(),
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
    z.object({
      kind: z.literal('missed-bill'),
      recurringId: z.string(),
      title: z.string(),
      accountId: z.string().optional(),
      expected: isoDate,
      days: z.number().int(),
      amount: z.number(),
      event: z.enum(['credit', 'income', 'payment', 'subscription']),
      sourceTransactionId: z.string(),
    }),
    z.object({
      kind: z.literal('contributions-to-confirm'),
      accountId: z.string(),
      count: z.number().int(),
      amount: z.number(),
    }),
    z.object({
      kind: z.literal('contribution-limit'),
      plan: z.enum(['workplace', 'ira']),
      taxYear: z.number().int(),
      limit: z.number(),
      amount: z.number(),
      onPace: z.boolean(),
    }),
    z.object({
      kind: z.literal('pay-missing'),
      documentId: z.string(),
      employer: z.string(),
      date: isoDate,
      amount: z.number(),
      accountId: z.string().optional(),
    }),
    z.object({
      kind: z.literal('w2-differs'),
      documentId: z.string(),
      employer: z.string(),
      taxYear: z.number().int(),
      boxes: z.number().int(),
    }),
    z.object({
      kind: z.literal('idle-cash'),
      accountId: z.string(),
      balance: z.number(),
      annualRate: z.number().optional(),
      betterAccountId: z.string(),
      betterRate: z.number(),
    }),
  ]),
)
/** Contributions against this year's limits, tax buckets, savings rate, and coverage. */
const retirementSchema = z
  .object({
    year: z.number().int(),
    workplace: z
      .object({
        accountIds: z.array(z.string()),
        limit: z.number().optional(),
        deferred: z.number(),
        /** The pay statements' year-to-date figure, ahead of the plan's export. */
        fromPayStatements: z.boolean().optional(),
        unlabeled: z.number(),
        employer: z.number(),
        through: isoDate.optional(),
        paychecksLeft: z.number().int().optional(),
        projected: z.number().optional(),
        perPaycheckToMax: z.number().optional(),
      })
      .optional(),
    ira: z.array(
      z.object({
        taxYear: z.number().int(),
        limit: z.number().optional(),
        contributed: z.number(),
        deadline: isoDate,
      }),
    ),
    deposits: z.array(
      z.object({
        transactionId: z.string(),
        accountId: z.string(),
        date: isoDate,
        amount: z.number(),
        flow: z.enum(['contribution', 'deposit', 'transfer']),
        years: z.array(z.number().int()),
        countsFor: z.number().int().optional(),
        chosen: z.boolean(),
        /** The brokerage's own statement names the year. */
        stated: z.boolean().default(false),
        waiting: z.boolean(),
      }),
    ),
    buckets: z.object({
      preTax: z.number(),
      taxFree: z.number(),
      taxable: z.number(),
      complete: z.boolean(),
    }),
    rothBasis: z.array(
      z.object({
        accountId: z.string(),
        since: isoDate.optional(),
        earlier: z.number().optional(),
        contributed: z.number(),
        withdrawn: z.number(),
        basis: z.number(),
      }),
    ),
    savings: z.array(
      z.object({
        year: z.number().int(),
        from: isoDate,
        through: isoDate,
        partial: z.boolean(),
        takeHome: z.number(),
        spending: z.number(),
        deferred: z.number(),
        /** Employee stock purchases taken from pay. */
        espp: z.number().default(0),
        rate: z.number().optional(),
      }),
    ),
    coverage: z
      .object({
        invested: z.number(),
        complete: z.boolean(),
        spending: z.number(),
        planned: z.number(),
        years: z.number(),
      })
      .optional(),
    uncounted: z.array(z.string()),
  })
  .default({
    year: 0,
    ira: [],
    deposits: [],
    buckets: { preTax: 0, taxFree: 0, taxable: 0, complete: false },
    rothBasis: [],
    savings: [],
    uncounted: [],
  })
export type Retirement = z.infer<typeof retirementSchema>

/** Pay statements matched to deposits, expected forms, W-2 predictions, and keep dates. */
const documentInsightsSchema = z
  .object({
    years: z.array(
      z.object({
        year: z.number().int(),
        predictions: z.array(
          z.object({
            employer: z.string(),
            fromDocument: z.string(),
            fromDate: isoDate,
            boxes: z.array(
              z.object({
                key: z.string(),
                label: z.string(),
                predicted: z.number(),
                actual: z.number().optional(),
              }),
            ),
            w2: z.string().optional(),
          }),
        ),
        income: z
          .object({
            wages: z.number(),
            wagesFrom: z.enum(['w2', 'payStatements']),
            selfEmployment: z.number(),
            selfEmploymentTax: z.number(),
            taxableIncome: z.number().optional(),
          })
          .optional(),
      }),
    ),
    pay: z.array(
      z.object({
        documentId: z.string(),
        date: isoDate,
        amount: z.number(),
        mask: z.string().optional(),
        transactionId: z.string().optional(),
        accountId: z.string().optional(),
      }),
    ),
    keep: z.array(
      z.object({ documentId: z.string(), until: isoDate.optional(), reason: z.string() }),
    ),
    toReview: z.number().int(),
    /** Documents a newer reader would read better. */
    outdated: z.array(z.string()).default([]),
    /** Transactions a document confirms: a pay statement's deposit, or a statement's row. */
    evidence: z
      .array(
        z.object({
          transactionId: z.string(),
          documentId: z.string(),
          kind: z.enum(['pay', 'statement']),
        }),
      )
      .default([]),
  })
  .default({ years: [], pay: [], keep: [], toReview: 0, outdated: [], evidence: [] })
export type DocumentInsights = z.infer<typeof documentInsightsSchema>

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
const changeWindowSchema = z.enum(['day', 'week', 'month', 'quarter', 'year', 'all'])
const typeChangeSchema = z.object({ type: z.string(), change: z.number().nullable() })
const activityTotalsSchema = z.object({
  /** `YYYY-MM-DD` for a day, `YYYY-MM` for a month. */
  period: z.string(),
  spent: z.number(),
  income: z.number(),
  net: z.number(),
})
/** Each value chart's change over each window; see `insights::chart_change`. */
export const chartChangesSchema = z.array(
  z.object({
    series: z.string(),
    windows: z.partialRecord(
      changeWindowSchema,
      z.object({ change: z.number(), percent: z.number() }),
    ),
  }),
)
export type ChartChanges = z.infer<typeof chartChangesSchema>
const groupValueSchema = z.object({ type: z.string(), value: z.number().nullable() })
/** Account totals as pages read them, from saved or live values; see `insights::rollup`. */
export const rollupSchema = z.object({
  groups: z.array(groupValueSchema),
  listed: z.array(groupValueSchema),
  cards: groupValueSchema.optional(),
  shares: z.array(z.object({ type: z.string(), value: z.number(), percent: z.number() })),
  sheet: z.object({ assets: z.number(), liabilities: z.number(), complete: z.boolean() }),
  unrealized: z.number().nullable(),
  realizedYtd: z.number().nullable(),
  retirement: z.number().nullable(),
  netWorth: z.number(),
  combined: z.number().nullable(),
  holdings: z.array(
    z.object({
      ticker: z.string(),
      shares: z.number().nullable(),
      vestedShares: z.number().nullable(),
      value: z.number().nullable(),
      basis: z.number().nullable(),
      totalChangePct: z.number().nullable(),
      averageBasis: z.number().nullable(),
      weight: z.number().nullable(),
      dailyChangePct: z.number().nullable(),
      weeklyChangePct: z.number().nullable(),
      accounts: z.array(
        z.object({
          accountId: z.string(),
          shares: z.number().nullable(),
          value: z.number().nullable(),
        }),
      ),
    }),
  ),
})
export type Rollup = z.infer<typeof rollupSchema>
const periodRecapSchema = z.object({
  window: changeWindowSchema,
  start: isoDate,
  end: isoDate,
  partial: z.boolean(),
  spending: z.number(),
  income: z.number(),
  net: z.number(),
  spendingChangePct: z.number().optional(),
  incomeChangePct: z.number().optional(),
  netChangePct: z.number().optional(),
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
      /** How far the recovered credits are from the fee, short of it or past it. */
      gap: z.number(),
      /** Credits on purchases made for someone else: used, but not your saving. */
      forOthers: z.number(),
      since: isoDate,
      basis: z.enum(['fee-charge', 'calendar-year', 'card-history']),
      /** The fee year's last day: a year after the fee charge, or December 31 without one. */
      through: isoDate,
      /** Credits the benefit periods still plan on your purchases by `through`. */
      expected: z.number(),
    })
    .optional(),
  activity: z.array(benefitActivitySchema),
  windows: z.array(benefitWindowSchema),
  /** Windows with allowance left, by ID, in the order to use them. */
  useFirst: z.array(z.string()),
  /** Each calendar benefit's periods from the history's first year through this one. */
  history: z.array(benefitHistorySchema),
  /** Each year summed over the benefits shown, with what landed on purchases for others. */
  years: z.array(benefitYearSchema.extend({ forOthers: z.number() })),
  /** Matches that need a decision, newest first. */
  review: z.array(benefitReviewSchema),
})

/** What was spent, beside the period before cut at the same day (none for all time). */
const spentSchema = z.object({
  total: z.number(),
  previous: z.number().nullable(),
  change: z.number().nullable(),
  changePct: z.number().nullable(),
  daily: z.number(),
})
const spendingPeriodSchema = z.object({
  kind: z.enum(['statement', 'week', 'month', 'quarter', 'year', 'all']),
  start: isoDate,
  end: isoDate,
  ...spentSchema.shape,
  pending: z.number(),
  /** The largest purchase's transaction ID. */
  largest: z.string().optional(),
  /** Categories with spending, largest first; a split part counts under its own category. */
  categories: z.array(
    spentSchema.extend({
      name: z.string(),
      share: z.number(),
      /** Usual by this point (zero for a new category) and how far above it; null without history. */
      typical: z.number().nullable(),
      aboveTypical: z.number().nullable(),
    }),
  ),
  /** The statement balance at the period's end. */
  balance: z.number().optional(),
  /** Usual spending by this point, by category, when history covers two earlier periods. */
  typical: z.record(z.string(), z.number()).optional(),
  /** The current period carried to its end at its pace so far. */
  projected: z
    .object({ total: z.number(), categories: z.record(z.string(), z.number()) })
    .optional(),
  /** The saved card's statement balance when the period opened. */
  opening: z.number().optional(),
})
const purchaseSchema = z.object({ id: z.string(), value: z.number() })
/** What a period lists and paces, for all of it or one category. */
const spendingPeriodViewSchema = z.object({
  /** Newest first. */
  transactionIds: z.array(z.string()),
  /** By merchant, refunds netting out, largest spending first. */
  merchants: z.array(
    z.object({
      key: z.string(),
      name: z.string(),
      total: z.number(),
      transactionIds: z.array(z.string()),
    }),
  ),
  pace: z.object({
    start: isoDate,
    /** The period's last day, past today for the current period. */
    end: isoDate,
    current: z.array(z.number()),
    previous: z.array(z.number()),
    projected: z.number().nullable(),
    days: z.array(
      z.object({
        index: z.number().int(),
        date: isoDate,
        total: z.number(),
        purchases: z.array(purchaseSchema),
      }),
    ),
  }),
})
/** One spending period in full, read on demand. */
export const spendingPeriodDetailSchema = spendingPeriodSchema.extend({
  /** The saved history reaches a period before this one. */
  earlier: z.boolean(),
  ...spendingPeriodViewSchema.shape,
  /** All time's bars by month; credits count against them. */
  bars: z
    .array(
      z.object({
        from: isoDate,
        to: isoDate,
        value: z.number().default(0),
        categories: z
          .array(
            z.object({ name: z.string(), value: z.number(), purchases: z.array(purchaseSchema) }),
          )
          .default([]),
      }),
    )
    .default([]),
  /** The view narrowed to each category with spending. */
  focus: z.record(z.string(), spendingPeriodViewSchema),
})
export type SpendingPeriodDetail = z.infer<typeof spendingPeriodDetailSchema>
export type SpendingPeriodView = z.infer<typeof spendingPeriodViewSchema>
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
    /** Month-end spending at the month's daily pace so far. */
    projected: z.number().default(0),
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
          /** Every day from start to end, with that day's spending and purchases. */
          days: z
            .array(z.object({ date: isoDate, spent: z.number(), purchases: z.number().int() }))
            .default([]),
          /** The total over the trip's days, negative. */
          perDay: z.number().default(0),
          destination: z.string(),
          abroad: z.boolean(),
          total: z.number(),
          /** The top categories and each one's share of the trip, in percent. */
          categories: z.array(
            z.object({ name: z.string(), amount: z.number(), share: z.number() }),
          ),
          /** What the top categories leave, positive, and its share. */
          rest: z.number(),
          restShare: z.number(),
          transactionIds: z.array(z.string()),
          ongoing: z.boolean(),
          /** Most likely flown: abroad, far from home, or with an airline or airport charge. */
          flew: z.boolean(),
          /** Each city its purchases were made in, most purchases first. */
          stops: z
            .array(
              z.object({
                city: z.string().optional(),
                region: z.string().optional(),
                country: z.string(),
                lat: z.number().optional(),
                lon: z.number().optional(),
                purchases: z.number().int(),
                first: isoDate,
              }),
            )
            .default([]),
          /** Who went, by person id. */
          companions: z.array(z.string()).optional(),
        }),
      ),
      /** Each year with a trip, newest first. */
      years: z.array(
        z.object({
          year: z.number().int(),
          count: z.number().int(),
          days: z.number().int(),
          spent: z.number(),
          abroad: z.number().int(),
          flown: z.number().int(),
          longest: z.string(),
        }),
      ),
      home: z.array(z.string()),
    })
    .default({ trips: [], years: [], home: [] }),
  people: z
    .object({
      /** Most recent activity first; people with nothing tagged follow by name. */
      people: z.array(
        z.object({
          id: z.string(),
          transactionCount: z.number().int(),
          /** Sent through a payment app, negative. */
          sent: z.number(),
          /** Received through a payment app. */
          received: z.number(),
          first: isoDate.optional(),
          last: isoDate.optional(),
          tripIds: z.array(z.string()),
        }),
      ),
      /** Everything sent to people through payment apps, positive. */
      sent: z.number(),
      /** Names Zelle descriptors carry that match nobody, and who saved they could be. */
      suggestions: z.array(
        z.object({
          name: z.string(),
          matchName: z.string(),
          transactionIds: z.array(z.string()),
          matches: z
            .array(z.object({ id: z.string(), fit: z.enum(['likely', 'possible']) }))
            .default([]),
        }),
      ),
    })
    .default({ people: [], sent: 0, suggestions: [] }),
  /**
   * Spending's scopes (an account ID, or `all`), whose spending each counts, their current period
   * of each kind, and their recent months. Earlier periods are read one at a time; see
   * `getSpendingPeriod`. Someone else's spending and everyone's appear once a purchase is theirs.
   */
  spending: z
    .array(
      z.object({
        scope: z.string(),
        owner: z.enum(spendingOwners),
        periods: z.array(spendingPeriodSchema),
        /** The last six calendar months, oldest first, this one to date. */
        months: z.array(z.object({ month: z.string(), total: z.number() })),
      }),
    )
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
      excludedAccountIds: z.array(z.string()),
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
          window: z.enum(['day', 'week', 'month', 'quarter', 'year-to-date', 'year', 'all']),
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
      /** The accounts' uninvested cash, and its share of the total. */
      cash: z.number(),
      cashPercent: z.number(),
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
  retirement: retirementSchema,
  documents: documentInsightsSchema,
  changes: z.array(
    z.object({
      window: changeWindowSchema,
      start: isoDate,
      end: isoDate,
      accounts: z.array(z.object({ accountId: z.string(), change: z.number() })),
      incomplete: z.boolean(),
      transactions: z.number().int().nonnegative(),
      /** Each type's change over every account, and over the counted ones; null when an
       * account's history doesn't reach the period. The spending cards' change apart. */
      groups: z.array(typeChangeSchema),
      listed: z.array(typeChangeSchema),
      cards: z.number().nullable().optional(),
    }),
  ),
  /** What moved through each account, and the combined one, over each chart period. */
  flows: z.array(
    z.object({
      accountId: z.string(),
      window: changeWindowSchema,
      start: isoDate.nullable(),
      end: isoDate,
      moneyIn: z.number(),
      moneyOut: z.number(),
      netFlow: z.number(),
      interest: z.number(),
      dividends: z.number(),
      contributions: z.number().nullable(),
      marketChange: z.number().nullable(),
    }),
  ),
  /** Each day's and month's spending and income across every account, for the calendar. */
  activityDays: z.array(activityTotalsSchema),
  activityMonths: z.array(activityTotalsSchema),
  /** Account totals from saved values; a live projection carries its own. */
  rollup: rollupSchema,
  /** Each value chart's change at its saved values; the market stream sends its own. */
  chartChanges: chartChangesSchema,
  interest: z.array(
    z.object({
      accountId: z.string(),
      monthlyNetFlow: z.number().nullable(),
      historyDays: z.number().int().optional(),
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
  /** Dated items for the Calendar page and the Brief calendar, by date. */
  agenda: z
    .array(
      z.object({
        id: z.string(),
        kind: z.enum([
          'paymentDue',
          'statementCloses',
          'creditExpiring',
          'bill',
          'subscription',
          'income',
          'dividend',
          'longTerm',
          'washSaleEnds',
          'trip',
        ]),
        date: isoDate,
        /** The last day of a trip. */
        end: isoDate.optional(),
        title: z.string().optional(),
        accountId: z.string().optional(),
        ticker: z.string().optional(),
        /** As cash moves; absent when unknown. */
        amount: z.number().optional(),
        estimated: z.boolean(),
        /** How often a forecast series recurs, as a phrase. */
        frequency: z.string().optional(),
        /** The range a forecast amount has posted in, when it isn't one amount. */
        amountLow: z.number().optional(),
        amountHigh: z.number().optional(),
        /** A payment due's minimum, as the issuer reported it. */
        minimumPayment: z.number().optional(),
      }),
    )
    .default([]),
})
export type Insights = z.infer<typeof insightsSchema>
export type AgendaItem = Insights['agenda'][number]
export type ExpectedEvent = Insights['expected'][number]
export type BenefitWindow = NonNullable<Insights['benefits']>['windows'][number]
export type BenefitActivity = BenefitWindow['activity'][number]
export type BenefitHistory = NonNullable<Insights['benefits']>['history'][number]
export type BenefitPeriod = BenefitHistory['periods'][number]
export type BenefitReview = NonNullable<Insights['benefits']>['review'][number]
export type AttentionItem = Insights['attention'][number]
export type CardBenefits = z.infer<typeof cardBenefitsSchema>
export type SpendingPeriodJudgment = z.infer<typeof spendingPeriodSchema>
export type SpendingFigures = z.infer<typeof spentSchema>
export type Allocation = NonNullable<Insights['allocation']>
export type ChangeWindow = z.infer<typeof changeWindowSchema>
export type CreditInsight = Insights['credit']
export type Trip = Insights['trips']['trips'][number]
export type Trips = Insights['trips']
export type Taxes = Insights['taxes']
export type TaxYear = z.infer<typeof taxYearSchema>
export type Subscription = Insights['subscriptions']['items'][number]
