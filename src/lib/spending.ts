import type { Account, Transaction } from './schema'

export type SpendingPeriod = 0 | 1 | 3 | 12
export type SpendingPeriodBasis = 'calendar' | 'statement'
const spendingPeriodKeys: Record<string, SpendingPeriod> = { m: 1, q: 3, y: 12, a: 0 }
type BenefitCadence = 'monthly' | 'quarterly' | 'semiannual' | 'annual'
const benefitRulesVersion = '2025-01'

type RankedSpend = {
  name: string
  value: number
  previousValue: number
  change: number
  percent: number
}

const platinumBenefits: Array<{
  id: string
  name: string
  limit: string
  cap: number
  cadence: BenefitCadence
  matches: RegExp
  effectiveFrom: string
}> = [
  {
    id: 'digital-entertainment',
    name: 'Digital entertainment',
    limit: '$25 / month',
    cap: 25,
    cadence: 'monthly',
    matches:
      /digital entertainment|disney|hulu|espn|new york times|paramount|peacock|wall street journal|youtube/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'uber-cash',
    name: 'Uber Cash',
    limit: '$15 monthly · +$20 Dec',
    cap: 15,
    cadence: 'monthly',
    matches: /uber cash|^uber(?:\s|$)/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'uber-one',
    name: 'Uber One',
    limit: '$120 / year',
    cap: 120,
    cadence: 'annual',
    matches: /uber one/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'walmart-plus',
    name: 'Walmart+',
    limit: '$12.95 / month',
    cap: 12.95,
    cadence: 'monthly',
    matches: /walmart/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'resy',
    name: 'Resy',
    limit: '$100 / quarter',
    cap: 100,
    cadence: 'quarterly',
    matches: /resy/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'lululemon',
    name: 'lululemon',
    limit: '$75 / quarter',
    cap: 75,
    cadence: 'quarterly',
    matches: /lululemon/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'hotel',
    name: 'Hotel credit',
    limit: '$300 / half-year',
    cap: 300,
    cadence: 'semiannual',
    matches:
      /hotel credit|fine hotels|\bfhr\b|the hotel collection|amex travel|american express travel/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'airline-fee',
    name: 'Airline fee',
    limit: '$200 / year',
    cap: 200,
    cadence: 'annual',
    matches:
      /airline fee (?:credit|reimbursement)|checked bag|baggage fee|inflight|in-flight|seat fee|lounge fee/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'clear',
    name: 'CLEAR+',
    limit: '$219 / year',
    cap: 219,
    cadence: 'annual',
    matches: /\bclear\b/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'oura',
    name: 'Oura Ring',
    limit: '$200 / year',
    cap: 200,
    cadence: 'annual',
    matches: /\boura\b/i,
    effectiveFrom: '2025-01-01',
  },
  {
    id: 'equinox',
    name: 'Equinox',
    limit: '$300 / year',
    cap: 300,
    cadence: 'annual',
    matches: /equinox/i,
    effectiveFrom: '2025-01-01',
  },
]

export const platinumBenefitOptions = platinumBenefits.map(({ id, name }) => ({ id, name }))

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
const dayMs = 24 * 60 * 60 * 1_000
const shortDateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})
const activityDateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  timeZone: 'UTC',
})
const dateKey = (date: Date) => date.toISOString().slice(0, 10)
const addDays = (date: Date, days: number) => new Date(date.getTime() + days * dayMs)
const monthStart = (date: Date, offset = 0) =>
  new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + offset, 1))
const amexAutopayPattern = /\bautopay payment(?: received)?\s*-?\s*thank you\b/i
const amexStatementCloseLeadDays = 15

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

function benefitWindow(date: Date, cadence: BenefitCadence) {
  const month = date.getUTCMonth()
  const startMonth =
    cadence === 'monthly'
      ? month
      : cadence === 'quarterly'
        ? Math.floor(month / 3) * 3
        : cadence === 'semiannual'
          ? Math.floor(month / 6) * 6
          : 0
  const duration =
    cadence === 'monthly' ? 1 : cadence === 'quarterly' ? 3 : cadence === 'semiannual' ? 6 : 12
  return {
    start: new Date(Date.UTC(date.getUTCFullYear(), startMonth, 1)),
    end: new Date(Date.UTC(date.getUTCFullYear(), startMonth + duration, 0)),
  }
}

