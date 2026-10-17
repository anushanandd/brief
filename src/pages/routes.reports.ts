/**
 * Reports and account totals for the route scenarios, in the shape Rust returns them. In the app
 * Rust computes them (`insights::reports` and `insights::rollup`, tested there); these tests
 * check only how pages present them, so this follows the same rules over the scenario.
 */
import { accountDisplayName } from '../lib/account-names'
import type { ReportRequest } from '../lib/api'
import { dayKey, dayMs, dayStart, validDate } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import type {
  Account,
  FinanceSettings,
  FinanceSnapshot,
  Reports,
  ReportEntry,
  ReportSummary,
  Rollup,
} from '../lib/schema'
import { transactionDateKey } from '../lib/spending'
import { incomeActivityGroup, splitParts } from '../lib/transaction-kind'

type Chart = ReportSummary['chart']
const charts: Chart[] = [
  'cash-flow',
  'income',
  'interest',
  'fees',
  'amex-credits',
  'dividends',
  'realized',
]
const kindLabels: Record<string, string> = {
  income: 'Income',
  dividend: 'Dividends',
  interest: 'Interest',
  reimbursement: 'Reimbursements',
  expense: 'Purchases',
  fee: 'Fees',
  tax: 'Taxes',
}
const cents = (value: number) => Math.round(value * 100)
const total = (values: Array<number | null>) =>
  values.some((value) => value == null)
    ? null
    : values.reduce<number>((sum, value) => sum + cents(value!), 0) / 100

function entries(data: FinanceSnapshot, chart: Chart, accounts?: string[]): ReportEntry[] {
  const settings = financeSettings(data)
  const usd = new Map(
    data.accounts.map(({ id, currency }) => [id, !currency || currency === 'USD']),
  )
  const eligible = (id?: string) =>
    (usd.get(id ?? '') ?? true) && (!accounts || Boolean(id && accounts.includes(id)))
  const end = transactionDateKey(data.updatedAt)
  if (chart === 'realized')
    return data.trades
      .filter(({ type, accountId }) => type.toUpperCase().includes('SELL') && eligible(accountId))
      .map((trade) => ({
        id: `trade:${trade.id}`,
        date: transactionDateKey(trade.date),
        value: trade.estimatedRealizedGain ?? null,
        accountId: trade.accountId,
        group: trade.ticker ?? 'Other sales',
      }))
      .filter(({ date }) => Boolean(validDate(date)) && date <= end)
  if (chart === 'amex-credits') {
    const benefits = data.insights?.benefits
    return (benefits && benefits.accountId === settings.spendingAccountId ? benefits.activity : [])
      .filter(
        ({ kind, pending, accountId }) =>
          !pending && eligible(accountId) && (kind === 'credit' || kind === 'reversal'),
      )
      .map((item) => ({
        id: `spending:${item.id}`,
        date: item.date,
        value: item.amount,
        accountId: item.accountId,
        group: item.benefitName,
      }))
  }
  return data.transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date)
    if (transaction.pending || !eligible(transaction.accountId) || !validDate(date) || date > end)
      return []
    const { mark, kind } = transaction.classification
    const accountName = accountDisplayName(
      transaction.accountId,
      transaction.account,
      settings.accountNames,
    )
    let value = transaction.amount
    let group: string
    if (chart === 'income') {
      if (!(value > 0 && ['income', 'dividend', 'interest'].includes(mark))) return []
      group = incomeActivityGroup(transaction)
    } else if (chart === 'dividends' || chart === 'interest') {
      if (value <= 0 || mark !== (chart === 'dividends' ? 'dividend' : 'interest')) return []
      group = accountName
    } else if (chart === 'fees') {
      if (kind !== 'fee') return []
      value = -value
      group = accountName
    } else {
      if (!kindLabels[kind]) return []
      if (['income', 'dividend', 'interest', 'reimbursement'].includes(kind) && value <= 0)
        return []
      group = kindLabels[kind]
    }
    return [
      { id: `spending:${transaction.id}`, date, value, accountId: transaction.accountId, group },
    ]
  })
}

