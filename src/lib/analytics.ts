import { accountDisplayName, type AccountDisplayNames } from './account-names'
import { dayKey, dayMs, dayStart, localDateKey, validDate } from './dates'
import { type ChartPeriod, chartPeriods } from './graph-preferences'
import { cents } from './money'
import type { FinanceSnapshot } from './schema'
import { transactionDateKey } from './spending'
import { incomeActivityGroup, moneyKind, transactionMarkKind } from './transaction-kind'

/** A report and how its page reads. */
type ReportDefinition = {
  id: string
  label: string
  /** Money, beside Spending, or Investments. */
  group: 'money' | 'investments'
  /** The chart's heading, when it says more than the label. */
  heading?: string
  /** Whether a smaller total is the better change, as with fees. */
  lowerIsBetter?: boolean
  /** Payers are listed by a short name, as the chart lists them. */
  briefTitles?: boolean
  /** The saved spending card's credits: its benefits and credit activity, never other accounts. */
  benefits?: boolean
  /** Money in and out drawn as a flow instead of bars. */
  flow?: boolean
  /** What the report's records are called in its figures. */
  records?: { count: string; average: string; largest: string }
}

// Money's strip lists its reports in this order, after Spending and Recurring.
export const analyticsCharts = [
  { id: 'cash-flow', label: 'Cash flow', group: 'money', flow: true },
  { id: 'income', label: 'Income', group: 'money', briefTitles: true },
  { id: 'interest', label: 'Interest', group: 'money' },
  { id: 'fees', label: 'Fees', group: 'money', lowerIsBetter: true },
  { id: 'amex-credits', label: 'Benefits', group: 'money', benefits: true },
  { id: 'dividends', label: 'Dividends', group: 'investments' },
  {
    id: 'realized',
    label: 'Realized P/L',
    group: 'investments',
    heading: 'Estimated realized P/L',
    records: { count: 'Imported sales', average: 'Average sale P/L', largest: 'Largest sale P/L' },
  },
] as const satisfies readonly ReportDefinition[]
export type AnalyticsChart = (typeof analyticsCharts)[number]['id']
export type AnalyticsGroup = (typeof analyticsCharts)[number]['group']
export const analyticsDefinition = (chart: AnalyticsChart): ReportDefinition =>
  analyticsCharts.find(({ id }) => id === chart)!
export const analyticsGroup = (chart: AnalyticsChart): AnalyticsGroup =>
  analyticsDefinition(chart).group
export type AnalyticsSearch = {
  chart?: AnalyticsChart
  range?: ChartPeriod
  account?: string
  from?: string
  to?: string
  /** The lower card lists the report's transactions instead of its breakdown. */
  view?: 'transactions'
  /** One breakdown group the transactions are narrowed to. */
  group?: string
}

/** A report's search without its lower card's view, for a link to another report. */
export const withoutReportView = ({ view: _view, group: _group, ...search }: AnalyticsSearch) =>
  search
export const analyticsChart = (value: unknown) => analyticsCharts.find(({ id }) => id === value)?.id

export function analyticsSearch(search: Record<string, unknown>): AnalyticsSearch {
  return {
    chart: analyticsChart(search.chart),
    range: chartPeriods.find((range) => range === search.range),
    account: typeof search.account === 'string' && search.account ? search.account : undefined,
    from: validDate(search.from),
    to: validDate(search.to),
    ...(search.view === 'transactions' ? { view: 'transactions' as const } : {}),
    ...(search.view === 'transactions' && typeof search.group === 'string' && search.group
      ? { group: search.group }
      : {}),
  }
}

// An explicit empty selection must never expand to all accounts in Activity.
export const noAnalyticsAccounts = '__none__'
export const analyticsAccountMatches = (scope: string | undefined, id?: string) =>
  scope === undefined || Boolean(id && scope.split(',').includes(id))

export function toggleAnalyticsAccount(scope: string | undefined, id: string, available: string[]) {
  const selected = new Set(scope === undefined ? available : scope.split(','))
  selected.delete(noAnalyticsAccounts)
  if (selected.has(id)) selected.delete(id)
  else selected.add(id)
  return selected.size ? [...selected].join(',') : noAnalyticsAccounts
}

export type AnalyticsEntry = {
  id: string
  date: string
  value: number | null
  accountId?: string
  group: string
}

// Presentation totals over the immutable native ledger; values are summed in minor units.
export const analyticsTotal = (entries: Pick<AnalyticsEntry, 'value'>[]) =>
  entries.some(({ value }) => value == null)
    ? null
    : entries.reduce((sum, { value }) => sum + Math.round(value! * 100), 0) / 100