export function transactionDateKey(value: string, referenceIso: string) {
  if (isoDate.test(value)) return value.slice(0, 10)

  const shortDate = /^([a-z]{3,9})\s+(\d{1,2})$/i.exec(value.trim())
  const reference = calendarDate(referenceIso)
  const month = shortDate ? monthIndexes[shortDate[1].slice(0, 3).toLocaleLowerCase()] : undefined
  if (shortDate && month !== undefined && !Number.isNaN(reference.getTime())) {
    const year = reference.getUTCFullYear() - (month > reference.getUTCMonth() ? 1 : 0)
    return dateKey(new Date(Date.UTC(year, month, Number(shortDate[2]))))
  }

  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? '' : dateKey(parsed)
}

export function formatTransactionDate(value: string, referenceIso: string) {
  const transactionDate = transactionDateKey(value, referenceIso)
  const referenceDate = transactionDateKey(referenceIso, referenceIso)
  if (!transactionDate || !referenceDate) return value
  if (transactionDate === referenceDate) return 'Today'
  if (transactionDate === dateKey(addDays(new Date(`${referenceDate}T00:00:00Z`), -1))) {
    return 'Yesterday'
  }
  return value
}

export function formatActivityDate(value: string, referenceIso: string) {
  const transactionDate = transactionDateKey(value, referenceIso)
  return transactionDate
    ? activityDateFormatter.format(new Date(`${transactionDate}T00:00:00Z`))
    : value
}

export const isSpendingTransaction = (transaction: Transaction) =>
  transaction.amount < 0 &&
  !/income|transfer|payment/i.test(`${transaction.category} ${transaction.merchant}`)

const isSpend = isSpendingTransaction
const minorUnits = (value: number) => Math.round(value * 100)

export function resolveSpendingAccount(accounts: Account[], preferredId: string) {
  return accounts.find(({ id, type }) => type === 'credit' && id === preferredId)
}

export const spendingCategoryColor = (name: string) => {
  const category = name.toLocaleLowerCase()
  if (/housing|rent|mortgage/.test(category)) return 'var(--spending-housing)'
  if (/travel|flight|hotel/.test(category)) return 'var(--spending-travel)'
  if (/food|dining|grocer/.test(category)) return 'var(--spending-food)'
  if (/shopping|retail|merchandise/.test(category)) return 'var(--spending-shopping)'
  if (/transport|gas|ride/.test(category)) return 'var(--spending-transport)'
  if (/entertainment|subscription/.test(category)) return 'var(--spending-entertainment)'
  if (/utilit|bill/.test(category)) return 'var(--spending-utilities)'
  return 'var(--spending-other)'
}