/** The window, a report over it, and the rest as `insights::reports::reports` reads them. */
export function scenarioReports(data: FinanceSnapshot, request: ReportRequest): Reports {
  const saved = data.insights?.today ?? transactionDateKey(data.updatedAt)
  const end = request.to && request.to < saved ? request.to : saved
  const range = request.range ?? 'month'
  const dates = [
    ...data.transactions.map((t) => transactionDateKey(t.postedOn ?? t.date)),
    ...data.trades.map((t) => transactionDateKey(t.date)),
  ]
    .filter((date) => Boolean(validDate(date)) && date <= end)
    .toSorted()
  const lengths = { day: 1, week: 7, month: 30, quarter: 90, year: 0, all: 0 }
  const start =
    request.from ??
    (range === 'all'
      ? (dates[0] ?? end)
      : range === 'year'
        ? `${end.slice(0, 4)}-01-01`
        : dayKey(dayStart(end) - (lengths[range] - 1) * dayMs))
  const days = Math.round((dayStart(end) - dayStart(start)) / dayMs) + 1
  const window = {
    start,
    end,
    savedEnd: saved,
    previousStart: dayKey(dayStart(start) - Math.max(1, days) * dayMs),
    previousEnd: dayKey(dayStart(start) - dayMs),
    range,
    custom: Boolean(request.from || request.to),
    valid: days > 0,
  }
  const report = (chart: Chart, all: ReportEntry[]) => {
    const selected = all.filter(({ date }) => date >= window.start && date <= window.end)
    const sum = total(selected.map(({ value }) => value))
    const comparable =
      window.range !== 'all' &&
      window.savedEnd >= window.start &&
      all.some(({ date }) => date <= window.previousStart)
    const previous = comparable
      ? total(
          all
            .filter(({ date }) => date >= window.previousStart && date <= window.previousEnd)
            .map(({ value }) => value),
        )
      : null
    const groups = new Map<string, ReportEntry[]>()
    for (const entry of selected)
      groups.set(entry.group, [...(groups.get(entry.group) ?? []), entry])
    const unit =
      days <= 45 ? ('day' as const) : days > 3 * 366 ? ('year' as const) : ('month' as const)
    const buckets = []
    let cursor = window.start
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
      const items = selected
        .filter((entry) => entry.date >= cursor && entry.date <= to)
        .toSorted((a, b) => b.date.localeCompare(a.date))
      const known = items.every(({ value }) => value != null)
      const sides = new Map<string, { total: number; entryIds: string[] }>()
      for (const entry of items.toSorted(
        (a, b) =>
          (a.accountId ?? '').localeCompare(b.accountId ?? '') || a.date.localeCompare(b.date),
      )) {
        if (!known || !entry.value) continue
        const key = `${entry.group}\u0000${entry.value < 0}`
        const side = sides.get(key) ?? { total: 0, entryIds: [] }
        side.total += cents(entry.value)
        side.entryIds.push(entry.id)
        sides.set(key, side)
      }
      buckets.push({
        from: cursor,
        to,
        value: cursor > window.savedEnd ? null : total(items.map(({ value }) => value)),
        count: items.length,
        entryIds: items.map(({ id }) => id),
        positive: known ? (total(items.map(({ value }) => Math.max(0, value!))) ?? 0) : 0,
        negative: known ? (total(items.map(({ value }) => Math.min(0, value!))) ?? 0) : 0,
        groups: [...sides]
          .map(([key, side]) => {
            const [name, negative] = key.split('\u0000')
            return {
              name,
              negative: negative === 'true',
              total: side.total / 100,
              entryIds: side.entryIds,
            }
          })
          .toSorted(
            (a, b) => a.name.localeCompare(b.name) || Number(a.negative) - Number(b.negative),
          ),
      })
      cursor = dayKey(next)
    }
    const largest = selected.reduce<number | null>(
      (current, { value }) =>
        current == null || Math.abs(value ?? 0) > Math.abs(current) ? (value ?? null) : current,
      null,
    )
    return {
      chart,
      total: sum,
      previous,
      change: sum != null && previous != null ? (cents(sum) - cents(previous)) / 100 : null,
      percent:
        sum != null && previous
          ? Math.round(((sum - previous) / Math.abs(previous)) * 10_000) / 100
          : null,
      entries: selected,
      average:
        selected.length && sum != null ? Math.round((sum / selected.length) * 100) / 100 : null,
      largest: sum == null ? null : largest,
      activeDays: new Set(selected.map(({ date }) => date)).size,
      breakdown: [...groups]
        .map(([label, items]) => ({
          label,
          value: total(items.map(({ value }) => value)),
          count: items.length,
        }))
        .toSorted((a, b) => Math.abs(b.value ?? 0) - Math.abs(a.value ?? 0)),
      buckets,
      unit,
    }
  }
  const summaries = charts.map((chart) => {
    const {
      entries: _entries,
      average,
      largest,
      activeDays,
      breakdown,
      buckets,
      unit,
      ...summary
    } = report(chart, entries(data, chart, request.accounts))
    void [_entries, average, largest, activeDays, breakdown, buckets, unit]
    return summary
  })
  if (!request.chart) return { window, summaries, accounts: [], groups: [], recurringIds: [] }
  const scoped = entries(data, request.chart, request.accounts)
  const full = report(request.chart, scoped)
  const everywhere = entries(data, request.chart)
  const inWindow = report(request.chart, everywhere).entries
  const ids = new Set(scoped.map(({ id }) => id))
  return {
    window,
    summaries,
    report: full,
    accounts: [...new Set(inWindow.flatMap(({ accountId }) => (accountId ? [accountId] : [])))]
      .filter((id) => data.accounts.some((account) => account.id === id))
      .map((accountId) => ({
        accountId,
        total: total(
          inWindow.filter((entry) => entry.accountId === accountId).map(({ value }) => value),
        ),
      })),
    groups: [...new Set(everywhere.map(({ group }) => group))].toSorted(),
    recurringIds: [
      ...new Set(
        data.transactions.flatMap(({ id, recurringId }) =>
          recurringId && ids.has(`spending:${id}`) ? [recurringId] : [],
        ),
      ),
    ],
    ...(request.chart === 'cash-flow' ? { flow: cashFlow(data, full.entries) } : {}),
  }
}

