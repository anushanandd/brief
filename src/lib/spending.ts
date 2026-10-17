import { dayKey } from './dates'
import { formatDay } from './format'
import type {
  Account,
  FinanceSnapshot,
  Insights,
  SpendingOwner,
  SpendingFigures,
  SpendingPeriodDetail,
  SpendingPeriodJudgment,
  SpendingPeriodView,
  Transaction,
} from './schema'
import { isSpendingTransaction } from './transaction-kind'

export type SpendingPeriod = 'statement' | 'week' | 0 | 1 | 3 | 12

/**
 * Spending's periods, each with the range Money's links carry (so every card in its strip opens
 * on the same period), its name, its key, and the kind of period Rust computes for it.
 */
export const spendingPeriodOptions = [
  {
    range: 'statement',
    value: 'statement',
    label: 'Statement',
    key: 's',
    unit: 'statement',
    kind: 'statement',
  },
  { range: 'week', value: 'week', label: 'Week', key: 'w', unit: 'week', kind: 'week' },
  { range: 'month', value: 1, label: 'Month', key: 'm', unit: 'month', kind: 'month' },
  { range: 'quarter', value: 3, label: 'Quarter', key: 'q', unit: 'quarter', kind: 'quarter' },
  { range: 'year', value: 12, label: 'Year', key: 'y', unit: 'year', kind: 'year' },
  { range: 'all', value: 0, label: 'All time', key: 'a', unit: 'year', kind: 'all' },
] as const satisfies ReadonlyArray<{
  range: string
  value: SpendingPeriod
  label: string
  key: string
  /** What the period is called in "This month" and "vs last month". */
  unit: string
  kind: SpendingPeriodJudgment['kind']
}>
export type SpendingRange = (typeof spendingPeriodOptions)[number]['range']
export const spendingRanges = spendingPeriodOptions.map(({ range }) => range)
export const spendingPeriodForRange = (range: unknown): SpendingPeriod | undefined =>
  spendingPeriodOptions.find((option) => option.range === range)?.value
export const spendingRangeForPeriod = (period: SpendingPeriod): SpendingRange =>
  spendingPeriodOptions.find((option) => option.value === period)!.range
/** The kind of period Rust computes for a choice; a statement without dates is the month. */
export const spendingPeriodKind = (period: SpendingPeriod): SpendingPeriodJudgment['kind'] =>
  spendingPeriodOptions.find((option) => option.value === period)!.kind
export type SpendingPeriodBasis = 'calendar' | 'statement'
/** One bar of the spending chart: the days it covers and its spending by category. */
export type SpendingBar = {
  from: string
  to: string
  value: number
  categories: Array<{
    name: string
    value: number
    activities: Array<{ id: string; value: number }>
  }>
}

const isoDate = /^\d{4}-\d{2}-\d{2}/

/** A transaction's calendar day: an ISO date as written, or an instant's UTC day. */
export function transactionDateKey(value: string) {
  if (isoDate.test(value)) return value.slice(0, 10)
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? '' : dayKey(parsed)
}

export function formatActivityDate(value: string) {
  const transactionDate = transactionDateKey(value)
  return transactionDate ? formatDay(transactionDate) : value
}

/** Lists name the year only when it isn't the snapshot's year. */
export function formatListDate(value: string, referenceIso: string) {
  const date = formatActivityDate(value)
  const year = transactionDateKey(referenceIso)?.slice(0, 4)
  return year && date.endsWith(`, ${year}`) ? date.slice(0, -`, ${year}`.length) : date
}

export function resolveSpendingAccount(accounts: Account[], preferredId: string) {
  return accounts.find(({ id, type }) => type === 'credit' && id === preferredId)
}

export const allSpendingAccounts = 'all'

/**
 * What Spending covers: one account, or every USD account with spending. The saved card is
 * the default and the only account with statements and benefits; no other card is ever
 * chosen in its place.
 */
