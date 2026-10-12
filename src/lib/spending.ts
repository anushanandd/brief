import { dayKey, dayMs, daysBetween } from './dates'
import { formatDay } from './format'
import { cents, sumCents } from './money'
import type {
  Account,
  FinanceSnapshot,
  Insights,
  SpendingPeriodJudgment,
  Transaction,
} from './schema'
import { isSpendingTransaction, splitParts } from './transaction-kind'

export type SpendingPeriod = 'statement' | 'week' | 0 | 1 | 3 | 12

/**
 * Spending's periods, each with the range Money's links carry (so every card in its strip opens
 * on the same period), its name, and its key.
 */
export const spendingPeriodOptions = [
  { range: 'statement', value: 'statement', label: 'Statement', key: 's', unit: 'statement' },
  { range: 'week', value: 'week', label: 'Week', key: 'w', unit: 'week' },
  { range: 'month', value: 1, label: 'Month', key: 'm', unit: 'month' },
  { range: 'quarter', value: 3, label: 'Quarter', key: 'q', unit: 'quarter' },
  { range: 'year', value: 12, label: 'Year', key: 'y', unit: 'year' },
  { range: 'all', value: 0, label: 'All time', key: 'a', unit: 'year' },
] as const satisfies ReadonlyArray<{
  range: string
  value: SpendingPeriod
  label: string
  key: string
  /** What the period is called in "This month" and "vs last month". */
  unit: string
}>
export type SpendingRange = (typeof spendingPeriodOptions)[number]['range']
export const spendingRanges = spendingPeriodOptions.map(({ range }) => range)
export const spendingPeriodForRange = (range: unknown): SpendingPeriod | undefined =>
  spendingPeriodOptions.find((option) => option.range === range)?.value
export const spendingRangeForPeriod = (period: SpendingPeriod): SpendingRange =>
  spendingPeriodOptions.find((option) => option.value === period)!.range
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
type RankedSpend = {
  name: string
  value: number
  previousValue: number
  change: number
  percent: number
}

const monthIndexes: Record<string, number> = {
  jan: 0,
  feb: 1,
  mar: 2,
  apr: 3,
  may: 4,
  jun: 5,
  jul: 6,
  aug: 7,
  sep: 8,
  oct: 9,
  nov: 10,
  dec: 11,
}

const isoDate = /^\d{4}-\d{2}-\d{2}/
const shiftDate = (date: Date, days: number) => new Date(date.getTime() + days * dayMs)
const monthStart = (date: Date, offset = 0) =>
  new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + offset, 1))

function calendarDate(referenceIso: string, timeZone?: string) {
  const parsed = new Date(referenceIso)
  if (Number.isNaN(parsed.getTime())) return new Date()
  const parts = new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    ...(timeZone ? { timeZone } : {}),
  })
    .formatToParts(parsed)
    .reduce<Record<string, number>>((values, part) => {
      if (part.type !== 'literal') values[part.type] = Number(part.value)
      return values
    }, {})
  return new Date(Date.UTC(parts.year, (parts.month ?? 1) - 1, parts.day ?? 1))
}

export function transactionDateKey(value: string, referenceIso: string) {
  if (isoDate.test(value)) return value.slice(0, 10)

  const shortDate = /^([a-z]{3,9})\s+(\d{1,2})$/i.exec(value.trim())
  const reference = calendarDate(referenceIso)
  const month = shortDate ? monthIndexes[shortDate[1].slice(0, 3).toLocaleLowerCase()] : undefined
  if (shortDate && month !== undefined && !Number.isNaN(reference.getTime())) {
    const year = reference.getUTCFullYear() - (month > reference.getUTCMonth() ? 1 : 0)
    return dayKey(new Date(Date.UTC(year, month, Number(shortDate[2]))))
  }

  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? '' : dayKey(parsed)
}

export function formatActivityDate(value: string, referenceIso: string) {
  const transactionDate = transactionDateKey(value, referenceIso)
  return transactionDate ? formatDay(transactionDate) : value
}

/** Lists name the year only when it isn't the snapshot's year. */
export function formatListDate(value: string, referenceIso: string) {
  const date = formatActivityDate(value, referenceIso)
  const year = transactionDateKey(referenceIso, referenceIso)?.slice(0, 4)
  return year && date.endsWith(`, ${year}`) ? date.slice(0, -`, ${year}`.length) : date
}

