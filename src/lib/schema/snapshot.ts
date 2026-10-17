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
  /** Days a manual account's value was observed: an entry or a statement's price. */
  observedOn: z.array(z.iso.date()).optional(),
  /** Fund units a manual account's latest statement shows, at the statement's price. */
  statementPositions: z
    .array(
      z.object({
        name: z.string(),
        units: z.number(),
        price: z.number(),
        value: z.number(),
        pricedOn: z.iso.date(),
      }),
    )
    .optional(),
  /** `estimated`: a manual account carried past `balanceAsOf` with the S&P 500. */
  balanceSource: z
    .enum(['reported', 'cash-and-positions', 'estimated', 'unavailable'])
    .nullable()
    .optional(),
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
  /** The account number's last four characters, as statements and payslips print them. */
  mask: z.string().optional(),
  holderCategory: z.string().optional(),
  taxTreatment: z.enum(['taxable', 'taxDeferred', 'taxFree']).optional(),
  /** The institution's registered domain, from its provider-reported website. */
  domain: z
    .string()
    .regex(/^[a-z0-9-]+\.[a-z0-9-]+$/)
    .optional()
    .catch(undefined),
})

const holdingSchema = z.object({
  ticker: z.string(),
  name: z.string(),
  accountId: z.string(),
  shares: z.number().nullable(),
  price: z.number().nullable(),
  value: z.number().nullable(),
  /** Unknown stays unknown; Rust never sends a basis it cannot find. */
  costBasis: z.number().nullable().default(null),
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
  /** `manual` for an account valued by hand or by its imported statements. */
  historySource: z
    .enum([
      'reported',
      'provider-estimated',
      'estimated',
      'transaction-derived',
      'manual',
      'unavailable',
    ])
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
  /** A purchase made for someone else that would otherwise be spending; sent only when true. */
  othersSpending: z.boolean().optional(),
})

export const manualAccountKinds = [
  'cash',
  'brokerage',
  'retirement',
  'property',
  'vehicle',
  'loan',
] as const
export const manualAccountSchema = z.object({
  /** `manual:` followed by an identifier chosen when it was added. */
  id: z.string().startsWith('manual:').max(64),
  name: z.string().min(1).max(80),
  kind: z.enum(manualAccountKinds),
  /**
   * Dated balances, oldest first. A loan's balance is what is owed, entered positive. Empty
   * when an imported statement values the account.
   */
  entries: z.array(z.object({ date: z.iso.date(), value: z.number() })).max(1000),
  /** Carry the last known value forward with the S&P 500 until the next statement or entry. */
  tracksSp500: z.boolean().optional(),
})
export type ManualAccount = z.infer<typeof manualAccountSchema>

export const personSchema = z.object({
  /** `person:` followed by an identifier chosen when they were added. */
  id: z.string().startsWith('person:').max(64),
  name: z.string().min(1).max(80),
  /** The Mac Contacts card they were added from, read only for its photo. */
  contactId: z.string().optional(),
  /** Names a bank prints for them; a payment to or from a person carrying one is theirs. */
  matchNames: z.array(z.string()).optional(),
})
export type Person = z.infer<typeof personSchema>

export const documentKinds = [
  'payslip',
  'taxForm',
  'taxReturn',
  'statement',
  'export',
  'other',
] as const

export const documentFieldSchema = z.object({
  key: z.string(),
  label: z.string(),
  amount: z.number().optional(),
  /** A pay statement's year-to-date figure. */
  ytd: z.number().optional(),
  text: z.string().optional(),
  /** The reading disagreed with itself; the user should check it. */
  uncertain: z.boolean().optional(),
  /** The user typed it; a later reading keeps it. */
  edited: z.boolean().optional(),
})
export type DocumentField = z.infer<typeof documentFieldSchema>

export const documentSchema = z.object({
  id: z.string(),
  fileName: z.string(),
  mediaType: z.string(),
  size: z.number(),
  importedAt: z.string(),
  kind: z.enum(documentKinds),
  form: z.string().optional(),
  corrected: z.boolean().optional(),
  issuer: z.string().optional(),
  taxYear: z.number().int().optional(),
  date: z.iso.date().optional(),
  periodStart: z.iso.date().optional(),
  periodEnd: z.iso.date().optional(),
  accountId: z.string().optional(),
  mask: z.string().optional(),
  fields: z.array(documentFieldSchema).default([]),
  deposits: z.array(z.object({ mask: z.string().optional(), amount: z.number() })).default([]),
  checks: z.array(z.object({ label: z.string(), passed: z.boolean() })).default([]),
  statementRows: z.number().int().optional(),
  statementCsv: z.string().optional(),
  statementFileId: z.string().optional(),
  reviewed: z.boolean().optional(),
  person: z.string().optional(),
  unreadable: z.string().optional(),
  /** Written on this Mac by Apple Intelligence; every figure in it is printed in the document. */
  summary: z.string().optional(),
})
export type BriefDocument = z.infer<typeof documentSchema>