type FlowChild = { id: string; label: string; value: number; date: string }
const add = (groups: Map<string, FlowChild[]>, group: string, child: FlowChild) =>
  groups.set(group, [...(groups.get(group) ?? []), child])

function cashFlow(data: FinanceSnapshot, selected: ReportEntry[]): NonNullable<Reports['flow']> {
  const ids = new Set(selected.map(({ id }) => id))
  const sources = new Map<string, FlowChild[]>()
  const purchases = new Map<string, FlowChild[]>()
  let fees = 0
  let taxes = 0
  for (const transaction of data.transactions) {
    if (!ids.has(`spending:${transaction.id}`)) continue
    const { kind } = transaction.classification
    const date = (transaction.postedOn ?? transaction.date).slice(0, 10)
    const amount = cents(Math.abs(transaction.amount))
    if (
      transaction.amount > 0 &&
      ['income', 'dividend', 'interest', 'reimbursement', 'fee', 'tax'].includes(kind)
    ) {
      const source = transaction.merchant.trim() || 'Unknown source'
      add(sources, source, {
        id: `source-transaction:${transaction.id}`,
        label: transaction.description?.trim() || source,
        value: amount / 100,
        date,
      })
    } else if (kind === 'expense') {
      splitParts(transaction).forEach((part, index) =>
        add(purchases, part.category.trim() || 'Other', {
          id: `purchase-transaction:${transaction.id}${index ? `:${index}` : ''}`,
          label: transaction.merchant.trim() || 'Purchase',
          value: cents(Math.abs(part.amount)) / 100,
          date,
        }),
      )
    } else if (kind === 'fee') fees += amount
    else if (kind === 'tax') taxes += amount
  }
  const items = (groups: typeof sources, prefix: string) =>
    [...groups]
      .map(([label, children]) => ({
        id: `${prefix}:${label}`,
        label,
        value: total(children.map(({ value }) => value)) ?? 0,
        children: children.toSorted((a, b) => b.date.localeCompare(a.date) || b.value - a.value),
      }))
      .toSorted((a, b) => b.value - a.value || a.label.localeCompare(b.label))
  const sourceItems = items(sources, 'source')
  const purchaseItems = items(purchases, 'purchase')
  const inflow = sourceItems.reduce((sum, item) => sum + cents(item.value), 0)
  const outflow = purchaseItems.reduce((sum, item) => sum + cents(item.value), 0) + fees + taxes
  return {
    sources: sourceItems,
    purchases: purchaseItems,
    fees: fees / 100,
    taxes: taxes / 100,
    inflow: inflow / 100,
    outflow: outflow / 100,
    gap: Math.max(0, outflow - inflow) / 100,
    retained: Math.max(0, inflow - outflow) / 100,
  }
}