const isSpend = isSpendingTransaction

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
    transactions.filter(isSpend).flatMap(({ accountId }) => (accountId ? [accountId] : [])),
  )
  const options = accounts.filter(
    ({ id, currency }) =>
      id !== 'all' && (!currency || currency === 'USD') && (spenders.has(id) || id === card?.id),
  )
  const selected =
    requested === allSpendingAccounts
      ? undefined
      : (options.find(({ id }) => id === requested) ?? card)
  const ids = new Set(options.map(({ id }) => id))
  return {
    card,
    options,
    selected,
    transactions: transactions.filter(({ accountId }) =>
      selected ? accountId === selected.id : accountId != null && ids.has(accountId),
    ),
  }
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

export function weeklySpendingCategoryPerformance(
  transactions: Transaction[],
  referenceIso: string,
) {
  const end = calendarDate(referenceIso)
  const currentStart = shiftDate(end, -6)
  const previousEnd = shiftDate(currentStart, -1)
  const previousStart = shiftDate(previousEnd, -6)
  const totals = (start: Date, through: Date) => {
    const values = new Map<string, number>()
    for (const transaction of transactions.flatMap(splitParts)) {
      const day = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      if (
        transaction.pending ||
        !isSpend(transaction) ||
        day < dayKey(start) ||
        day > dayKey(through)
      )
        continue
      values.set(
        transaction.category,
        (values.get(transaction.category) ?? 0) + cents(Math.abs(transaction.amount)),
      )
    }
    return values
  }
  const current = totals(currentStart, end)
  const previous = totals(previousStart, previousEnd)
  return new Map(
    [...new Set([...current.keys(), ...previous.keys()])].map((category) => {
      const currentValue = current.get(category) ?? 0
      const previousValue = previous.get(category) ?? 0
      const performance = previousValue
        ? ((previousValue - currentValue) / previousValue) * 100
        : currentValue
          ? -100
          : null
      return [category, performance]
    }),
  )
}

export function buildMonthlySpendingHistory(
  transactions: Transaction[],
  referenceIso: string,
  count = 6,
) {
  const reference = calendarDate(referenceIso)
  const totals = new Map<string, number>()
  for (const transaction of transactions.filter(isSpend)) {
    const month = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso).slice(
      0,
      7,
    )
    if (month) totals.set(month, (totals.get(month) ?? 0) + cents(Math.abs(transaction.amount)))
  }
  return Array.from({ length: count }, (_, index) => {
    const month = dayKey(monthStart(reference, index - count + 1)).slice(0, 7)
    return { month, value: (totals.get(month) ?? 0) / 100 }
  })
}

