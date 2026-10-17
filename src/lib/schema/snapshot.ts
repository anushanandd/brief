/**
 * The committed finance snapshot as the renderer receives it. The types are generated from
 * Rust's (`src/lib/bindings`). Settings keep a runtime schema: the renderer edits them, and
 * they arrive from browser storage and imported files, which are not Rust's.
 */
import { z } from 'zod'

import type { Classification } from '../bindings/Classification'
import type { FinanceSettings } from '../bindings/FinanceSettings'
import type { FinanceSnapshot as View } from '../bindings/FinanceSnapshot'

export type { Document as BriefDocument } from '../bindings/Document'
export type { Field as DocumentField } from '../bindings/Field'
export type { Evidence as QualityEvidence } from '../bindings/Evidence'
export type { ManualAccount } from '../bindings/ManualAccount'
export type { Person } from '../bindings/Person'
export type { TransactionChange } from '../bindings/TransactionChange'
export type { FinanceSettings }

/**
 * A transaction as the renderer reads it. Saved transactions carry no classification; the view
 * always does, from Rust's policy (`derive_view`, checked by a Rust test).
 */
export type Transaction = Omit<View['transactions'][number], 'classification'> & {
  classification: Classification
}
export type FinanceSnapshot = Omit<View, 'transactions'> & { transactions: Transaction[] }
export type Account = FinanceSnapshot['accounts'][number]
export type Trade = FinanceSnapshot['trades'][number]

export const manualAccountKinds = [
  'cash',
  'brokerage',
  'retirement',
  'property',
  'vehicle',
  'loan',
] as const

export const documentKinds = [
  'payslip',
  'taxForm',
  'taxReturn',
  'statement',
  'export',
  'other',
] as const

const manualAccountSchema = z.object({
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

const personSchema = z.object({
  /** `person:` followed by an identifier chosen when they were added. */
  id: z.string().startsWith('person:').max(64),
  name: z.string().min(1).max(80),
  /** The Mac Contacts card they were added from, read only for its photo. */
  contactId: z.string().optional(),
  /** Names a bank prints for them; a payment to or from a person carrying one is theirs. */
  matchNames: z.array(z.string()).optional(),
})

/** Settings as the renderer edits, stores, and imports them; Rust validates them again. */
export const financeSettingsSchema: z.ZodType<FinanceSettings> = z.object({
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
