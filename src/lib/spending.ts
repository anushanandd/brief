import { isSpendingTransaction } from './transaction-kind'
export { isSpendingTransaction } from './transaction-kind'
import type { Account, Transaction } from './schema'

export type SpendingPeriod = 0 | 1 | 3 | 12
export type SpendingPeriodBasis = 'calendar' | 'statement'
const spendingPeriodKeys: Record<string, SpendingPeriod> = { m: 1, q: 3, y: 12, a: 0 }
type BenefitCadence = 'monthly' | 'quarterly' | 'semiannual' | 'annual' | 'renewal' | 'purchase'
const benefitRulesVersion = '2026-09-20'
const amexBenefitUrl = (path: string) => `https://global.americanexpress.com/card-benefits/${path}`

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
  creditMatches: RegExp
  instruction: string
  effectiveFrom?: string
  effectiveUntil?: string
  url: string
  timeZone?: string
  deadlineNote?: string
  allowanceChanges?: Array<{
    from: string
    cap: number
    limit: string
    cadence: BenefitCadence
  }>
}> = [
  {
    id: 'digital-entertainment',
    name: 'Digital entertainment',
    limit: '$20 / month',
    cap: 20,
    cadence: 'monthly',
    matches:
      /digital entertainment|disney|hulu|espn|new york times|paramount|peacock|wall street journal|youtube/i,
    creditMatches: /digital entertainment.*credit/i,
    instruction:
      'Enroll, then pay directly for Disney+, ESPN, Hulu, NYT, Paramount+, standalone Peacock, WSJ or YouTube (including Music). No third-party billing or Peacock bundles with other services.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/digital-entertainment/platinum'),
    allowanceChanges: [{ from: '2025-09-18', cap: 25, limit: '$25 / month', cadence: 'monthly' }],
  },
  {
    id: 'uber-cash',
    name: 'Uber Cash',
    limit: '$15 monthly · +$20 Dec',
    cap: 15,
    cadence: 'monthly',
    matches: /uber cash|^uber(?:\s|$)/i,
    creditMatches: /$^/,
    instruction:
      'Add Platinum to Uber. For U.S. rides and orders, pay with an Amex and turn Uber Cash on. Unused cash expires each month.',
    effectiveFrom: '2025-01-01',
    url: 'https://www.uber.com/us/en/u/amex/',
    timeZone: 'Pacific/Honolulu',
    deadlineNote: '11:59 p.m. Hawaii time',
  },
  {
    id: 'uber-one',
    name: 'Uber One',
    limit: '$120 / year',
    cap: 120,
    cadence: 'annual',
    matches: /uber one/i,
    creditMatches: /uber one.*credit/i,
    instruction:
      'Pay directly with Platinum for a U.S. monthly or annual membership; turn Uber Cash off. Free trials, discounts and Uber Cash payments do not qualify.',
    effectiveFrom: '2025-09-18',
    url: amexBenefitUrl('detail/uber-one-credit/platinum'),
  },
  {
    id: 'walmart-plus',
    name: 'Walmart+',
    limit: '$12.95 + tax / month',
    cap: 12.95,
    cadence: 'monthly',
    matches: /walmart(?:\+| plus|.*membership)|walmart.*credit/i,
    creditMatches: /(?:amex|platinum).*walmart.*credit|walmart(?:\+| plus).*credit/i,
    instruction:
      'Pay with Platinum for one monthly membership, including tax. Excludes annual plans, Plus Ups and Walmart Business+.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/walmart-platinum/platinum'),
  },
  {
    id: 'resy',
    name: 'Resy',
    limit: '$100 / quarter',
    cap: 100,
    cadence: 'quarterly',
    matches: /resy|\btock\b/i,
    creditMatches: /resy.*credit/i,
    instruction:
      'Enroll and use a U.S. venue marked Resy Credit eligible—not just any restaurant. Includes eligible Tock purchases from Sep 15, 2026.',
    effectiveFrom: '2025-09-18',
    url: amexBenefitUrl('detail/400-resy-credit/platinum'),
  },
  {
    id: 'lululemon',
    name: 'lululemon',
    limit: '$75 / quarter',
    cap: 75,
    cadence: 'quarterly',
    matches: /lululemon/i,
    creditMatches: /lululemon.*credit/i,
    instruction:
      'Enroll and buy directly at eligible U.S. stores, website or app. Excludes gift cards, outlets, Like New, Studio, warehouses and events.',
    effectiveFrom: '2025-09-18',
    url: amexBenefitUrl('terms/platinum'),
  },
  {
    id: 'hotel',
    name: 'Hotel credit',
    limit: '$200 / year',
    cap: 200,
    cadence: 'annual',
    matches:
      /hotel\s?credit|fine hotels|\bfhr\b|the hotel collection|amex travel|american express travel/i,
    creditMatches: /hotel\s?credit/i,
    instruction:
      'Prepay Fine Hotels + Resorts or The Hotel Collection through Amex Travel. The Hotel Collection requires two consecutive nights. Excludes pay-at-hotel bookings and property charges.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/hotel-credit/platinum'),
    timeZone: 'America/Chicago',
    deadlineNote: '11:59 p.m. Central time · processing date',
    allowanceChanges: [
      { from: '2025-09-18', cap: 300, limit: '$300 / half-year', cadence: 'semiannual' },
    ],
  },
  {
    id: 'airline-fee',
    name: 'Airline fee',
    limit: '$200 / year',
    cap: 200,
    cadence: 'annual',
    matches:
      /airline fee (?:credit|reimbursement)|checked bag|baggage fee|inflight|in-flight|seat fee|lounge fee/i,
    creditMatches: /airline fee (?:credit|reimbursement)/i,
    instruction:
      'Select an airline in Amex first; changes are allowed in January. Covers eligible incidental fees charged separately, not tickets, upgrades, gift cards or miles.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/airline-fee-credit/platinum'),
  },
  {
    id: 'clear',
    name: 'CLEAR+',
    limit: '$209 / year',
    cap: 209,
    cadence: 'annual',
    matches: /\bclear\b/i,
    creditMatches: /(?:amex|platinum).*clear.*credit|clear\+? plus.*credit/i,
    instruction:
      'Pay with Platinum for auto-renewing CLEAR+ and complete identity verification. Membership coverage excludes taxes and fees.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/clear-credit/platinum'),
    allowanceChanges: [{ from: '2026-07-01', cap: 219, limit: '$219 / year', cadence: 'annual' }],
  },
  {
    id: 'oura',
    name: 'Oura Ring',
    limit: '$200 / year',
    cap: 200,
    cadence: 'annual',
    matches: /\boura\b/i,
    creditMatches: /oura.*credit/i,
    instruction:
      'Enroll and buy a ring at U.S. Ouraring.com for U.S. delivery. Excludes memberships, chargers, warranties, gift cards and other retailers.',
    effectiveFrom: '2025-09-18',
    url: amexBenefitUrl('detail/oura-credit/platinum'),
  },
  {
    id: 'equinox',
    name: 'Equinox',
    limit: '$300 / year',
    cap: 300,
    cadence: 'annual',
    matches: /^(?!.*(?:soulcycle|bike)).*equinox/i,
    creditMatches: /^(?!.*(?:soulcycle|bike)).*equinox.*credit/i,
    instruction:
      'Enroll and validate eligibility. Pay Equinox directly for an eligible club membership or Equinox+ subscription. Excludes app-store billing.',
    effectiveFrom: '2025-01-01',
    url: amexBenefitUrl('detail/credit-equinox/platinum'),
  },
  {
    id: 'global-entry',
    name: 'Global Entry / TSA PreCheck',
    limit: '$120 / up to $85',
    cap: 120,
    cadence: 'renewal',
    matches: /global entry|tsa\s?pre\s?check/i,
    creditMatches: /(?:global entry|tsa\s?pre\s?check).*(?:credit|reimbursement)/i,
    instruction:
      'Pay the application fee: $120 Global Entry or up to $85 TSA PreCheck, once every four years. Additional cards have separate eligibility. Confirm your next eligible date in Amex.',
    url: amexBenefitUrl('terms/platinum'),
  },
  {
    id: 'soulcycle',
    name: 'SoulCycle bike',
    limit: '$300 / qualifying bike',
    cap: 300,
    cadence: 'purchase',
    matches: /soulcycle|equinox.*bike/i,
    creditMatches: /(?:soulcycle|equinox.*bike).*(?:credit|reimbursement)/i,
    instruction:
      'Buy a full-price at-home bike at Equinox+ in one transaction with a 12-month Equinox+ membership. No financing. Up to 15 qualifying bikes per year.',
    url: amexBenefitUrl('terms/platinum'),
  },
  {
    id: 'saks',
    name: 'Saks Fifth Avenue',
    limit: '$50 / half-year',
    cap: 50,
    cadence: 'semiannual',
    matches: /saks/i,
    creditMatches: /saks.*(?:credit|reimbursement)/i,
    instruction: 'Retired July 1, 2026. Historical credits remain included by posting date.',
    effectiveUntil: '2026-06-30',
    url: amexBenefitUrl('view-all/platinum'),
  },
]