function rank(
  current: Transaction[],
  previous: Transaction[],
  key: (transaction: Transaction) => string,
  total: number,
): RankedSpend[] {
  const sum = (transactions: Transaction[]) => {
    const values = new Map<string, number>()
    for (const transaction of transactions.filter(isSpend)) {
      const name = key(transaction)
      values.set(name, (values.get(name) ?? 0) + cents(Math.abs(transaction.amount)))
    }
    return values
  }
  const currentValues = sum(current)
  const previousValues = sum(previous)

  return [...currentValues]
    .map(([name, value]) => ({
      name,
      value: value / 100,
      previousValue: (previousValues.get(name) ?? 0) / 100,
      change: (value - (previousValues.get(name) ?? 0)) / 100,
      percent: total ? (value / 100 / total) * 100 : 0,
    }))
    .toSorted((left, right) => right.value - left.value)
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

export function spendingMonthReference(referenceIso: string, monthOffset: number) {
  if (monthOffset === 0) return referenceIso
  const reference = calendarDate(referenceIso)
  const date = dayKey(
    new Date(Date.UTC(reference.getUTCFullYear(), reference.getUTCMonth() + monthOffset + 1, 0)),
  )
  return `${date}T12:00:00Z`
}

/**
 * What Spending's headline shows for a period: the statement balance when the period is a real
 * statement, otherwise the period's spending, with its change from the same point last period.
 */
export function spendingHeadline(
  data: Pick<FinanceSnapshot, 'accounts' | 'transactions' | 'updatedAt' | 'insights'>,
  spendingAccountId: string,
  period: SpendingPeriod,
  requested?: string,
) {
  const scope = spendingScope(data.accounts, data.transactions, spendingAccountId, requested)
  const account = scope.selected
  const statement =
    account && data.insights?.benefits?.accountId === account.id
      ? (data.insights.statement ?? undefined)
      : undefined
  const view = buildSpendingView(
    scope.transactions,
    data.updatedAt,
    period,
    spendingPeriodReference(statement, data.updatedAt, 0, period),
    spendingJudgments(data.insights, account?.id ?? allSpendingAccounts),
    statement,
  )
  return {
    hasScope: scope.options.length > 0,
    value:
      view.periodBasis === 'statement' && period === 'statement'
        ? view.statementBalance
        : view.total,
    change: period ? view.total - view.previousTotal : null,
  }
}

function calendarPeriodStart(date: Date, period: 1 | 3 | 12) {
  const month =
    period === 12 ? 0 : period === 3 ? Math.floor(date.getUTCMonth() / 3) * 3 : date.getUTCMonth()
  return new Date(Date.UTC(date.getUTCFullYear(), month, 1))
}

/** Native statement evidence for the saved spending card; see `insights.statement`. */
export type StatementEvidence = {
  boundaries: string[]
  autopays: Array<{ date: string; amount: number }>
}

export function spendingPeriodReference(
  statement: StatementEvidence | undefined,
  referenceIso: string,
  periodOffset: number,
  period: SpendingPeriod,
) {
  if (periodOffset === 0 || period === 0) return referenceIso
  if (period === 'week')
    return `${dayKey(shiftDate(calendarDate(referenceIso), periodOffset * 7))}T12:00:00Z`
  if (typeof period === 'number') {
    const start = calendarPeriodStart(calendarDate(referenceIso), period)
    const end = new Date(
      Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + (periodOffset + 1) * period, 0),
    )
    return `${dayKey(end)}T12:00:00Z`
  }
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  const boundaries = (statement?.boundaries ?? []).filter((date) => date <= referenceDay)
  if (!boundaries.length) return spendingMonthReference(referenceIso, periodOffset)
  const endsOnBoundary = boundaries.at(-1) === referenceDay
  const latestCompletedIndex = boundaries.length - 1 - (endsOnBoundary ? 1 : 0)
  const selected = boundaries[latestCompletedIndex + periodOffset + 1]
  return selected ? `${selected}T12:00:00Z` : spendingMonthReference(referenceIso, periodOffset)
}

function statementPeriodStart(boundaries: string[], end: Date) {
  const endDay = dayKey(end)
  const priorBoundaries = boundaries.filter((date) => date <= endDay)
  const endsOnBoundary = priorBoundaries.at(-1) === endDay
  const index = priorBoundaries.length - 1 - (endsOnBoundary ? 1 : 0)
  const boundary = priorBoundaries[index]
  return boundary ? shiftDate(new Date(`${boundary}T00:00:00Z`), 1) : null
}

/** Rust's judgments about one scope's periods; see `insights.spending`. */
export function spendingJudgments(
  insights: Pick<Insights, 'spending'> | undefined,
  scope: string,
): SpendingPeriodJudgment[] {
  return insights?.spending.find((entry) => entry.scope === scope)?.periods ?? []
}

/** Whether a day key falls on or between two dates' days. */
const withinDays = (date: string, from: Date, to: Date) =>
  date >= dayKey(from) && date <= dayKey(to)

/**
 * A period's spending, beside the same point in the period before. React sums the period's
 * activity; Rust's judgments for the scope supply what a statement opened at, what is usual by
 * this point, and where the current period is heading. `focus` names the one category the
 * transactions were narrowed to, so its own projection applies.
 */
