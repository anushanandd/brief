import { websiteDomain } from './logos'
import type { FinanceSnapshot, Transaction } from './schema'
import { platinumBenefitCreditIdentity, transactionDateKey } from './spending'
import { moneyKind } from './transaction-kind'
export { moneyKind, type MoneyKind } from './transaction-kind'

export type MoneyRange = 'week' | 'month' | 'quarter' | 'year' | 'all'

const dayMs = 86_400_000
const money = (value: number) => Math.round(value * 100) / 100
const dateKey = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`

export function moneyRangeStart(range: MoneyRange, referenceIso: string) {
  if (range === 'all') return '0000-01-01'
  const reference = new Date(`${referenceIso.slice(0, 10)}T12:00:00`)
  if (range === 'week') {
    const start = new Date(reference.getFullYear(), reference.getMonth(), reference.getDate())
    start.setDate(start.getDate() - ((start.getDay() + 6) % 7))
    return dateKey(start)
  }
  if (range === 'month') return dateKey(new Date(reference.getFullYear(), reference.getMonth(), 1))
  if (range === 'quarter')
    return dateKey(new Date(reference.getFullYear(), Math.floor(reference.getMonth() / 3) * 3, 1))
  return dateKey(new Date(reference.getFullYear(), 0, 1))
}

export function moneySummary(data: FinanceSnapshot, range: MoneyRange) {
  const start = moneyRangeStart(range, data.updatedAt)
  const end = data.updatedAt.slice(0, 10)
  const transactions = data.transactions.filter((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
    return !transaction.pending && date >= start && date <= end
  })
  const totals = {
    income: 0,
    dividends: 0,
    interest: 0,
    fees: 0,
    taxes: 0,
    reimbursements: 0,
    spending: 0,
    transfers: 0,
  }
  for (const transaction of transactions) {
    const amount = Math.abs(transaction.amount)
    switch (moneyKind(transaction)) {
      case 'income':
        if (transaction.amount > 0) totals.income += amount
        break
      case 'dividend':
        if (transaction.amount > 0) totals.dividends += amount
        break
      case 'interest':
        if (transaction.amount > 0) totals.interest += amount
        break
      case 'fee':
        totals.fees += amount
        break
      case 'tax':
        totals.taxes += amount
        break
      case 'reimbursement':
        if (transaction.amount > 0) totals.reimbursements += amount
        break
      case 'expense':
        totals.spending += amount
        break
      case 'transfer':
        totals.transfers += transaction.amount
        break
    }
  }
  const sales = data.trades.filter((trade) => {
    const date = transactionDateKey(trade.date, data.updatedAt)
    return trade.type.toLocaleUpperCase().includes('SELL') && date >= start && date <= end
  })
  const estimatedRealizedGain = sales.every((trade) => trade.estimatedRealizedGain != null)
    ? money(sales.reduce((total, trade) => total + (trade.estimatedRealizedGain ?? 0), 0))
    : null
  const grossIncome = totals.income + totals.dividends + totals.interest
  const personalSpending = Math.max(0, totals.spending - totals.reimbursements)
  const netCashFlow = grossIncome - personalSpending - totals.fees - totals.taxes
  return {
    income: money(totals.income),
    dividends: money(totals.dividends),
    interest: money(totals.interest),
    fees: money(totals.fees),
    taxes: money(totals.taxes),
    reimbursements: money(totals.reimbursements),
    spending: money(totals.spending),
    transfers: money(totals.transfers),
    estimatedRealizedGain,
    grossIncome: money(grossIncome),
    personalSpending: money(personalSpending),
    netCashFlow: money(netCashFlow),
    transactions,
    start,
    end,
  }
}

export type CashFlowBreakdownItem = {
  id: string
  label: string
  value: number
  children: CashFlowBreakdownTransaction[]
}

export type CashFlowBreakdownTransaction = {
  id: string
  label: string
  value: number
  date: string
}

function aggregateCashFlowItems(
  entries: Array<{ group: string; transaction: CashFlowBreakdownTransaction }>,
  prefix: string,
): CashFlowBreakdownItem[] {
  const groups = new Map<string, CashFlowBreakdownTransaction[]>()
  for (const { group, transaction } of entries) {
    const children = groups.get(group) ?? []
    children.push(transaction)
    groups.set(group, children)
  }
  return [...groups]
    .map(([label, children]) => ({
      id: `${prefix}:${label}`,
      label,
      value: money(children.reduce((total, child) => total + child.value, 0)),
      children: children.toSorted(
        (left, right) =>
          right.date.localeCompare(left.date) ||
          right.value - left.value ||
          left.label.localeCompare(right.label),
      ),
    }))
    .toSorted((left, right) => right.value - left.value || left.label.localeCompare(right.label))
}

export function cashFlowBreakdown(transactions: Transaction[]) {
  const sources: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  const purchases: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  let fees = 0
  let taxes = 0
  for (const transaction of transactions) {
    const kind = moneyKind(transaction)
    const amount = Math.abs(transaction.amount)
    if (
      transaction.amount > 0 &&
      (kind === 'income' ||
        kind === 'dividend' ||
        kind === 'interest' ||
        kind === 'reimbursement' ||
        kind === 'fee' ||
        kind === 'tax')
    ) {
      const source = transaction.merchant.trim() || 'Unknown source'
      sources.push({
        group: source,
        transaction: {
          id: `source-transaction:${transaction.id}`,
          label: transaction.description?.trim() || source,
          value: money(amount),
          date: (transaction.postedOn ?? transaction.date).slice(0, 10),
        },
      })
    } else if (kind === 'expense') {
      purchases.push({
        group: transaction.category.trim() || 'Other',
        transaction: {
          id: `purchase-transaction:${transaction.id}`,
          label: transaction.merchant.trim() || transaction.description?.trim() || 'Purchase',
          value: money(amount),
          date: (transaction.postedOn ?? transaction.date).slice(0, 10),
        },
      })
    } else if (kind === 'fee') {
      fees += amount
    } else if (kind === 'tax') {
      taxes += amount
    }
  }
  return {
    sources: aggregateCashFlowItems(sources, 'source'),
    purchases: aggregateCashFlowItems(purchases, 'purchase'),
    fees: money(fees),
    taxes: money(taxes),
  }
}

export type ExpectedMoneyEvent = {
  id: string
  recurringId: string
  title: string
  kind: 'credit' | 'income' | 'payment' | 'subscription'
  date: string
  amount: number
  amountLow: number
  amountHigh: number
  frequency: string
  observations: number
  lastSeen: string
  state: 'expected' | 'pending' | 'awaiting-post'
  accountId?: string
  matchedTransactionId?: string
}

const recurringMerchant = (transaction: Transaction) => {
  const domain = transaction.website && websiteDomain(transaction.website)
  if (domain) return `domain:${domain.toLocaleLowerCase()}`
  return transaction.merchant
    .toLocaleLowerCase()
    .replace(
      /\b(?:reference|ref|confirmation|transaction)\s*(?:number|no)?\s*:?\s*[a-z0-9-]{5,}\b.*$/i,
      ' ',
    )
    .replace(/\b[x*]{3,}\d{2,}\b/gi, ' ')
    .replace(/\b\d{5,}\b/g, ' ')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
}

export const recurringMoneyKey = (transaction: Transaction) => {
  const benefit = transaction.amount > 0 ? platinumBenefitCreditIdentity(transaction) : undefined
  return JSON.stringify({
    version: 3,
    account: transaction.accountId ?? transaction.account,
    direction: transaction.amount < 0 ? 'out' : 'in',
    merchant: benefit ? `platinum-benefit:${benefit.id}` : recurringMerchant(transaction),
  })
}

const median = (values: number[]) => {
  const sorted = values.toSorted((left, right) => left - right)
  return sorted[Math.floor(sorted.length / 2)]
}

type Cadence = 'weekly' | 'biweekly' | 'semi-monthly' | 'monthly' | 'quarterly' | 'annual'
const cadences: Cadence[] = ['semi-monthly', 'biweekly', 'weekly', 'monthly', 'quarterly', 'annual']
const cadenceLabel: Record<Cadence, string> = {
  weekly: 'Weekly',
  biweekly: 'Every two weeks',
  'semi-monthly': 'Twice monthly',
  monthly: 'Monthly',
  quarterly: 'Quarterly',
  annual: 'Yearly',
}
const utcDate = (key: string) => new Date(`${key.slice(0, 10)}T12:00:00Z`)
const utcKey = (date: Date) => date.toISOString().slice(0, 10)
const daysBetween = (left: string, right: string) =>
  Math.round((utcDate(right).getTime() - utcDate(left).getTime()) / dayMs)
const observedHoliday = (year: number, month: number, day: number) => {
  const date = new Date(Date.UTC(year, month, day, 12))
  if (date.getUTCDay() === 6) date.setUTCDate(date.getUTCDate() - 1)
  if (date.getUTCDay() === 0) date.setUTCDate(date.getUTCDate() + 1)
  return utcKey(date)
}
const nthWeekday = (year: number, month: number, weekday: number, occurrence: number) => {
  const date = new Date(Date.UTC(year, month, 1, 12))
  date.setUTCDate(1 + ((weekday - date.getUTCDay() + 7) % 7) + (occurrence - 1) * 7)
  return utcKey(date)
}
const lastWeekday = (year: number, month: number, weekday: number) => {
  const date = new Date(Date.UTC(year, month + 1, 0, 12))
  date.setUTCDate(date.getUTCDate() - ((date.getUTCDay() - weekday + 7) % 7))
  return utcKey(date)
}
const isFederalHoliday = (date: Date) => {
  const key = utcKey(date)
  const year = date.getUTCFullYear()
  return new Set([
    observedHoliday(year, 0, 1),
    observedHoliday(year + 1, 0, 1),
    nthWeekday(year, 0, 1, 3),
    nthWeekday(year, 1, 1, 3),
    lastWeekday(year, 4, 1),
    ...(year >= 2021 ? [observedHoliday(year, 5, 19)] : []),
    observedHoliday(year, 6, 4),
    nthWeekday(year, 8, 1, 1),
    nthWeekday(year, 9, 1, 2),
    observedHoliday(year, 10, 11),
    nthWeekday(year, 10, 4, 4),
    observedHoliday(year, 11, 25),
  ]).has(key)
}
const isBusinessDay = (date: Date) =>
  date.getUTCDay() !== 0 && date.getUTCDay() !== 6 && !isFederalHoliday(date)
const businessDaysAfter = (left: string, right: string) => {
  let days = 0
  for (
    let time = utcDate(left).getTime() + dayMs;
    time <= utcDate(right).getTime() && days <= 5;
    time += dayMs
  ) {
    if (isBusinessDay(new Date(time))) days++
  }
  return days
}
const shiftToBusinessDay = (key: string, direction: -1 | 1) => {
  const date = utcDate(key)
  while (!isBusinessDay(date)) date.setUTCDate(date.getUTCDate() + direction)
  return utcKey(date)
}
const monthIndex = (date: Date) => date.getUTCFullYear() * 12 + date.getUTCMonth()
const monthEnd = (date: Date) =>
  date.getUTCDate() ===
  new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0)).getUTCDate()
const nextMonthDate = (date: Date, months: number, day: number, endOfMonth: boolean) => {
  const first = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + months, 1, 12))
  const lastDay = new Date(
    Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0),
  ).getUTCDate()
  first.setUTCDate(endOfMonth ? lastDay : Math.min(day, lastDay))
  return first
}

function matchesCadence(left: string, right: string, cadence: Cadence) {
  const days = daysBetween(left, right)
  if (cadence === 'weekly') return Math.abs(days - 7) <= 2
  if (cadence === 'biweekly') return Math.abs(days - 14) <= 3
  if (cadence === 'semi-monthly') return days >= 12 && days <= 19
  const earlier = utcDate(left)
  const later = utcDate(right)
  const months = cadence === 'monthly' ? 1 : cadence === 'quarterly' ? 3 : 12
  return (
    monthIndex(later) - monthIndex(earlier) === months &&
    (Math.abs(later.getUTCDate() - earlier.getUTCDate()) <= 4 ||
      (monthEnd(earlier) && monthEnd(later)))
  )
}

function semiMonthlyAnchors(dates: string[]) {
  if (dates.length < 4) return null
  const days = dates.map((date) => utcDate(date).getUTCDate())
  const alternating = [
    days.filter((_, index) => index % 2 === 0),
    days.filter((_, index) => index % 2 === 1),
  ]
  const anchors = alternating.map(median)
  const first = Math.min(...anchors)
  const second = Math.max(...anchors)
  if (
    second - first < 12 ||
    alternating.some((group, index) => group.some((day) => Math.abs(day - anchors[index]) > 3))
  )
    return null
  return {
    first,
    second,
    endOfMonth: dates
      .filter((date) => Math.abs(utcDate(date).getUTCDate() - second) <= 3)
      .every((date) => monthEnd(utcDate(date))),
  }
}

function nextOccurrence(dates: string[], cadence: Cadence, transaction: Transaction): string {
  const last = utcDate(dates.at(-1)!)
  let next: string
  if (cadence === 'weekly' || cadence === 'biweekly') {
    next = utcKey(new Date(last.getTime() + (cadence === 'weekly' ? 7 : 14) * dayMs))
  } else if (cadence === 'semi-monthly') {
    const anchors = semiMonthlyAnchors(dates)!
    next = utcKey(
      last.getUTCDate() <= anchors.first + 3
        ? nextMonthDate(last, 0, anchors.second, anchors.endOfMonth)
        : nextMonthDate(last, 1, anchors.first, false),
    )
  } else {
    const anchorDay = median(dates.map((date) => utcDate(date).getUTCDate()))
    const endOfMonth = dates.every((date) => monthEnd(utcDate(date)))
    next = utcKey(
      nextMonthDate(
        last,
        cadence === 'monthly' ? 1 : cadence === 'quarterly' ? 3 : 12,
        anchorDay,
        endOfMonth,
      ),
    )
  }
  const evidence = `${transaction.merchant} ${transaction.category} ${transaction.categoryDetail ?? ''} ${transaction.description ?? ''} ${transaction.transactionCode ?? ''}`
  if (/payroll|paycheck|salary|direct deposit/i.test(evidence)) return shiftToBusinessDay(next, -1)
  if (
    /\bach\b|direct debit|rent|mortgage|loan|utility|utilities|electric|water|insurance/i.test(
      evidence,
    )
  )
    return shiftToBusinessDay(next, 1)
  return next
}

function amountClusters(values: Transaction[]) {
  const clusters: Transaction[][] = []
  for (const transaction of values) {
    const amount = Math.abs(transaction.amount)
    const cluster = clusters
      .map((entries) => ({
        entries,
        typical: median(entries.map((entry) => Math.abs(entry.amount))),
      }))
      .filter(({ typical }) => Math.abs(amount - typical) <= Math.max(2, typical * 0.08))
      .toSorted(
        (left, right) => Math.abs(amount - left.typical) - Math.abs(amount - right.typical),
      )[0]?.entries
    if (cluster) cluster.push(transaction)
    else clusters.push([transaction])
  }
  return clusters
}

function recurringMoneyTitle(transaction: Transaction, benefitName?: string) {
  if (benefitName) return benefitName
  const description = transaction.description?.trim()
  const banking =
    /\bach\b|direct deposit|automatic payment|autopay|bill payment|banking|income|loan payments|rent and utilities/i.test(
      `${transaction.merchant} ${transaction.category} ${transaction.categoryDetail ?? ''} ${transaction.transactionCode ?? ''}`,
    )
  return banking && description ? description : transaction.merchant
}

function recurrenceTraits(transaction: Transaction) {
  const evidence = `${transaction.merchant} ${transaction.category} ${transaction.categoryDetail ?? ''} ${transaction.description ?? ''}`
  const kind = moneyKind(transaction)
  const benefitCredit =
    transaction.amount > 0 ? platinumBenefitCreditIdentity(transaction) : undefined
  const debtPayment =
    transaction.amount < 0 &&
    kind === 'transfer' &&
    /loan payments?|credit card payment|card payment|autopay payment/i.test(evidence)
  return {
    benefitCredit,
    credit: Boolean(benefitCredit) || transaction.classification.credit,
    debtPayment,
    income: kind === 'income' || kind === 'dividend' || kind === 'interest',
    subscription: /subscription|streaming|membership|recurring/i.test(evidence),
    variableBill:
      debtPayment || /utility|utilities|electric|water|internet|phone|insurance/i.test(evidence),
    habitual:
      /grocer|supermarket|food and drink|restaurant|dining|coffee|gas station|fuel|rideshare|taxi|transit|general merchandise|department store|retail/i.test(
        `${transaction.category} ${transaction.categoryDetail ?? ''}`,
      ),
  }
}

function cadenceFor(values: Transaction[], updatedAt: string, strongAnnualEvidence: boolean) {
  const ordered = values.toSorted((left, right) =>
    transactionDateKey(left.postedOn ?? left.date, updatedAt).localeCompare(
      transactionDateKey(right.postedOn ?? right.date, updatedAt),
    ),
  )
  const dates = ordered.map((transaction) =>
    transactionDateKey(transaction.postedOn ?? transaction.date, updatedAt),
  )
  if (dates.some((date) => !/^\d{4}-\d{2}-\d{2}$/.test(date))) return null
  let selected: { cadence: Cadence; start: number } | undefined
  for (const cadence of cadences) {
    let start = ordered.length - 1
    while (start > 0 && matchesCadence(dates[start - 1], dates[start], cadence)) start--
    const run = dates.slice(start)
    const minimum = cadence === 'semi-monthly' ? 4 : cadence === 'annual' ? 2 : 3
    if (run.length < minimum || (cadence === 'annual' && !strongAnnualEvidence)) continue
    if (cadence === 'semi-monthly' && !semiMonthlyAnchors(run)) continue
    if (!selected || run.length > ordered.length - selected.start) selected = { cadence, start }
  }
  if (!selected) return null
  return {
    cadence: selected.cadence,
    transactions: ordered.slice(selected.start),
    dates: dates.slice(selected.start),
  }
}

export function expectedMoneyEvents(
  data: FinanceSnapshot,
  today = data.updatedAt.slice(0, 10),
): ExpectedMoneyEvent[] {
  // An old saved snapshot cannot establish that a missing payment has stopped.
  if (daysBetween(data.updatedAt.slice(0, 10), today) > 3) return []
  const groups = new Map<string, Transaction[]>()
  for (const transaction of data.transactions) {
    const traits = recurrenceTraits(transaction)
    if (
      (moneyKind(transaction) === 'transfer' && !traits.debtPayment) ||
      !transaction.merchant.trim()
    )
      continue
    const key = recurringMoneyKey(transaction)
    const current = groups.get(key) ?? []
    current.push(transaction)
    groups.set(key, current)
  }
  const events: ExpectedMoneyEvent[] = []
  for (const [key, values] of groups) {
    const ordered = values
      .filter(({ pending }) => !pending)
      .toSorted((left, right) =>
        transactionDateKey(left.postedOn ?? left.date, data.updatedAt).localeCompare(
          transactionDateKey(right.postedOn ?? right.date, data.updatedAt),
        ),
      )
      .slice(-12)
    const latest = ordered.at(-1)
    if (!latest) continue
    const accountId = latest.accountId
    const account = data.accounts.find(({ id }) => id === accountId)
    const sourceUpdatedAt = account?.activityAsOf ?? account?.balanceFetchedAt
    if (accountId?.startsWith('plaid:') && (!account || !sourceUpdatedAt)) continue
    if (sourceUpdatedAt && daysBetween(sourceUpdatedAt.slice(0, 10), today) > 3) continue

    const flexibleAmounts =
      ordered.every((transaction) => {
        const traits = recurrenceTraits(transaction)
        return traits.income || traits.credit
      }) || ordered.some((transaction) => recurrenceTraits(transaction).variableBill)
    const clusters = flexibleAmounts ? [ordered] : amountClusters(ordered)
    const candidateSeries = clusters.map((transactions) => ({
      transactions,
      amountBasis: transactions,
    }))
    for (const older of clusters) {
      for (const newer of clusters) {
        if (
          older === newer ||
          older.length < 2 ||
          newer.length < 2 ||
          transactionDateKey(older.at(-1)!.postedOn ?? older.at(-1)!.date, data.updatedAt) >=
            transactionDateKey(newer[0].postedOn ?? newer[0].date, data.updatedAt)
        )
          continue
        candidateSeries.push({
          transactions: [...older, ...newer],
          amountBasis: newer,
        })
      }
    }

    const candidates = candidateSeries.flatMap(({ transactions, amountBasis }) => {
      const traits = recurrenceTraits(transactions.at(-1)!)
      if (!traits.income && !traits.credit && traits.habitual && !traits.subscription) return []
      const cadence = cadenceFor(
        transactions,
        data.updatedAt,
        traits.income || traits.credit || traits.subscription || traits.variableBill,
      )
      if (!cadence) return []
      const last = cadence.transactions.at(-1)!
      const expected = nextOccurrence(cadence.dates, cadence.cadence, last)
      // A late posting can be pending, but a missed occurrence never advances to another cycle.
      if (
        daysBetween(today, expected) > 30 ||
        (expected < today && businessDaysAfter(expected, today) > 5)
      )
        return []
      const amounts = cadence.transactions.map(({ amount }) => Math.abs(amount))
      const typicalAmount = median(amountBasis.map(({ amount }) => Math.abs(amount)))
      return [
        {
          ids: new Set(cadence.transactions.map(({ id }) => id)),
          event: {
            id: key,
            recurringId: key,
            title: recurringMoneyTitle(last, traits.benefitCredit?.name),
            kind: traits.benefitCredit
              ? ('credit' as const)
              : traits.income
                ? ('income' as const)
                : traits.subscription
                  ? ('subscription' as const)
                  : ('payment' as const),
            date: expected,
            amount: money(last.amount > 0 ? typicalAmount : -typicalAmount),
            amountLow: money(Math.min(...amounts)),
            amountHigh: money(Math.max(...amounts)),
            frequency: cadenceLabel[cadence.cadence],
            observations: cadence.transactions.length,
            lastSeen: cadence.dates.at(-1)!,
            state: expected < today ? ('awaiting-post' as const) : ('expected' as const),
            accountId: last.accountId,
          },
        },
      ]
    })
    const accepted: typeof candidates = []
    const usedTransactions = new Set<string>()
    for (const candidate of candidates.toSorted(
      (left, right) =>
        right.event.lastSeen.localeCompare(left.event.lastSeen) ||
        right.event.observations - left.event.observations,
    )) {
      if ([...candidate.ids].some((id) => usedTransactions.has(id))) continue
      accepted.push(candidate)
      candidate.ids.forEach((id) => usedTransactions.add(id))
    }
    const groupEvents = accepted.map(({ event }, index) => ({
      ...event,
      id:
        accepted.length === 1
          ? key
          : `${key}:amount:${Math.round(Math.abs(event.amount) * 100)}:${index}`,
    }))
    const pendingTransactions = values.filter((transaction) => transaction.pending)
    const matches = groupEvents.map((event) =>
      pendingTransactions.filter((transaction) => {
        const pendingDate = transactionDateKey(
          transaction.postedOn ?? transaction.date,
          data.updatedAt,
        )
        const amount = Math.abs(transaction.amount)
        const tolerance = Math.max(5, Math.abs(event.amount) * 0.25)
        return (
          Math.abs(daysBetween(event.date, pendingDate)) <= 5 &&
          amount >= event.amountLow - tolerance &&
          amount <= event.amountHigh + tolerance
        )
      }),
    )
    for (const [index, event] of groupEvents.entries()) {
      const match = matches[index].length === 1 ? matches[index][0] : undefined
      const unambiguous =
        match &&
        matches.every((other, otherIndex) => otherIndex === index || !other.includes(match))
      events.push(
        unambiguous
          ? {
              ...event,
              amount: money(match.amount),
              state: 'pending',
              matchedTransactionId: match.id,
            }
          : event,
      )
    }
  }
  return events.toSorted((left, right) => left.date.localeCompare(right.date)).slice(0, 16)
}

export function chatEvidence(data: FinanceSnapshot, today = data.updatedAt.slice(0, 10)) {
  const summary = moneySummary(data, 'month')
  return {
    savedAt: data.updatedAt,
    thisMonth: {
      income: summary.grossIncome,
      spendingAfterReimbursements: summary.personalSpending,
      fees: summary.fees,
      netCashFlow: summary.netCashFlow,
    },
    accounts: data.accounts
      .filter(({ id }) => id !== 'all')
      .map(({ name, institution, type, value }) => ({ name, institution, type, value })),
    holdings: data.holdings
      .toSorted((left, right) => (right.value ?? 0) - (left.value ?? 0))
      .slice(0, 12)
      .map(({ ticker, name, value, totalChangePct }) => ({ ticker, name, value, totalChangePct })),
    recentActivity: data.transactions.slice(0, 20).map((transaction) => ({
      merchant: transaction.merchant,
      category: transaction.category,
      date: transaction.date,
      amount: transaction.amount,
      pending: transaction.pending,
    })),
    upcoming: expectedMoneyEvents(data, today)
      .filter(({ state }) => state === 'expected')
      .slice(0, 8),
  }
}

export { isZelle } from './transaction-kind'