type RolledAccount = Pick<
  Account,
  'id' | 'type' | 'value' | 'knownUnrealizedGain' | 'estimatedRealizedGainYtd'
>

/** Account totals as `insights::rollup::rollup` reads them, leaving out the spending cards. */
export function scenarioRollup(
  accounts: RolledAccount[],
  holdings: FinanceSnapshot['holdings'],
  netWorth: number,
  settings: Pick<FinanceSettings, 'spendingAccountId' | 'spendingCards'>,
): Rollup {
  const spending = new Set(
    [settings.spendingAccountId, ...(settings.spendingCards ?? [])].filter(Boolean),
  )
  const isCard = (account: RolledAccount) => account.type === 'credit' && spending.has(account.id)
  const every = accounts.filter(({ id }) => id !== 'all')
  const listed = every.filter((account) => !isCard(account))
  const cards = every.filter(isCard)
  const groups = (members: RolledAccount[]) =>
    [...new Set(members.map(({ type }) => type))]
      .map((type) => ({
        type,
        value: total(
          members
            .filter((account) => account.type === type)
            .filter(({ id, value }) => value != null || !id.startsWith('manual:'))
            .map(({ value }) => value),
        ),
      }))
      .toSorted((a, b) => (b.value ?? -Infinity) - (a.value ?? -Infinity))
  const positive = groups(every.filter(({ value }) => value != null && value > 0))
  const positiveTotal = positive.reduce((sum, { value }) => sum + cents(value ?? 0), 0)
  const values = listed.flatMap(({ value }) => (value == null ? [] : [value]))
  const investments = accounts.filter(({ type }) => type === 'brokerage' || type === 'retirement')
  const known = (picked: Array<number | null | undefined>) => {
    const present = picked.filter((value): value is number => value != null)
    return present.length ? total(present) : null
  }
  const cardTotal = total(cards.map(({ value }) => value))
  const withoutCards = (value: number) =>
    cardTotal == null ? value : (cents(value) - cents(cardTotal)) / 100
  const combined = accounts.find(({ id }) => id === 'all')?.value
  return {
    groups: groups(every),
    listed: groups(listed),
    ...(cards.length ? { cards: { type: 'spending', value: cardTotal } } : {}),
    shares: positive.map(({ type, value }) => ({
      type,
      value: value ?? 0,
      percent: positiveTotal ? Math.round((cents(value ?? 0) / positiveTotal) * 10_000) / 100 : 0,
    })),
    sheet: {
      assets: total(values.filter((value) => value > 0)) ?? 0,
      liabilities: total(values.filter((value) => value < 0)) ?? 0,
      complete: values.length === listed.length,
    },
    unrealized: known(investments.map(({ knownUnrealizedGain }) => knownUnrealizedGain)),
    realizedYtd: known(investments.map(({ estimatedRealizedGainYtd }) => estimatedRealizedGainYtd)),
    retirement: total(
      investments.filter(({ type }) => type === 'retirement').map(({ value }) => value),
    ),
    netWorth: withoutCards(netWorth),
    combined: combined == null ? null : withoutCards(combined),
    holdings: [...new Set(holdings.map(({ ticker }) => ticker))].map((ticker) => {
      const positions = holdings.filter((holding) => holding.ticker === ticker)
      const value = total(positions.map((position) => position.value))
      return {
        ticker,
        shares: total(positions.map((position) => position.shares ?? null)),
        vestedShares: null,
        value,
        basis: total(positions.map((position) => position.costBasis)),
        totalChangePct: null,
        averageBasis: null,
        weight: null,
        dailyChangePct: positions[0]?.dailyChangePct ?? null,
        weeklyChangePct: positions[0]?.weeklyChangePct ?? null,
        accounts: [...new Set(positions.map(({ accountId }) => accountId))].map((accountId) => ({
          accountId,
          shares: total(
            positions.filter((p) => p.accountId === accountId).map((p) => p.shares ?? null),
          ),
          value: total(positions.filter((p) => p.accountId === accountId).map((p) => p.value)),
        })),
      }
    }),
  }
}