export const platinumBenefitOptions = platinumBenefits
  .filter(({ effectiveUntil }) => !effectiveUntil)
  .map(({ id, name }) => ({ id, name }))

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

export function formatActivityDate(value: string, referenceIso: string) {
  const transactionDate = transactionDateKey(value, referenceIso)
  return transactionDate
    ? activityDateFormatter.format(new Date(`${transactionDate}T00:00:00Z`))
    : value
}

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

export function weeklySpendingCategoryPerformance(
  transactions: Transaction[],
  referenceIso: string,
) {
  const end = calendarDate(referenceIso)
  const currentStart = addDays(end, -6)
  const previousEnd = addDays(currentStart, -1)
  const previousStart = addDays(previousEnd, -6)
  const totals = (start: Date, through: Date) => {
    const values = new Map<string, number>()
    for (const transaction of transactions) {
      const day = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      if (
        transaction.pending ||
        !isSpend(transaction) ||
        day < dateKey(start) ||
        day > dateKey(through)
      )
        continue
      values.set(
        transaction.category,
        (values.get(transaction.category) ?? 0) + minorUnits(Math.abs(transaction.amount)),
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

export function spendingMonthDirectionForKey(
  event: Pick<KeyboardEvent, 'altKey' | 'ctrlKey' | 'key' | 'metaKey' | 'shiftKey'>,
) {
  if (
    event.altKey ||
    event.ctrlKey ||
    event.shiftKey ||
    (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')
  ) {
    return undefined
  }
  return event.key === 'ArrowLeft' ? -1 : 1
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

export function getPlatinumBenefitActivity(transactions: Transaction[], referenceIso: string) {
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  return transactions
    .flatMap((transaction) => {
      const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
      if (
        !date ||
        date > referenceDay ||
        !Number.isFinite(transaction.amount) ||
        transaction.amount === 0
      )
        return []
      const text = `${transaction.merchant} ${transaction.description ?? ''}`
      const uberOneSubscription =
        minorUnits(transaction.amount) === -999 &&
        transaction.merchant.trim().toLocaleLowerCase() === 'uber'
      const benefit = platinumBenefits.find(
        ({ id, matches, creditMatches }) =>
          (matches.test(text) ||
            creditMatches.test(text) ||
            (id === 'uber-one' && uberOneSubscription)) &&
          !(id === 'uber-cash' && (/uber one/i.test(text) || uberOneSubscription)) &&
          !(id === 'resy' && !/resy/i.test(text) && date < '2026-09-15'),
      )
      // ponytail: conservative descriptor matching; unknown issuer descriptions stay unverified,
      // not inferred as credits from merchant names or a generic Credit category.
      const identifiedCredit =
        benefit?.id !== 'uber-cash' &&
        transaction.benefitConfirmed !== false &&
        (transaction.benefitConfirmed === true ||
          (benefit?.creditMatches.test(text) && !/\brefund\b/i.test(text)))
      const kind = identifiedCredit
        ? transaction.amount > 0
          ? ('credit' as const)
          : ('reversal' as const)
        : transaction.amount < 0
          ? ('purchase' as const)
          : ('unverified' as const)
      return benefit &&
        (!benefit.effectiveFrom || date >= benefit.effectiveFrom) &&
        (!benefit.effectiveUntil || date <= benefit.effectiveUntil || identifiedCredit)
        ? [
            {
              ...transaction,
              date,
              benefitId: benefit.id,
              benefitName: benefit.name,
              kind,
              matchEvidence: `Matched by local benefit rules ${benefitRulesVersion}`,
            },
          ]
        : []
    })
    .toSorted((left, right) => right.date.localeCompare(left.date))
}

const sumCredits = (items: Array<{ amount: number }>) =>
  items.reduce((sum, { amount }) => sum + Math.round(amount * 100), 0) / 100

// ponytail: the card feed has no itemized Uber tender. Assume the monthly allowance was used
// after any posted matching purchase; replace this only with provider-supplied tender evidence.
const isPostedUberPurchase = ({
  benefitId,
  kind,
  pending,
}: {
  benefitId: string
  kind: string
  pending: boolean
}) => benefitId === 'uber-cash' && kind === 'purchase' && !pending

export function buildPlatinumBenefitHistory(
  activity: ReturnType<typeof getPlatinumBenefitActivity>,
  year: number,
) {
  const current = activity.filter(({ date }) => Number(date.slice(0, 4)) === year)
  const credits = current.filter(
    ({ kind, pending }) => (kind === 'credit' || kind === 'reversal') && !pending,
  )
  const uberPurchases = current.filter(isPostedUberPurchase)
  const estimatedActivity = [...new Set(uberPurchases.map(({ date }) => date.slice(0, 7)))].map(
    (month) => {
      const purchases = uberPurchases.filter(({ date }) => date.startsWith(month))
      return {
        id: `estimated-uber-cash-${month}`,
        merchant: 'Uber Cash',
        date: purchases.at(-1)!.date,
        amount: month.endsWith('-12') ? 35 : 15,
        pending: false,
        benefitId: 'uber-cash',
        benefitName: 'Uber Cash',
        kind: 'estimate' as const,
        description: `Estimated from ${purchases.length} posted Uber ${purchases.length === 1 ? 'purchase' : 'purchases'}.`,
      }
    },
  )
  const historyActivity = [...credits, ...estimatedActivity]

  return {
    activity: current,
    estimatedActivity,
    creditedAmount: sumCredits(credits),
    estimatedAmount: sumCredits(estimatedActivity),
    creditCount: credits.filter(({ kind }) => kind === 'credit').length,
    months: Array.from({ length: 12 }, (_, month) => ({
      month: `${year}-${String(month + 1).padStart(2, '0')}`,
      creditedAmount: sumCredits(
        credits.filter(({ date }) => Number(date.slice(5, 7)) === month + 1),
      ),
    })),
    benefits: platinumBenefits
      .map((benefit) => {
        const matches = historyActivity.filter(({ benefitId }) => benefitId === benefit.id)
        return {
          id: benefit.id,
          name: benefit.name,
          retiredOn: benefit.effectiveUntil
            ? dateKey(addDays(new Date(`${benefit.effectiveUntil}T00:00:00Z`), 1))
            : undefined,
          creditedAmount: sumCredits(matches),
          creditCount: matches.filter(({ kind }) => kind === 'credit').length,
          estimatedCount: matches.filter(({ kind }) => kind === 'estimate').length,
        }
      })
      .toSorted((left, right) => right.creditedAmount - left.creditedAmount),
  }
}

export function buildPlatinumBenefitTracker(
  transactions: Transaction[],
  referenceIso: string,
  snapshotIso = referenceIso,
) {
  const reference = calendarDate(referenceIso)
  const matchedActivity = getPlatinumBenefitActivity(transactions, snapshotIso)

  return platinumBenefits
    .filter(
      ({ effectiveFrom, effectiveUntil }) =>
        (!effectiveFrom || effectiveFrom <= dateKey(reference)) &&
        (!effectiveUntil || effectiveUntil >= dateKey(reference)),
    )
    .map((definition) => {
      const ruleReference = calendarDate(referenceIso, definition.timeZone)
      const allowance = definition.allowanceChanges?.findLast(
        ({ from }) => from <= dateKey(ruleReference),
      )
      const benefit = { ...definition, ...allowance }
      const window = benefitWindow(ruleReference, benefit.cadence)
      // A changed cadence starts a new allowance, not a retroactive application of the new cap.
      const firstDay =
        allowance && allowance.cadence !== definition.cadence
          ? allowance.from
          : definition.effectiveFrom
      if (firstDay && firstDay > dateKey(window.start))
        window.start = new Date(`${firstDay}T00:00:00Z`)
      const matches = matchedActivity
        .filter(({ benefitId, date }) => benefitId === benefit.id && date <= dateKey(ruleReference))
        .map((transaction) => ({ transaction, date: transaction.date }))
      const activity = matches.filter(
        ({ date }) =>
          benefit.cadence === 'renewal' ||
          (date >= dateKey(window.start) && date <= dateKey(window.end)),
      )
      const lastCredit = matches
        .filter(({ transaction }) => transaction.kind === 'credit' && !transaction.pending)
        .toSorted((left, right) => right.date.localeCompare(left.date))[0]
      const cap =
        benefit.id === 'uber-cash' && ruleReference.getUTCMonth() === 11 ? 35 : benefit.cap
      const credits = activity
        .map(({ transaction }) => transaction)
        .filter(({ kind, pending }) => (kind === 'credit' || kind === 'reversal') && !pending)
      const creditedAmount = sumCredits(credits)
      const estimatedCreditAmount =
        benefit.id === 'uber-cash' &&
        activity.some(({ transaction }) => isPostedUberPurchase(transaction))
          ? cap
          : benefit.id === 'uber-cash'
            ? 0
            : null
      const postingDelayDays = benefit.id === 'hotel' ? 90 : 56
      const hasPriorWindow =
        !definition.effectiveFrom || definition.effectiveFrom < dateKey(window.start)
      // ponytail: the feed has no purchase-to-credit link. Near-reset credits and reversals
      // cannot establish the benefit period; add attribution only with explicit provider evidence.
      const periodUncertain = credits.some(
        ({ kind, date }) =>
          kind === 'reversal' ||
          (hasPriorWindow && date <= dateKey(addDays(window.start, postingDelayDays - 1))),
      )
      const snapshotBeforeWindow =
        dateKey(calendarDate(snapshotIso, definition.timeZone)) < dateKey(window.start)
      const remainingAmount =
        periodUncertain ||
        snapshotBeforeWindow ||
        ['uber-cash', 'walmart-plus'].includes(benefit.id) ||
        benefit.cadence === 'renewal' ||
        benefit.cadence === 'purchase'
          ? null
          : Math.min(cap, Math.max(0, Math.round((cap - creditedAmount) * 100) / 100))
      const renewalCredit =
        benefit.cadence === 'renewal' &&
        lastCredit &&
        !credits.some(({ kind, date }) => kind === 'reversal' && date >= lastCredit.date)
          ? lastCredit
          : undefined
      const renewalDate = renewalCredit ? new Date(`${renewalCredit.date}T00:00:00Z`) : undefined
      if (renewalDate) renewalDate.setUTCFullYear(renewalDate.getUTCFullYear() + 4)
      const status =
        benefit.id === 'uber-cash'
          ? 'external'
          : creditedAmount > 0
            ? 'credited'
            : activity.some(({ transaction }) => transaction.kind === 'purchase')
              ? 'matched'
              : 'unused'

      return {
        ...benefit,
        cap,
        periodUncertain,
        snapshotBeforeWindow,
        postingDelayDays,
        renewalCheck: renewalDate ? activityDateFormatter.format(renewalDate) : undefined,
        status,
        creditedAmount,
        estimatedCreditAmount,
        remainingAmount,
        activity: activity.map(({ transaction }) => transaction),
        windowStart: dateKey(window.start),
        windowEnd: dateKey(window.end),
        windowLabel: `${shortDateFormatter.format(window.start)} – ${shortDateFormatter.format(window.end)}, ${window.end.getUTCFullYear()}`,
        daysRemaining: Math.max(
          0,
          Math.ceil((window.end.getTime() - ruleReference.getTime()) / dayMs),
        ),
        lastCredit:
          lastCredit && !activity.includes(lastCredit)
            ? {
                amount: lastCredit.transaction.amount,
                date: activityDateFormatter.format(new Date(`${lastCredit.date}T00:00:00Z`)),
              }
            : undefined,
        reset: shortDateFormatter.format(window.end),
      }
    })
    .toSorted(
      (left, right) =>
        Number(left.cadence === 'renewal' || left.cadence === 'purchase') -
          Number(right.cadence === 'renewal' || right.cadence === 'purchase') ||
        left.windowEnd.localeCompare(right.windowEnd) ||
        left.name.localeCompare(right.name),
    )
}