export function analyticsEntries(
  data: FinanceSnapshot,
  chart: AnalyticsChart,
  spendingAccountId: string,
  accountId?: string,
  accountNames: AccountDisplayNames = {},
): AnalyticsEntry[] {
  const accounts = new Map(data.accounts.map((account) => [account.id, account]))
  const eligible = (id?: string) =>
    analyticsAccountMatches(accountId, id) &&
    (!accounts.get(id ?? '')?.currency || accounts.get(id ?? '')?.currency === 'USD')
  const end = transactionDateKey(data.updatedAt, data.updatedAt)
  if (chart === 'realized')
    return data.trades
      .filter((trade) => trade.type.toUpperCase().includes('SELL') && eligible(trade.accountId))
      .map((trade) => ({
        id: `trade:${trade.id}`,
        date: transactionDateKey(trade.date, data.updatedAt),
        value: trade.estimatedRealizedGain ?? null,
        accountId: trade.accountId,
        group: trade.ticker ?? 'Other sales',
      }))
      .filter(({ date }) => Boolean(validDate(date)) && date <= end)

  const transactions = data.transactions.filter(
    (transaction) => !transaction.pending && eligible(transaction.accountId),
  )
  if (chart === 'amex-credits') {
    // Native benefit rules classify the saved card's activity; this only scopes it.
    const benefits = data.insights?.benefits
    return (
      benefits && spendingAccountId && benefits.accountId === spendingAccountId
        ? benefits.activity
        : []
    )
      .filter(
        ({ kind, pending, accountId: id }) =>
          !pending && eligible(id) && (kind === 'credit' || kind === 'reversal'),
      )
      .map((transaction) => ({
        id: `spending:${transaction.id}`,
        date: transaction.date,
        value: transaction.amount,
        accountId: transaction.accountId,
        group: transaction.benefitName,
      }))
  }

  return transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
    if (!validDate(date) || date > end) return []
    const mark = transactionMarkKind(transaction)
    const kind = moneyKind(transaction)
    const positiveIncome =
      transaction.amount > 0 && ['income', 'dividend', 'interest'].includes(mark)
    let group = transaction.merchant
    let value = transaction.amount
    if (chart === 'income') {
      if (!positiveIncome) return []
      group = incomeActivityGroup(transaction)
    } else if (chart === 'dividends' || chart === 'interest') {
      if (transaction.amount <= 0 || mark !== (chart === 'dividends' ? 'dividend' : 'interest'))
        return []
      group = accountDisplayName(transaction.accountId, transaction.account, accountNames)
    } else if (chart === 'fees') {
      if (kind !== 'fee') return []
      value = -transaction.amount
      group = accountDisplayName(transaction.accountId, transaction.account, accountNames)
    } else {
      if (
        !['income', 'dividend', 'interest', 'reimbursement', 'expense', 'fee', 'tax'].includes(kind)
      )
        return []
      if (
        ['income', 'dividend', 'interest', 'reimbursement'].includes(kind) &&
        transaction.amount <= 0
      )
        return []
      group = {
        income: 'Income',
        dividend: 'Dividends',
        interest: 'Interest',
        reimbursement: 'Reimbursements',
        expense: 'Purchases',
        fee: 'Fees',
        tax: 'Taxes',
        transfer: 'Transfers',
        owed: 'For others',
        other: 'Other',
      }[kind]
    }
    return [
      { id: `spending:${transaction.id}`, date, value, accountId: transaction.accountId, group },
    ]
  })
}

export function analyticsWindow(data: FinanceSnapshot, search: AnalyticsSearch, today?: string) {
  const saved = localDateKey(Date.parse(data.updatedAt) / 1000)
  const current = today ?? saved
  const end = search.to && search.to < current ? search.to : current
  const dates = [
    ...data.transactions.map((t) => transactionDateKey(t.postedOn ?? t.date, data.updatedAt)),
    ...data.trades.map((t) => transactionDateKey(t.date, data.updatedAt)),
  ]
    .filter((date) => Boolean(validDate(date)) && date <= end)
    .toSorted()
  const range = search.range ?? 'month'
  const rangeDays = range === 'week' ? 7 : range === 'quarter' ? 90 : 30
  const start =
    search.from ??
    (range === 'all'
      ? (dates[0] ?? end)
      : range === 'year'
        ? `${end.slice(0, 4)}-01-01`
        : dayKey(dayStart(end) - (rangeDays - 1) * dayMs))
  const days = Math.round((dayStart(end) - dayStart(start)) / dayMs) + 1
  const previousStart = dayKey(dayStart(start) - Math.max(1, days) * dayMs)
  const previousEnd = dayKey(dayStart(start) - dayMs)
  return {
    start,
    end,
    savedEnd: saved,
    previousStart,
    previousEnd,
    range,
    custom: Boolean(search.from || search.to),
    valid: days > 0,
  }
}
export type AnalyticsWindow = ReturnType<typeof analyticsWindow>