export function buildMonthlySpending(transactions: Transaction[], referenceIso: string) {
  const view = buildSpendingView(transactions, referenceIso, 1)
  return {
    monthTotal: Math.round(view.total * 100) / 100,
    categories: view.categories.map(({ name, value, percent }) => ({
      name,
      value: Math.round(value * 100) / 100,
      percent: Math.round(percent * 100) / 100,
      color: spendingCategoryColor(name),
    })),
  }
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
    if (month)
      totals.set(month, (totals.get(month) ?? 0) + minorUnits(Math.abs(transaction.amount)))
  }
  return Array.from({ length: count }, (_, index) => {
    const month = dateKey(monthStart(reference, index - count + 1)).slice(0, 7)
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
      values.set(name, (values.get(name) ?? 0) + minorUnits(Math.abs(transaction.amount)))
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

export function spendingPeriodForKey(key: string) {
  return spendingPeriodKeys[key.toLocaleLowerCase()]
}

export function spendingPeriodLabel(
  period: SpendingPeriod,
  monthOffset: number,
  selectedPeriod: string,
  basis: SpendingPeriodBasis = 'calendar',
) {
  if (basis === 'statement' && period !== 0) {
    if (monthOffset === 0) {
      if (period === 1) return 'Current statement'
      return `${period} statements`
    }
    if (period === 1) return `Statement ending ${selectedPeriod}`
    return `${period} statements through ${selectedPeriod}`
  }
  if (monthOffset === 0) {
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
  const date = dateKey(
    new Date(Date.UTC(reference.getUTCFullYear(), reference.getUTCMonth() + monthOffset + 1, 0)),
  )
  return `${date}T12:00:00Z`
}

function recognizedAmexAutopays(transactions: Transaction[], referenceIso: string) {
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  return transactions
    .flatMap((transaction) => {
      const text = `${transaction.merchant} ${transaction.description ?? ''}`
      const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      return transaction.amount > 0 && !transaction.pending && amexAutopayPattern.test(text) && date
        ? [{ date, amount: transaction.amount }]
        : []
    })
    .filter(({ date }) => date <= referenceDay)
    .toSorted((left, right) => left.date.localeCompare(right.date))
    .filter((autopay, index, autopays) => index === 0 || autopay.date !== autopays[index - 1].date)
}

function autopayStatementBoundaries(transactions: Transaction[], referenceIso: string) {
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  const autopayDates = recognizedAmexAutopays(transactions, referenceIso).map(({ date }) => date)

  if (autopayDates.length < 2) return []

  const intervals = autopayDates.slice(1).map((date, index) => {
    const current = new Date(`${date}T00:00:00Z`)
    const previous = new Date(`${autopayDates[index]}T00:00:00Z`)
    const months =
      (current.getUTCFullYear() - previous.getUTCFullYear()) * 12 +
      current.getUTCMonth() -
      previous.getUTCMonth()
    return months > 0 && months <= 3 ? (current.getTime() - previous.getTime()) / dayMs / months : 0
  })
  if (intervals.filter((days) => days >= 24 && days <= 38).length < intervals.length * 0.6) {
    return []
  }

  const observedClosingDates = autopayDates.map((date) =>
    dateKey(addDays(new Date(`${date}T00:00:00Z`), -amexStatementCloseLeadDays)),
  )
  const typicalDay = observedClosingDates
    .map((date) => Number(date.slice(8, 10)))
    .toSorted((left, right) => left - right)[Math.floor(observedClosingDates.length / 2)]
  const observedByMonth = new Map(observedClosingDates.map((date) => [date.slice(0, 7), date]))
  const parsedDates = transactions
    .map((transaction) =>
      transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso),
    )
    .filter(Boolean)
    .toSorted()
  const first = new Date(`${parsedDates[0] ?? observedClosingDates[0]}T00:00:00Z`)
  const reference = new Date(`${referenceDay}T00:00:00Z`)
  const comparisonCoverage = monthStart(reference, -25)
  const firstMonth = monthStart(first < comparisonCoverage ? first : comparisonCoverage, -1)
  const lastMonth = monthStart(reference, 1)
  const boundaries: string[] = []

  for (let month = firstMonth; month <= lastMonth; month = monthStart(month, 1)) {
    const monthKey = dateKey(month).slice(0, 7)
    const lastDay = new Date(
      Date.UTC(month.getUTCFullYear(), month.getUTCMonth() + 1, 0),
    ).getUTCDate()
    boundaries.push(
      observedByMonth.get(monthKey) ??
        dateKey(
          new Date(
            Date.UTC(month.getUTCFullYear(), month.getUTCMonth(), Math.min(typicalDay, lastDay)),
          ),
        ),
    )
  }
  return boundaries
}

export function spendingPeriodReference(
  transactions: Transaction[],
  referenceIso: string,
  monthOffset: number,
) {
  if (monthOffset === 0) return referenceIso
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  const boundaries = autopayStatementBoundaries(transactions, referenceIso).filter(
    (date) => date <= referenceDay,
  )
  if (!boundaries.length) return spendingMonthReference(referenceIso, monthOffset)
  const endsOnBoundary = boundaries.at(-1) === referenceDay
  const latestCompletedIndex = boundaries.length - 1 - (endsOnBoundary ? 1 : 0)
  const selected = boundaries[latestCompletedIndex + monthOffset + 1]
  return selected ? `${selected}T12:00:00Z` : spendingMonthReference(referenceIso, monthOffset)
}

function statementPeriodStart(boundaries: string[], end: Date, period: Exclude<SpendingPeriod, 0>) {
  const endDay = dateKey(end)
  const priorBoundaries = boundaries.filter((date) => date <= endDay)
  const endsOnBoundary = priorBoundaries.at(-1) === endDay
  const index = priorBoundaries.length - period - (endsOnBoundary ? 1 : 0)
  const boundary = priorBoundaries[index]
  return boundary ? addDays(new Date(`${boundary}T00:00:00Z`), 1) : null
}

export function buildSpendingView(
  transactions: Transaction[],
  referenceIso: string,
  period: SpendingPeriod,
  periodReferenceIso = referenceIso,
  preferredBasis: SpendingPeriodBasis = 'calendar',
  currentAccountBalance?: number,
) {
  const end = calendarDate(periodReferenceIso)
  const parsed = transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
    return date ? [{ transaction, date }] : []
  })
  const statementBoundaries =
    preferredBasis === 'statement' ? autopayStatementBoundaries(transactions, referenceIso) : []
  const statementStart =
    period && statementBoundaries.length
      ? statementPeriodStart(statementBoundaries, end, period)
      : null
  const basis: SpendingPeriodBasis = statementStart ? 'statement' : 'calendar'
  const start = period
    ? (statementStart ?? monthStart(end, 1 - period))
    : new Date(
        `${
          parsed
            .map(({ date }) => date)
            .filter((date) => date <= dateKey(end))
            .toSorted()[0] ?? dateKey(end)
        }T00:00:00Z`,
      )
  const previousWindowEnd = addDays(start, -1)
  const previousStart = period
    ? ((basis === 'statement'
        ? statementPeriodStart(statementBoundaries, previousWindowEnd, period)
        : null) ?? monthStart(start, -period))
    : start
  const elapsedDays = Math.floor((end.getTime() - start.getTime()) / dayMs) + 1
  const previousEnd = period
    ? new Date(Math.min(addDays(previousStart, elapsedDays - 1).getTime(), start.getTime() - dayMs))
    : addDays(start, -1)
  const within = (date: string, from: Date, to: Date) =>
    date >= dateKey(from) && date <= dateKey(to)
  const currentEntries = parsed.filter(({ date }) => within(date, start, end))
  const current = currentEntries.map(({ transaction }) => transaction)
  const previous = parsed
    .filter(({ date }) => within(date, previousStart, previousEnd))
    .map(({ transaction }) => transaction)
  const expenses = current.filter(isSpend)
  const previousExpenses = previous.filter(isSpend)
  const total =
    expenses.reduce((sum, transaction) => sum + minorUnits(Math.abs(transaction.amount)), 0) / 100
  const previousTotal =
    previousExpenses.reduce(
      (sum, transaction) => sum + minorUnits(Math.abs(transaction.amount)),
      0,
    ) / 100
  const pendingTotal =
    expenses
      .filter(({ pending }) => pending)
      .reduce((sum, transaction) => sum + minorUnits(Math.abs(transaction.amount)), 0) / 100
  const activityAmount = (transaction: Transaction) =>
    isSpend(transaction)
      ? minorUnits(Math.abs(transaction.amount))
      : transaction.amount > 0
        ? -minorUnits(transaction.amount)
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
  const currentNetActivity =
    currentEntries.reduce((sum, { transaction }) => sum + activityAmount(transaction), 0) / 100
  const inferredOpeningBalance = recognizedAmexAutopays(transactions, referenceIso).find(
    ({ date }) => within(date, start, end),
  )?.amount
  const startingBalance =
    basis === 'statement'
      ? currentAccountBalance == null
        ? (inferredOpeningBalance ?? 0)
        : currentAccountBalance - currentNetActivity
      : 0
  const currentActivityByDate = new Map<string, typeof parsed>()
  for (const entry of currentEntries) {
    if (!activityAmount(entry.transaction)) continue
    const entries = currentActivityByDate.get(entry.date) ?? []
    entries.push(entry)
    currentActivityByDate.set(entry.date, entries)
  }
  const previousByDate = activityByDate(
    parsed.filter(({ date }) => within(date, previousStart, previousEnd)),
  )
  let currentCumulative = minorUnits(startingBalance)
  let previousCumulative = 0
  const activityMarkers: Array<{
    id: string
    date: string
    sequence: number
    count: number
    value: number
    direction: 'expense' | 'credit'
    merchant: string
    amount: number
  }> = []
  const trend = Array.from({ length: elapsedDays }, (_, index) => {
    const currentDate = addDays(start, index)
    const currentDateKey = dateKey(currentDate)
    const priorDate = addDays(previousStart, index)
    const dailyActivity = (currentActivityByDate.get(currentDateKey) ?? []).toSorted(
      (left, right) => left.transaction.id.localeCompare(right.transaction.id),
    )
    for (const [sequence, entry] of dailyActivity.entries()) {
      const amount = activityAmount(entry.transaction)
      currentCumulative += amount
      activityMarkers.push({
        id: entry.transaction.id,
        date: currentDateKey,
        sequence,
        count: dailyActivity.length,
        value: currentCumulative / 100,
        direction: amount > 0 ? 'expense' : 'credit',
        merchant: entry.transaction.merchant,
        amount: Math.abs(entry.transaction.amount),
      })
    }
    previousCumulative += previousByDate.get(dateKey(priorDate)) ?? 0
    return {
      date: currentDateKey,
      current: currentCumulative / 100,
      previous: previousCumulative / 100,
    }
  })
  return {
    start: dateKey(start),
    end: dateKey(end),
    periodBasis: basis,
    startingBalance,
    statementBalance: currentCumulative / 100,
    transactions: sortTransactionsByRecency(current, referenceIso),
    total,
    previousTotal,
    pendingTotal,
    percentChange: previousTotal ? ((total - previousTotal) / previousTotal) * 100 : null,
    dailyAverage: elapsedDays ? total / elapsedDays : 0,
    biggest: expenses.toSorted((left, right) => Math.abs(right.amount) - Math.abs(left.amount))[0],
    categories: rank(current, previous, ({ category }) => category, total),
    merchants: rank(current, previous, ({ merchant }) => merchant, total),
    trend,
    activityMarkers,
  }
}