export function spendingScope(
  accounts: Account[],
  transactions: Transaction[],
  savedCardId: string,
  requested?: string,
) {
  const card = resolveSpendingAccount(accounts, savedCardId)
  const spenders = new Set(
    transactions
      .filter(isSpendingTransaction)
      .flatMap(({ accountId }) => (accountId ? [accountId] : [])),
  )
  const options = accounts.filter(
    ({ id, currency }) =>
      id !== 'all' && (!currency || currency === 'USD') && (spenders.has(id) || id === card?.id),
  )
  const selected =
    requested === allSpendingAccounts
      ? undefined
      : (options.find(({ id }) => id === requested) ?? card)
  return { card, options, selected }
}

export const spendingCategoryColor = (name: string) => {
  const category = name.toLocaleLowerCase()
  if (/housing|rent|mortgage|^home$/.test(category)) return 'var(--spending-housing)'
  if (/travel|flight|hotel/.test(category)) return 'var(--spending-travel)'
  if (/food|dining|grocer/.test(category)) return 'var(--spending-food)'
  if (/shopping|retail|merchandise/.test(category)) return 'var(--spending-shopping)'
  if (/transport|gas|ride/.test(category)) return 'var(--spending-transport)'
  if (/entertainment|subscription/.test(category)) return 'var(--spending-entertainment)'
  if (/utilit|bill/.test(category)) return 'var(--spending-utilities)'
  if (/health|medical/.test(category)) return 'var(--data-4)'
  if (/personal care/.test(category)) return 'var(--data-5)'
  if (/service/.test(category)) return 'var(--data-2)'
  if (/giving|people/.test(category)) return 'var(--data-1)'
  if (/tax/.test(category)) return 'var(--data-3)'
  if (/government/.test(category)) return 'var(--data-6)'
  return 'var(--spending-other)'
}

export function spendingPeriodLabel(
  period: SpendingPeriod,
  periodOffset: number,
  selectedPeriod: string,
  basis: SpendingPeriodBasis = 'calendar',
) {
  if (period === 'statement') {
    if (basis === 'statement')
      return periodOffset === 0 ? 'Current statement' : `Statement ending ${selectedPeriod}`
    return periodOffset === 0 ? 'This month' : selectedPeriod
  }
  if (period === 'week') return periodOffset === 0 ? 'This week' : `Week ending ${selectedPeriod}`
  if (periodOffset === 0) {
    if (period === 1) return 'This month'
    if (period === 3) return 'This quarter'
    if (period === 12) return 'This year'
    return 'All time'
  }
  if (period === 1) return selectedPeriod
  if (period === 3) return `Quarter through ${selectedPeriod}`
  if (period === 12) return `Year through ${selectedPeriod}`
  return `All time through ${selectedPeriod}`
}

/** Rust's current periods of one scope, counted for one owner; see `insights.spending`. */
export function spendingJudgments(
  insights: Pick<Insights, 'spending'> | undefined,
  scope: string,
  owner: SpendingOwner = 'me',
): SpendingPeriodJudgment[] {
  return (
    insights?.spending.find((entry) => entry.scope === scope && entry.owner === owner)?.periods ??
    []
  )
}

/** A scope's current period of one kind; a statement without dates is the month. */
export function currentSpendingPeriod(
  insights: Pick<Insights, 'spending'> | undefined,
  scope: string,
  kind: SpendingPeriodJudgment['kind'],
) {
  const periods = spendingJudgments(insights, scope)
  return (
    periods.find((period) => period.kind === kind) ??
    (kind === 'statement' ? periods.find((period) => period.kind === 'month') : undefined)
  )
}

/**
 * What Spending's headline shows for a period: the statement balance when the period is a real
 * statement, otherwise the period's spending, with its change from the same point last period.
 */
export function spendingHeadline(
  data: Pick<FinanceSnapshot, 'accounts' | 'transactions'> & {
    insights?: Pick<Insights, 'spending'>
  },
  spendingAccountId: string,
  period: SpendingPeriod,
  requested?: string,
) {
  const scope = spendingScope(data.accounts, data.transactions, spendingAccountId, requested)
  const current = currentSpendingPeriod(
    data.insights,
    scope.selected?.id ?? allSpendingAccounts,
    spendingPeriodKind(period),
  )
  return {
    hasScope: scope.options.length > 0,
    value: current?.kind === 'statement' ? (current.balance ?? 0) : (current?.total ?? 0),
    change: period ? (current?.change ?? null) : null,
  }
}