export const financeSettingsSchema = z.object({
  spendingAccountId: z.string().optional(),
  /** More cards used for spending; like the saved one, they stay out of the accounts total. */
  spendingCards: z.array(z.string()).optional(),
  /** What the user records about an account: its page and the APY it states. */
  accountDetails: z
    .record(
      z.string(),
      z.object({
        website: z.string().startsWith('https://').optional(),
        apy: z.number().min(0).max(100).optional(),
      }),
    )
    .optional(),
  accountNames: z.record(z.string(), z.string()).default({}),
  accountStartDates: z.record(z.string(), z.iso.date()).default({}),
  /** The day an account was closed; it leaves today's accounts and totals. */
  accountClosedDates: z.record(z.string(), z.iso.date()).default({}),
  /** Accounts no provider reports, valued by hand. */
  manualAccounts: z.array(manualAccountSchema).default([]),
  hiddenBenefitIds: z.array(z.string()).default([]),
  plaidLiabilities: z.boolean().default(false),
  backgroundRefresh: z.boolean().default(false),
  notifications: z.boolean().default(false),
  /** Keep the Brief calendar in macOS Calendar matched to the agenda. */
  calendarSync: z.boolean().default(false),
  calendarId: z.string().optional(),
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
  retirement: z
    .object({
      /** Only decides catch-up eligibility. */
      birthYear: z.number().int().optional(),
      /** Monthly costs to add to today's spending, such as rent after moving out. */
      plannedMonthly: z.number().nonnegative().optional(),
      /** Roth IRA contributions made before the account's history in Brief. */
      earlierContributions: z.record(z.string(), z.number().nonnegative()).optional(),
      /** The tax year an IRA inflow counts toward; null when it isn't a contribution. */
      contributionYears: z.record(z.string(), z.number().int().nullable()).optional(),
    })
    .optional(),
  plaidInvestmentTransactions: z.boolean().default(false),
  plaidRecurring: z.boolean().default(false),
  people: z.array(personSchema).default([]),
  /** Who went on a trip, saved against the trip's transactions because trip ids move. */
  tripCompanions: z
    .array(z.object({ transactionIds: z.array(z.string()), people: z.array(z.string()) }))
    .default([]),
})
export type FinanceSettings = z.infer<typeof financeSettingsSchema>

/** One change a refresh found in a saved transaction. */
const transactionChangeSchema = z.object({
  id: z.string(),
  observedAt: z.string(),
  kind: z.enum(['added', 'settled', 'removed', 'amended']),
  transactionId: z.string(),
  accountId: z.string(),
  merchant: z.string(),
  date: z.string(),
  amount: z.number(),
  previousAmount: z.number().optional(),
  pending: z.boolean(),
})
export type TransactionChange = z.infer<typeof transactionChangeSchema>

const qualityEvidenceSchema = z.object({
  grade: z.enum(['conflict', 'weak', 'supported', 'confirmed']),
  basis: z.enum([
    'statement',
    'statementDiffers',
    'statementOnly',
    'pending',
    'provider',
    'providerConfident',
    'providerUnsure',
    'manual',
    'sourcesAgree',
    'sourcesDisagree',
    'descriptorOnly',
    'bankOnly',
    'unreported',
    'issuer',
    'providerContradicted',
    'placeMatched',
  ]),
})
export type QualityEvidence = z.infer<typeof qualityEvidenceSchema>