export function buildSpendingView(
  transactions: Transaction[],
  referenceIso: string,
  period: SpendingPeriod,
  periodReferenceIso = referenceIso,
  judgments: SpendingPeriodJudgment[] = [],
  statement?: StatementEvidence,
  focus?: string,
) {
  const end = calendarDate(periodReferenceIso)
  const parsed = transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
    return date ? [{ transaction, date }] : []
  })
  const statementBoundaries = period === 'statement' ? (statement?.boundaries ?? []) : []
  const statementStart = statementBoundaries.length
    ? statementPeriodStart(statementBoundaries, end)
    : null
  const basis: SpendingPeriodBasis = statementStart ? 'statement' : 'calendar'
  const calendarPeriod = typeof period === 'number' && period !== 0 ? period : null
  const start =
    period === 'week'
      ? shiftDate(end, -6)
      : calendarPeriod
        ? calendarPeriodStart(end, calendarPeriod)
        : statementStart
          ? statementStart
          : period === 'statement'
            ? monthStart(end)
            : new Date(
                `${
                  parsed
                    .map(({ date }) => date)
                    .filter((date) => date <= dayKey(end))
                    .toSorted()[0] ?? dayKey(end)
                }T00:00:00Z`,
              )
  // The start of the comparable period before one that starts on `from`.
  const priorStart = (from: Date) =>
    period === 'week'
      ? shiftDate(from, -7)
      : calendarPeriod
        ? monthStart(from, -calendarPeriod)
        : period === 'statement'
          ? basis === 'statement'
            ? (statementPeriodStart(statementBoundaries, shiftDate(from, -1)) ??
              monthStart(from, -1))
            : monthStart(from, -1)
          : from
  const previousStart = priorStart(start)
  const kind: SpendingPeriodJudgment['kind'] | undefined =
    basis === 'statement'
      ? 'statement'
      : period === 'week'
        ? 'week'
        : period === 3
          ? 'quarter'
          : period === 12
            ? 'year'
            : period === 0
              ? undefined
              : 'month'
  const judged = kind
    ? judgments.find((entry) => entry.kind === kind && entry.start === dayKey(start))
    : undefined
  const elapsedDays = Math.floor((end.getTime() - start.getTime()) / dayMs) + 1
  // The same number of days into a prior period, never running into the next one.
  const samePointEnd = (from: Date, next: Date) =>
    new Date(Math.min(shiftDate(from, elapsedDays - 1).getTime(), next.getTime() - dayMs))
  const previousEnd = period ? samePointEnd(previousStart, start) : shiftDate(start, -1)
  const currentEntries = parsed.filter(({ date }) => withinDays(date, start, end))
  const current = currentEntries.map(({ transaction }) => transaction)
  const previous = parsed
    .filter(({ date }) => withinDays(date, previousStart, previousEnd))
    .map(({ transaction }) => transaction)
  const expenses = current.filter(isSpend)
  const previousExpenses = previous.filter(isSpend)
  const total =
    expenses.reduce((sum, transaction) => sum + cents(Math.abs(transaction.amount)), 0) / 100
  const previousTotal =
    previousExpenses.reduce((sum, transaction) => sum + cents(Math.abs(transaction.amount)), 0) /
    100
  const pendingTotal =
    expenses
      .filter(({ pending }) => pending)
      .reduce((sum, transaction) => sum + cents(Math.abs(transaction.amount)), 0) / 100
  const activityAmount = (transaction: Transaction) =>
    isSpend(transaction)
      ? cents(Math.abs(transaction.amount))
      : transaction.amount > 0
        ? -cents(transaction.amount)
        : 0
  const activityByDate = (items: typeof parsed) => {
    const totals = new Map<string, number>()
    for (const entry of items) {
      const amount = activityAmount(entry.transaction)
      if (!amount) continue
      totals.set(entry.date, (totals.get(entry.date) ?? 0) + amount)
    }
    return totals
  }
  const startingBalance = basis === 'statement' ? (judged?.opening ?? 0) : 0
  const currentByDate = activityByDate(currentEntries)
  const previousByDate = activityByDate(
    parsed.filter(({ date }) => withinDays(date, previousStart, previousEnd)),
  )
  let currentCumulative = cents(startingBalance)
  let previousCumulative = 0
  const trend = Array.from({ length: elapsedDays }, (_, index) => {
    const currentDate = shiftDate(start, index)
    const currentDateKey = dayKey(currentDate)
    const priorDate = shiftDate(previousStart, index)
    currentCumulative += currentByDate.get(currentDateKey) ?? 0
    previousCumulative += previousByDate.get(dayKey(priorDate)) ?? 0
    return {
      date: currentDateKey,
      current: currentCumulative / 100,
      previous: previousCumulative / 100,
    }
  })
  const dailyAverage = elapsedDays ? total / elapsedDays : 0
  const closingBoundary =
    basis === 'statement' ? statementBoundaries.find((date) => date >= dayKey(end)) : undefined
  // The whole selected period, so the chart can show how far into it today is.
  const periodEnd =
    period === 'week' || period === 0
      ? end
      : calendarPeriod
        ? new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + calendarPeriod, 0))
        : closingBoundary
          ? new Date(`${closingBoundary}T00:00:00Z`)
          : basis === 'statement'
            ? end
            : new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + 1, 0))
  const cumulativeSpend = (from: Date, to: Date) => {
    const byDate = new Map<string, number>()
    for (const { transaction, date } of parsed) {
      if (!isSpend(transaction) || !withinDays(date, from, to)) continue
      byDate.set(date, (byDate.get(date) ?? 0) + cents(Math.abs(transaction.amount)))
    }
    const days = Math.floor((to.getTime() - from.getTime()) / dayMs) + 1
    let running = 0
    return Array.from({ length: Math.max(0, days) }, (_, index) => {
      running += byDate.get(dayKey(shiftDate(from, index))) ?? 0
      return running / 100
    })
  }
  // Each day of observed spending, so the chart can mark it and list what was bought.
  const spendDays = new Map<
    string,
    { index: number; date: string; activities: Array<{ id: string; value: number }> }
  >()
  for (const { transaction, date } of currentEntries) {
    if (!isSpend(transaction)) continue
    const day = spendDays.get(date) ?? {
      index: Math.floor((Date.parse(`${date}T00:00:00Z`) - start.getTime()) / dayMs),
      date,
      activities: [],
    }
    day.activities.push({ id: `spending:${transaction.id}`, value: Math.abs(transaction.amount) })
    spendDays.set(date, day)
  }
  return {
    start: dayKey(start),
    end: dayKey(end),
    periodBasis: basis,
    startingBalance,
    statementBalance: currentCumulative / 100,
    transactions: sortTransactionsByRecency(current, referenceIso),
    total,
    previousTotal,
    pendingTotal,
    percentChange: previousTotal ? ((total - previousTotal) / previousTotal) * 100 : null,
    dailyAverage,
    pace: {
      start: dayKey(start),
      end: dayKey(periodEnd),
      current: cumulativeSpend(start, end),
      previous: period === 0 ? [] : cumulativeSpend(previousStart, shiftDate(start, -1)),
      days: [...spendDays.values()]
        .map((day) => ({
          ...day,
          activities: day.activities.toSorted((left, right) => right.value - left.value),
        }))
        .toSorted((left, right) => left.index - right.index),
      projected: (focus ? judged?.projected?.categories[focus] : judged?.projected?.total) ?? null,
    },
    biggest: expenses.toSorted((left, right) => Math.abs(right.amount) - Math.abs(left.amount))[0],
    categories: rank(
      current.flatMap(splitParts),
      previous.flatMap(splitParts),
      ({ category }) => category,
      total,
    ).map((category) => ({
      ...category,
      typical: judged?.typical ? (judged.typical[category.name] ?? 0) : null,
    })),
    merchants: rank(current, previous, ({ merchant }) => merchant, total),
    trend,
  }
}