export function sortTransactionsByRecency(transactions: Transaction[], referenceIso: string) {
  return transactions.toSorted(
    (left, right) =>
      transactionDateKey(right.date, referenceIso).localeCompare(
        transactionDateKey(left.date, referenceIso),
      ) || Math.abs(right.amount) - Math.abs(left.amount),
  )
}

const subscriptionCadences = [
  { label: 'Monthly', days: 30, tolerance: 5, minimumIntervals: 2 },
  { label: 'Quarterly', days: 91, tolerance: 11, minimumIntervals: 2 },
  { label: 'Annual', days: 365, tolerance: 35, minimumIntervals: 1 },
] as const

export function identifySubscriptions(transactions: Transaction[], referenceIso: string) {
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  const merchants = new Map<string, Array<{ transaction: Transaction; date: string }>>()

  for (const transaction of transactions) {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
    const merchant = transaction.merchant
      .toLocaleLowerCase()
      .replaceAll(/[^a-z0-9]+/g, ' ')
      .trim()
    if (!merchant || !date || date > referenceDay || transaction.pending || !isSpend(transaction)) {
      continue
    }
    const entries = merchants.get(merchant) ?? []
    entries.push({ transaction, date })
    merchants.set(merchant, entries)
  }

  return [...merchants.values()]
    .flatMap((entries) => {
      const amounts = entries
        .map(({ transaction }) => Math.abs(transaction.amount))
        .toSorted((left, right) => left - right)
      const median = amounts[Math.floor(amounts.length / 2)] ?? 0
      const amountTolerance = Math.max(1, median * 0.1)
      const consistent = entries
        .filter(
          ({ transaction }) => Math.abs(Math.abs(transaction.amount) - median) <= amountTolerance,
        )
        .toSorted((left, right) => left.date.localeCompare(right.date))
      const intervals = consistent
        .slice(1)
        .map(
          ({ date }, index) =>
            (Date.parse(`${date}T00:00:00Z`) - Date.parse(`${consistent[index].date}T00:00:00Z`)) /
            dayMs,
        )
      const cadence = subscriptionCadences
        .map((option) => ({
          ...option,
          matches: intervals.filter((days) => Math.abs(days - option.days) <= option.tolerance)
            .length,
        }))
        .filter(({ matches, minimumIntervals }) => matches >= minimumIntervals)
        .toSorted((left, right) => right.matches - left.matches)[0]
      const latest = consistent.at(-1)
      return cadence && latest
        ? [
            {
              merchant: latest.transaction.merchant,
              cadence: cadence.label,
              occurrences: consistent.length,
              latestAmount: Math.abs(latest.transaction.amount),
              lastChargedOn: latest.date,
            },
          ]
        : []
    })
    .toSorted(
      (left, right) =>
        right.lastChargedOn.localeCompare(left.lastChargedOn) ||
        left.merchant.localeCompare(right.merchant),
    )
}

