import type { BriefDocument, FinanceSettings, Insights, Transaction } from '../lib/schema'

const initial = () => ({
  recovery: undefined as { message: string; canRestore: boolean } | undefined,
  annotationWarning: undefined as string | undefined,
  holdings: [] as Array<Record<string, unknown>>,
  trades: [] as Array<Record<string, unknown>>,
  extraTransactions: [] as Transaction[],
  incomplete: false,
  providerError: false,
  singleHistory: false,
  homeHistory: false,
  accountType: 'credit',
  otherAccountType: 'credit',
  otherKind: 'other' as Transaction['classification']['kind'],
  otherCategory: 'Credit',
  otherMerchant: 'Other account transaction',
  partialBasis: false,
  linkedPending: false,
  uberPurchase: false,
  hiddenBenefits: '',
  spendingAccountId: 'selected',
  insights: {} as Partial<Insights>,
  people: [] as FinanceSettings['people'],
  tripCompanions: [] as FinanceSettings['tripCompanions'],
  benefitAmount: 10,
  /** Files the seven synthetic card credits as income, for tests of a report's chart. */
  creditKind: 'other' as 'other' | 'income',
  postingDate: undefined as string | undefined,
  activityMethod: undefined as string | undefined,
  snaptradeReference: false,
  documents: [] as BriefDocument[],
})

/** The synthetic finance scenario the route tests adjust; `routes.harness` serves it. */
export const scenario = initial()

/** Puts every field back as a test found it. */
export const resetScenario = () => Object.assign(scenario, initial())