export function buildSpendingBars(
  transactions: Transaction[],
  referenceIso: string,
  start: string,
  end: string,
  period: SpendingPeriod,
) {
  const totals = new Map<
    string,
    Map<string, { value: number; activities: Array<{ id: string; value: number }> }>
  >()
  transactions
    .filter((transaction) => isSpend(transaction) || transaction.classification.credit)
    .flatMap(splitParts)
    .forEach((transaction) => {
      const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      const name = transaction.classification.credit ? 'Credits' : transaction.category
      const categories = totals.get(date) ?? new Map()
      const category = categories.get(name) ?? { value: 0, activities: [] }
      const value = cents(Math.abs(transaction.amount)) * (name === 'Credits' ? -1 : 1)
      category.value += value
      category.activities.push({
        id: `spending:${transaction.id}`,
        value: Math.abs(transaction.amount),
      })
      categories.set(name, category)
      totals.set(date, categories)
    })
  const startDate = new Date(`${start}T00:00:00Z`)
  const days = daysBetween(start, end) + 1
  const bars: SpendingBar[] = []
  Array.from({ length: Math.max(1, days) }, (_, index) =>
    dayKey(shiftDate(startDate, index)),
  ).forEach((date, index) => {
    const categories = [...(totals.get(date) ?? new Map()).entries()]
      .map(([name, category]) => ({
        name,
        value: category.value / 100,
        activities: category.activities,
      }))
      .toSorted((left, right) => left.name.localeCompare(right.name))
    const value = categories.reduce((sum, category) => sum + category.value, 0)
    const bar = bars.at(-1)
    const sameBucket =
      period === 3
        ? index % 7 !== 0
        : (period === 12 || period === 0) && bar?.from.slice(0, 7) === date.slice(0, 7)
    if (bar && sameBucket) {
      bar.to = date
      bar.value = (Math.round(bar.value * 100) + Math.round(value * 100)) / 100
      categories.forEach((category) => {
        const existing = bar.categories.find(({ name }) => name === category.name)
        if (existing) {
          existing.value =
            (Math.round(existing.value * 100) + Math.round(category.value * 100)) / 100
          existing.activities.push(...category.activities)
        } else bar.categories.push(category)
      })
      bar.categories.sort((left, right) => left.name.localeCompare(right.name))
    } else {
      bars.push({ from: date, to: date, value, categories })
    }
  })
  return bars
}