export function getPlatinumBenefitActivity(transactions: Transaction[], referenceIso: string) {
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  return transactions
    .flatMap((transaction) => {
      const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      if (!date || date > referenceDay) return []
      const text = `${transaction.merchant} ${transaction.description ?? ''} ${transaction.category}`
      const benefit = platinumBenefits.find(
        ({ id, matches }) => matches.test(text) && !(id === 'uber-cash' && /uber one/i.test(text)),
      )
      return benefit && date >= benefit.effectiveFrom
        ? [
            {
              ...transaction,
              date,
              benefitId: benefit.id,
              benefitName: benefit.name,
              matchEvidence: `Matched by local benefit rules ${benefitRulesVersion}`,
            },
          ]
        : []
    })
    .toSorted((left, right) => right.date.localeCompare(left.date))
}

const sumCredits = (items: Transaction[]) =>
  items.reduce((sum, { amount }) => sum + Math.round(amount * 100), 0) / 100

export function buildPlatinumBenefitHistory(
  activity: ReturnType<typeof getPlatinumBenefitActivity>,
  year: number,
) {
  const current = activity.filter(({ date }) => Number(date.slice(0, 4)) === year)
  const credits = current.filter(({ amount, pending }) => amount > 0 && !pending)

  return {
    activity: current,
    creditedAmount: sumCredits(credits),
    creditCount: credits.length,
    months: Array.from({ length: 12 }, (_, month) => ({
      month: `${year}-${String(month + 1).padStart(2, '0')}`,
      creditedAmount: sumCredits(
        credits.filter(({ date }) => Number(date.slice(5, 7)) === month + 1),
      ),
    })),
    benefits: platinumBenefitOptions
      .map((benefit) => {
        const matches = credits.filter(({ benefitId }) => benefitId === benefit.id)
        return { ...benefit, creditedAmount: sumCredits(matches), creditCount: matches.length }
      })
      .toSorted((left, right) => right.creditedAmount - left.creditedAmount),
  }
}

