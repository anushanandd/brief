import type { Holding } from '../lib/bindings/Holding'
import type { BriefDocument, FinanceSettings, Insights, Reports, Transaction } from '../lib/schema'

const initial = () => ({
  recovery: undefined as { message: string; canRestore: boolean } | undefined,
  annotationWarning: undefined as string | undefined,
  /** Holdings by the fields a test names; the harness fills the rest as Rust would. */
  holdings: [] as Array<Pick<Holding, 'ticker' | 'accountId'> & Partial<Holding>>,
  trades: [] as Array<Record<string, unknown>>,
  /** Transactions by the fields a test names; the harness fills the rest as Rust would. */
  extraTransactions: [] as Array<
    Pick<Transaction, 'id' | 'date' | 'amount'> & Partial<Transaction>
  >,
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
  /** Reports as Rust would read them, for the tests of a report page. */
  reports: {} as Partial<Reports>,
  people: [] as FinanceSettings['people'],
  tripCompanions: [] as FinanceSettings['tripCompanions'],
  benefitAmount: 10,
  /** Files the seven synthetic card credits as income, for tests of a report's chart. */
  creditKind: 'other' as 'other' | 'income',
  postingDate: undefined as string | undefined,
  activityMethod: undefined as string | undefined,
  snaptradeReference: false,
  documents: [] as BriefDocument[],
  /** Opens with screen privacy on, as the hide-on-launch preference does. */
  valuesHidden: false,
})

/** The synthetic finance scenario the route tests adjust; `routes.harness` serves it. */
export const scenario = initial()

/** Puts every field back as a test found it. */
export const resetScenario = () => Object.assign(scenario, initial())