export function sortTransactionsByRecency(transactions: Transaction[], referenceIso: string) {
  return transactions.toSorted(
    (left, right) =>
      transactionDateKey(right.date, referenceIso).localeCompare(
        transactionDateKey(left.date, referenceIso),
      ) || Math.abs(right.amount) - Math.abs(left.amount),
  )
}

type CardBenefits = NonNullable<Insights['benefits']>
type BenefitWindow = CardBenefits['windows'][number]
type BenefitOutcome = CardBenefits['outcomes'][number]
type AmountRange = { low: number; high: number }

/**
 * Posted credits net of reversals in a range, and the allowance left unused in windows that
 * ended in it. Rust classifies the activity and gives each window its verdict; this only sums
 * cents.
 */
export function platinumCreditSummary(
  benefits: CardBenefits | undefined,
  start: string,
  end: string,
  hiddenBenefitIds: readonly string[] = [],
) {
  const visible = ({ benefitId }: { benefitId: string }) => !hiddenBenefitIds.includes(benefitId)
  const earned = sumCents(
    (benefits?.activity ?? [])
      .filter(
        (item) =>
          visible(item) &&
          item.date >= start &&
          item.date <= end &&
          !item.pending &&
          (item.kind === 'credit' || item.kind === 'reversal'),
      )
      .map(({ amount }) => amount),
  )
  const ended = (benefits?.outcomes ?? []).filter(
    (outcome) => visible(outcome) && outcome.windowEnd >= start && outcome.windowEnd <= end,
  )
  const judged = (verdict: BenefitOutcome['verdict']) =>
    ended.filter((outcome) => outcome.verdict === verdict)
  return {
    earned,
    missed: {
      low: sumCents(judged('missed').map(({ unused }) => unused ?? 0)),
      high: sumCents(judged('missed').map(({ unusedHigh }) => unusedHigh ?? 0)),
    } satisfies AmountRange,
    unclaimed: sumCents(judged('unclaimed').map(({ unused }) => unused ?? 0)),
    /** Windows whose credits may still post. */
    settling: judged('settling').length,
    /** Windows the card history can't judge. */
    unknown: judged('unknown').length,
  }
}

export type PlatinumCreditSummary = ReturnType<typeof platinumCreditSummary>

/** Totals across the current calendar benefit windows, as computed natively per window. */
export function benefitWindowSummary(windows: BenefitWindow[]) {
  const current = windows.filter(({ cadence }) => cadence !== 'renewal' && cadence !== 'purchase')
  const tracked = current.flatMap(({ remainingRange }) => (remainingRange ? [remainingRange] : []))
  const next = current
    .filter(({ remainingRange }) => (remainingRange?.high ?? 0) > 0)
    .toSorted((a, b) => a.windowEnd.localeCompare(b.windowEnd))[0]
  return {
    captured: sumCents(current.map(({ creditedAmount }) => creditedAmount)),
    remaining: {
      low: sumCents(tracked.map(({ low }) => low)),
      high: sumCents(tracked.map(({ high }) => high)),
    } satisfies AmountRange,
    awaiting: sumCents(current.map(({ awaitingCreditAmount }) => awaitingCreditAmount)),
    nextReset: next ? { label: next.reset, daysRemaining: next.daysRemaining } : null,
  }
}

/** The windows to act on, in the order Rust gives: see `CardBenefits.useFirst`. */
export function benefitsToUseFirst(windows: BenefitWindow[], order: readonly string[]) {
  return order.flatMap((id) => windows.find((window) => window.id === id) ?? [])
}