export function buildPlatinumBenefitTracker(transactions: Transaction[], referenceIso: string) {
  const reference = calendarDate(referenceIso)
  const referenceDay = reference
  const matchedActivity = getPlatinumBenefitActivity(transactions, referenceIso)

  return platinumBenefits
    .map((benefit) => {
      const ruleReference =
        benefit.id === 'hotel' ? calendarDate(referenceIso, 'America/Chicago') : reference
      const window = benefitWindow(ruleReference, benefit.cadence)
      const matches = matchedActivity
        .filter(({ benefitId }) => benefitId === benefit.id)
        .map((transaction) => ({ transaction, date: transaction.date }))
      const activity = matches.filter(
        ({ date }) => date >= dateKey(window.start) && date <= dateKey(window.end),
      )
      const lastCredit = matches
        .filter(({ transaction }) => transaction.amount > 0 && !transaction.pending)
        .toSorted((left, right) => right.date.localeCompare(left.date))[0]
      const cap = benefit.id === 'uber-cash' && reference.getUTCMonth() === 11 ? 35 : benefit.cap
      const creditedAmount =
        Math.round(
          activity
            .filter(({ transaction }) => transaction.amount > 0 && !transaction.pending)
            .reduce((sum, { transaction }) => sum + transaction.amount, 0) * 100,
        ) / 100
      const remainingAmount = Math.max(0, Math.round((cap - creditedAmount) * 100) / 100)
      const status: 'credited' | 'matched' | 'unused' = creditedAmount
        ? 'credited'
        : activity.length
          ? 'matched'
          : 'unused'

      return {
        ...benefit,
        cap,
        status,
        creditedAmount,
        remainingAmount,
        daysRemaining: Math.max(
          0,
          Math.ceil((window.end.getTime() - referenceDay.getTime()) / dayMs),
        ),
        lastCredit:
          lastCredit && !activity.includes(lastCredit)
            ? {
                amount: lastCredit.transaction.amount,
                date: shortDateFormatter.format(new Date(`${lastCredit.date}T00:00:00Z`)),
              }
            : undefined,
        reset: shortDateFormatter.format(window.end),
      }
    })
    .toSorted(
      (left, right) =>
        Number(left.remainingAmount === 0) - Number(right.remainingAmount === 0) ||
        left.daysRemaining - right.daysRemaining ||
        right.remainingAmount - left.remainingAmount,
    )
}