export function analyticsReport(entries: AnalyticsEntry[], window: AnalyticsWindow) {
  const selected = entries.filter(({ date }) => date >= window.start && date <= window.end)
  const total = analyticsTotal(selected)
  // An older imported record supports an imported-period comparison, not complete bank coverage.
  const comparable =
    window.range !== 'all' &&
    window.savedEnd >= window.start &&
    entries.some(({ date }) => date <= window.previousStart)
  // Past a stale snapshot, compare only the days the snapshot covers against as many prior days.
  const previousEnd =
    window.end <= window.savedEnd
      ? window.previousEnd
      : dayKey(dayStart(window.previousStart) + dayStart(window.savedEnd) - dayStart(window.start))
  const previous = comparable
    ? analyticsTotal(
        entries.filter(({ date }) => date >= window.previousStart && date <= previousEnd),
      )
    : null
  const percent =
    total != null && previous != null && previous !== 0
      ? ((total - previous) / Math.abs(previous)) * 100
      : null
  const groups = new Map<string, AnalyticsEntry[]>()
  selected.forEach((entry) => {
    const group = groups.get(entry.group) ?? []
    group.push(entry)
    groups.set(entry.group, group)
  })
  const breakdown = [...groups]
    .map(([label, items]) => ({ label, value: analyticsTotal(items), count: items.length }))
    .toSorted((a, b) => Math.abs(b.value ?? 0) - Math.abs(a.value ?? 0))
  const days = (dayStart(window.end) - dayStart(window.start)) / dayMs + 1
  const unit = days <= 45 ? 'day' : days > 3 * 366 ? 'year' : 'month'
  const buckets: Array<{
    from: string
    to: string
    value: number | null
    count: number
    entries: AnalyticsEntry[]
  }> = []
  let cursor = window.start
  // Calendar buckets are bounded by the selected window; no future periods or padded lifetime history.
  while (window.valid && cursor <= window.end) {
    const date = new Date(`${cursor}T00:00:00Z`)
    const next =
      unit === 'day'
        ? dayStart(cursor) + dayMs
        : Date.UTC(
            date.getUTCFullYear() + (unit === 'year' ? 1 : 0),
            unit === 'year' ? 0 : date.getUTCMonth() + 1,
            1,
          )
    const to = dayKey(Math.min(next - dayMs, dayStart(window.end)))
    const items = selected.filter((entry) => entry.date >= cursor && entry.date <= to)
    buckets.push({
      from: cursor,
      to,
      value: cursor > window.savedEnd ? null : analyticsTotal(items),
      count: items.length,
      entries: items.toSorted((a, b) => b.date.localeCompare(a.date)),
    })
    cursor = dayKey(next)
  }
  const average = selected.length && total != null ? total / selected.length : null
  const largest =
    total == null
      ? null
      : selected.reduce<number | null>(
          (current, { value }) =>
            current == null || Math.abs(value!) > Math.abs(current) ? value : current,
          null,
        )
  const activeDays = new Set(selected.map(({ date }) => date)).size
  return {
    entries: selected,
    total,
    previous,
    percent,
    average,
    largest,
    activeDays,
    breakdown,
    buckets,
    unit,
  }
}

export function analyticsBarStack(entries: AnalyticsEntry[]) {
  if (entries.some(({ value }) => value == null)) return { positive: 0, negative: 0, segments: [] }
  let positive = 0
  let negative = 0
  const segments = entries
    .toSorted(
      (a, b) =>
        a.group.localeCompare(b.group) ||
        (a.accountId ?? '').localeCompare(b.accountId ?? '') ||
        a.date.localeCompare(b.date) ||
        a.id.localeCompare(b.id),
    )
    .map((entry) => {
      const value = cents(entry.value!)
      const start = value < 0 ? negative : positive
      if (value < 0) negative += value
      else positive += value
      return { ...entry, start: start / 100, end: (start + value) / 100 }
    })
  return { positive: positive / 100, negative: negative / 100, segments }
}

/** Build from the full chart domain, before date and account filtering. */
export function analyticsGroupColors(groups: string[]) {
  return new Map(
    [...new Set(groups)]
      .toSorted()
      .map((group, index) => [
        group,
        index < 7
          ? `var(--data-${index + 1})`
          : `oklch(72% 0.14 ${((index - 7) * 137.508 + 20) % 360})`,
      ]),
  )
}
