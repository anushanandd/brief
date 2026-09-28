import { accountDisplayName, type AccountDisplayNames } from './account-name-preferences'
import type { FinanceSnapshot } from './schema'
import { getPlatinumBenefitActivity, transactionDateKey } from './spending'
import { incomeActivityGroup, moneyKind, transactionMarkKind } from './transaction-kind'

export const analyticsCharts = [
  { id: 'cash-flow', label: 'Cash flow' },
  { id: 'income', label: 'Income' },
  { id: 'amex-credits', label: 'Amex credits' },
  { id: 'dividends', label: 'Dividends' },
  { id: 'interest', label: 'Interest' },
  { id: 'fees', label: 'Fees' },
  { id: 'realized', label: 'Realized P/L' },
] as const
export type AnalyticsChart = (typeof analyticsCharts)[number]['id']
export type AnalyticsRange = 'week' | 'month' | 'quarter' | 'year' | 'all'
export const analyticsRanges: AnalyticsRange[] = ['week', 'month', 'quarter', 'year', 'all']
export type AnalyticsSearch = {
  chart?: AnalyticsChart
  range?: AnalyticsRange
  account?: string
  from?: string
  to?: string
}
export const analyticsChart = (value: unknown) => analyticsCharts.find(({ id }) => id === value)?.id

export function validDate(value: unknown) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return undefined
  const time = Date.parse(`${value}T00:00:00Z`)
  return Number.isFinite(time) && new Date(time).toISOString().slice(0, 10) === value
    ? value
    : undefined
}

export function analyticsSearch(search: Record<string, unknown>): AnalyticsSearch {
  return {
    chart: analyticsChart(search.chart),
    range: analyticsRanges.find((range) => range === search.range),
    account: typeof search.account === 'string' && search.account ? search.account : undefined,
    from: validDate(search.from),
    to: validDate(search.to),
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
  if (chart === 'amex-credits')
    return getPlatinumBenefitActivity(
      transactions.filter(
        ({ accountId: id }) => Boolean(spendingAccountId) && id === spendingAccountId,
      ),
      data.updatedAt,
    )
      .filter(({ kind }) => kind === 'credit' || kind === 'reversal')
      .map((transaction) => ({
        id: `spending:${transaction.id}`,
        date: transaction.date,
        value: transaction.amount,
        accountId: transaction.accountId,
        group: transaction.benefitName,
      }))

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
        other: 'Other',
      }[kind]
    }
    return [
      { id: `spending:${transaction.id}`, date, value, accountId: transaction.accountId, group },
    ]
  })
}

const dayMs = 86_400_000
const iso = (time: number) => new Date(time).toISOString().slice(0, 10)
const at = (date: string) => Date.parse(`${date}T00:00:00Z`)
export const localDateKey = (seconds: number) => {
  const date = new Date(seconds * 1000)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

export function analyticsWindow(data: FinanceSnapshot, search: AnalyticsSearch, today?: string) {
  const saved = transactionDateKey(data.updatedAt, data.updatedAt)
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
        : iso(at(end) - (rangeDays - 1) * dayMs))
  const days = Math.round((at(end) - at(start)) / dayMs) + 1
  const previousStart = iso(at(start) - Math.max(1, days) * dayMs)
  const previousEnd = iso(at(start) - dayMs)
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
    window.end <= window.savedEnd &&
    entries.some(({ date }) => date <= window.previousStart)
  const previous = comparable
    ? analyticsTotal(
        entries.filter(({ date }) => date >= window.previousStart && date <= window.previousEnd),
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
  const days = (at(window.end) - at(window.start)) / dayMs + 1
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
        ? at(cursor) + dayMs
        : Date.UTC(
            date.getUTCFullYear() + (unit === 'year' ? 1 : 0),
            unit === 'year' ? 0 : date.getUTCMonth() + 1,
            1,
          )
    const to = iso(Math.min(next - dayMs, at(window.end)))
    const items = selected.filter((entry) => entry.date >= cursor && entry.date <= to)
    buckets.push({
      from: cursor,
      to,
      value: cursor > window.savedEnd ? null : analyticsTotal(items),
      count: items.length,
      entries: items.toSorted((a, b) => b.date.localeCompare(a.date)),
    })
    cursor = iso(next)
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
      const cents = Math.round(entry.value! * 100)
      const start = cents < 0 ? negative : positive
      if (cents < 0) negative += cents
      else positive += cents
      return { ...entry, start: start / 100, end: (start + cents) / 100 }
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