/** How well each detail of a bank or card transaction is supported; the weakest sets `grade`. */
const transactionQualitySchema = z.object({
  grade: qualityEvidenceSchema.shape.grade,
  amount: qualityEvidenceSchema,
  merchant: qualityEvidenceSchema.optional(),
  category: qualityEvidenceSchema,
  location: qualityEvidenceSchema.optional(),
  statementAmount: z.number().optional(),
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
      /** What the card issuer's imported export says about the merchant. */
      issuer: z
        .object({
          name: z.string().optional(),
          address: z.string().optional(),
          city: z.string().optional(),
          region: z.string().optional(),
          postalCode: z.string().optional(),
          country: z.string().optional(),
          category: z.string().optional(),
          phone: z.string().optional(),
        })
        .optional(),
      /**
       * Where it was made, as best known: the issuer's address, the bank's descriptor, then
       * `location`. Absent for a purchase made remotely.
       */
      place: z
        .object({
          address: z.string().optional(),
          postalCode: z.string().optional(),
          city: z.string().optional(),
          region: z.string().optional(),
          country: z.string(),
          lat: z.number().optional(),
          lon: z.number().optional(),
          source: z.enum(['issuer', 'descriptor', 'bank']),
          precision: z.enum(['exact', 'street', 'city', 'region']),
        })
        .optional(),
      /** Whether the cardholder was there, as Rust weighed the records. */
      presence: z.enum(['inPerson', 'remote']).optional(),
      /** Plaid's name, when the bank's own records contradict it. */
      providerMerchant: z.string().optional(),
      /** The business Rust matched in Overture's places, when exactly one fits. */
      business: z
        .object({
          id: z.string(),
          name: z.string(),
          category: z.string().optional(),
          address: z.string().optional(),
          phone: z.string().optional(),
          website: z.string().optional(),
          lat: z.number(),
          lon: z.number(),
          closed: z.boolean().optional(),
        })
        .optional(),
      paymentChannel: z.string().optional(),
      amount: z.number(),
      account: z.string(),
      accountId: z.string().optional(),
      pending: z.boolean(),
      benefitConfirmed: z.boolean().optional(),
      /** The card benefit this transaction was assigned to by hand. */
      benefitId: z.string().optional(),
      logoUrl: z.string().optional(),
      website: z.string().optional(),
      logoName: z.string().optional(),
      categoryDetail: z.string().optional(),
      categoryConfidence: z.string().optional(),
      counterpartyType: z.string().optional(),
      transactionCode: z.string().optional(),
      categorySource: z.enum(['provider', 'manual']).optional(),
      providerCategory: z.string().optional(),
      reviewed: z.boolean().optional(),
      recurringId: z.string().optional(),
      tags: z.array(z.string()).optional(),
      splits: z.array(z.object({ category: z.string(), amount: z.number() })).optional(),
      refundOf: z.string().optional(),
      /** The other leg of a move between two of your accounts, as native policy paired them. */
      transferPairId: z.string().optional(),
      /** Someone else's money: a purchase made for them, or what they paid you back. */
      other: z.boolean().optional(),
      /** A note in the user's own hand. */
      note: z.string().optional(),
      /** Person ids: chosen by hand, or matched by name. */
      people: z.array(z.string()).optional(),
      /** Absent when nobody was chosen and no name matched. */
      peopleSource: z.enum(['manual', 'matched']).optional(),
      merchantEntityId: z.string().optional(),
      dividendType: z.enum(['qualified', 'nonQualified']).optional(),
      /** What the institution called cash moving into or out of an investment account. */
      flow: z
        .enum([
          'contribution',
          'deposit',
          'transfer',
          'withdrawal',
          'deferral',
          'employer',
          'afterTax',
        ])
        .optional(),
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
      /** How well each detail is supported, graded by Rust. */
      quality: transactionQualitySchema.optional(),
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
        /** How much of the change that refresh's transaction changes account for. */
        explained: z.number().optional(),
      }),
    )
    .default([]),
  /** What refreshes changed in saved transactions, oldest first. */
  transactionChanges: z.array(transactionChangeSchema).default([]),
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
  /** Imported statements checked against Plaid, per account. */
  statementReconciliation: z
    .array(
      z.object({
        accountId: z.string(),
        from: z.string(),
        through: z.string(),
        matched: z.array(z.string()),
        amountDiffers: z.array(
          z.object({ transactionId: z.string(), statementAmount: z.number() }),
        ),
        missing: z.array(
          z.object({
            fileId: z.string(),
            rowId: z.string(),
            date: z.string(),
            amount: z.number(),
            description: z.string(),
            confirmed: z.boolean(),
          }),
        ),
        extra: z.array(z.string()),
      }),
    )
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
  /** Accounts the user closed, which `accounts` no longer lists. */
  closedAccounts: z
    .array(
      z.object({
        id: z.string(),
        name: z.string(),
        institution: z.string(),
        type: z.string(),
        closedOn: z.iso.date(),
      }),
    )
    .optional(),
  insights: insightsSchema.optional(),
  /** What Brief read from the user's documents; the originals stay encrypted in Rust. */
  documents: z.array(documentSchema).optional(),
  /** How far matching purchases to businesses has got. */
  places: z
    .object({
      inPerson: z.number(),
      matched: z.number(),
      searchable: z.number(),
      unsearched: z.number(),
    })
    .optional(),
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
          explained: z.number().optional(),
        }),
      ),
      newTransactionIds: z.array(z.string()),
      /** Absent when the refresh changed no saved transaction. */
      transactionChanges: z.array(transactionChangeSchema).optional(),
      /** New transactions dated well before the previous refresh: history arriving late. */
      backfilled: z.number().int().nonnegative().optional(),
    })
    .optional(),
})

export type FinanceSnapshot = z.infer<typeof financeSnapshotSchema>
export type Account = FinanceSnapshot['accounts'][number]
export type Transaction = FinanceSnapshot['transactions'][number]
export type Trade = FinanceSnapshot['trades'][number]
