/**
 * Renderer test fixtures in Rust's own shapes. Views come from the Rust seed test
 * (`empty-view.json`, `seed-view.json`); each builder takes the fields a test is about and fills
 * the rest with what Rust sends for an unknown or absent value.
 */
import type { Holding } from '../../lib/bindings/Holding'
import type { Performance } from '../../lib/bindings/Performance'
import { fromRust } from '../../lib/rust-json'
import type {
  Account,
  BriefDocument,
  FinanceSnapshot,
  InvestmentSummary,
  Trade,
  Transaction,
} from '../../lib/schema'
import emptyView from '../empty-view.json'
import seedView from '../seed-view.json'
import { classification } from './classification'

/** A view of no saved data. */
export const emptySnapshot = () => fromRust<FinanceSnapshot>(structuredClone(emptyView))

/** The synthetic seed's view, as the browser preview shows it. */
export const seedSnapshot = () => fromRust<FinanceSnapshot>(structuredClone(seedView.view))

/** The portfolio summary Rust made from the seed's synthetic closes. */
export const seedSummary = () => fromRust<InvestmentSummary>(structuredClone(seedView.summary))

export const account = (fields: Pick<Account, 'id'> & Partial<Account>): Account => ({
  name: fields.id,
  institution: 'Example Bank',
  type: 'cash',
  value: null,
  cashValue: null,
  investedValue: null,
  knownCostBasis: null,
  knownUnrealizedGain: null,
  knownUnrealizedGainPct: null,
  investmentIncomeYtd: null,
  saleProceedsYtd: null,
  estimatedRealizedGainYtd: null,
  balanceAsOf: null,
  currency: null,
  balanceFetchedAt: null,
  balanceSource: null,
  reportedBalance: null,
  positionsAsOf: null,
  activityAsOf: null,
  ...fields,
})

export const transaction = (
  fields: Pick<Transaction, 'id' | 'date' | 'amount'> & Partial<Transaction>,
): Transaction => ({
  merchant: 'Example merchant',
  category: 'Other',
  account: 'Example account',
  occurredOn: null,
  postedOn: null,
  pending: false,
  description: null,
  classification: classification(fields.amount < 0 ? 'expense' : 'income'),
  ...fields,
})

export const holding = (
  fields: Pick<Holding, 'ticker' | 'accountId'> & Partial<Holding>,
): Holding => ({
  name: fields.ticker,
  shares: null,
  price: null,
  value: null,
  costBasis: null,
  unrealizedGain: null,
  dailyChangePct: null,
  weeklyChangePct: null,
  weeklyReferencePrice: null,
  weeklyReferenceDate: null,
  totalChangePct: null,
  marketAsOf: null,
  color: '',
  currency: 'USD',
  valuationNote: null,
  ...fields,
})

export const trade = (
  fields: Pick<Trade, 'id' | 'type' | 'date' | 'amount' | 'accountId'> & Partial<Trade>,
): Trade => ({
  account: fields.accountId,
  ticker: null,
  description: null,
  units: null,
  price: null,
  realizedCostBasis: null,
  estimatedRealizedGain: null,
  estimatedRealizedGainPct: null,
  ...fields,
})

export const performance = (
  fields: Pick<Performance, 'accountId'> & Partial<Performance>,
): Performance => ({
  name: fields.accountId,
  institution: 'Example Broker',
  currentValue: 0,
  historyStart: null,
  points: [],
  ...fields,
})

export const document = (
  fields: Pick<BriefDocument, 'id' | 'kind'> & Partial<BriefDocument>,
): BriefDocument => ({
  fileName: `${fields.id}.pdf`,
  mediaType: 'application/pdf',
  size: 1,
  importedAt: '2026-09-01T12:00:00Z',
  fields: [],
  deposits: [],
  checks: [],
  reader: 0,
  ...fields,
})