/** Whose spending the page counts, in the order F cycles through them. */
export const spendingOwnerOptions = [
  { value: 'me', label: 'For me' },
  { value: 'other', label: 'For other' },
  { value: 'all', label: 'All spending' },
] as const satisfies ReadonlyArray<{ value: SpendingOwner; label: string }>

const nothingSpent: SpendingFigures = {
  total: 0,
  previous: null,
  change: null,
  changePct: null,
  daily: 0,
}

/** Rust's figures for a period, or for one of its categories; nothing when Rust has none. */
export function spendingFigures(period: SpendingPeriodJudgment | undefined, focus?: string) {
  return (focus ? period?.categories.find(({ name }) => name === focus) : period) ?? nothingSpent
}

/** A period's view for all of it, or for the category in focus; nothing listed when it has none. */
export function spendingView(detail: SpendingPeriodDetail, focus?: string): SpendingPeriodView {
  if (!focus) return detail
  return (
    detail.focus[focus] ?? {
      transactionIds: [],
      merchants: [],
      pace: { ...detail.pace, current: [], previous: [], projected: null, days: [] },
    }
  )
}

/** Activity IDs as the page's lists and charts name transactions. */
const activityId = (id: string) => `spending:${id}`

/** Rust's pace as the pace chart draws it. */
export function spendingChartPace(pace: SpendingPeriodView['pace']) {
  return {
    ...pace,
    days: pace.days.map(({ purchases, ...day }) => ({
      ...day,
      activities: purchases.map(({ id, value }) => ({ id: activityId(id), value })),
    })),
  }
}

/**
 * Rust's bars as the bar chart draws them; a focus keeps only its own category. Rust leaves out
 * empty bars, zero values, and empty category lists.
 */
export function spendingChartBars(bars: SpendingPeriodDetail['bars'] = [], focus?: string) {
  return bars.map(({ from, to, value = 0, categories = [] }): SpendingBar => {
    const shown = categories
      .filter(({ name }) => !focus || name === focus)
      .map(({ name, value: amount, purchases }) => ({
        name,
        value: amount,
        activities: purchases.map(({ id, value: spent }) => ({ id: activityId(id), value: spent })),
      }))
    return { from, to, value: focus ? (shown[0]?.value ?? 0) : value, categories: shown }
  })
}

/** A scope's spending in each of its recent calendar months, as the monthly chart reads it. */
export function monthlySpendingHistory(
  insights: Pick<Insights, 'spending'> | undefined,
  scope: string,
) {
  return (
    insights?.spending.find((entry) => entry.scope === scope && entry.owner === 'me')?.months ?? []
  ).map(({ month, total }) => ({ month, value: total }))
}

export function sortTransactionsByRecency(transactions: Transaction[]) {
  return transactions.toSorted(
    (left, right) =>
      transactionDateKey(right.date).localeCompare(transactionDateKey(left.date)) ||
      Math.abs(right.amount) - Math.abs(left.amount),
  )
}

type CardBenefits = NonNullable<Insights['benefits']>
type BenefitWindow = CardBenefits['windows'][number]

/** Benefit matches waiting for a decision, leaving out hidden benefits'. */
export function benefitReviewItems(
  benefits: CardBenefits | undefined,
  hiddenBenefitIds: readonly string[] = [],
) {
  return (benefits?.review ?? []).filter(
    ({ benefitId }) => !benefitId || !hiddenBenefitIds.includes(benefitId),
  )
}

/** The windows to act on, in the order Rust gives: see `CardBenefits.useFirst`. */
export function benefitsToUseFirst(windows: BenefitWindow[], order: readonly string[]) {
  return order.flatMap((id) => windows.find((window) => window.id === id) ?? [])
}
