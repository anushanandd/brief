import { z } from 'zod'

import { formatCurrency } from './format'

/** One read-only ledger query and the figures native code returned for it. */
export const ledgerCallSchema = z.object({
  name: z.string(),
  arguments: z.record(z.string(), z.unknown()),
  result: z.record(z.string(), z.unknown()),
})
export type LedgerCall = z.infer<typeof ledgerCallSchema>

export type LedgerCell = { text: string; sensitive: boolean }
export type LedgerView = {
  title: string
  scope: string
  facts: Array<{ label: string } & LedgerCell>
  table?: { columns: string[]; rows: LedgerCell[][]; more: number }
}

const titles: Record<string, string> = {
  listAccounts: 'Accounts',
  spendingByCategory: 'Spending by category',
  cashFlow: 'Cash flow',
  searchTransactions: 'Transactions',
  merchantHistory: 'Merchant history',
  upcomingActivity: 'Expected activity',
  monthPlan: 'This month',
  topHoldings: 'Largest holdings',
  creditCards: 'Credit cards',
  taxSummary: 'Taxes',
  trips: 'Trips',
  subscriptions: 'Subscriptions',
  dividendIncome: 'Dividend income',
}
const money =
  /amount|spending|income|balance|fees|taxes|cash|flow|value|spent|projected|typical|outflows|inflows|total|limit|interest|dividends|gains|disallowed|loss|tax(low|high)/i
const percent = /percent/i
const hidden = new Set(['start', 'end', 'from', 'savedAt', 'throughDate', 'monthStart'])
const MAX_ROWS = 6
const MAX_COLUMNS = 4

const label = (key: string) => {
  const words = key.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase()
  return words.charAt(0).toUpperCase() + words.slice(1)
}

/** Formats one native figure. Amounts are marked so screen privacy can mask them. */
function cell(key: string, value: unknown): LedgerCell | undefined {
  if (value == null) return { text: '—', sensitive: false }
  if (typeof value === 'number') {
    if (key === 'year') return { text: String(value), sensitive: false }
    if (percent.test(key)) return { text: `${value.toFixed(2)}%`, sensitive: false }
    if (money.test(key)) return { text: formatCurrency(value), sensitive: true }
    return { text: value.toLocaleString('en-US'), sensitive: false }
  }
  if (typeof value === 'string') return { text: value, sensitive: false }
  if (typeof value === 'boolean') return { text: value ? 'Yes' : 'No', sensitive: false }
  return undefined
}

const isRow = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/**
 * Lays out a ledger result for reading: its single figures, then its first list as a short
 * table. Nothing is computed; values are shown as native code returned them.
 */
export function ledgerView({ name, arguments: args, result }: LedgerCall): LedgerView {
  const text = (key: string) => (typeof result[key] === 'string' ? result[key] : undefined)
  const start = text('start') ?? text('from')
  const end = text('end') ?? text('throughDate')
  const subject = [args.merchant, args.text, args.category].find(
    (value): value is string => typeof value === 'string' && value.trim().length > 0,
  )
  const facts = Object.entries(result).flatMap(([key, value]) => {
    if (hidden.has(key) || typeof value === 'object') return []
    const formatted = cell(key, value)
    return formatted ? [{ label: label(key), ...formatted }] : []
  })
  const list = Object.values(result).find(
    (value): value is Record<string, unknown>[] =>
      Array.isArray(value) && value.length > 0 && value.every(isRow),
  )
  const columns = list
    ? Object.keys(list[0])
        .filter((key) => cell(key, list[0][key]) !== undefined)
        .slice(0, MAX_COLUMNS)
    : []
  return {
    title: titles[name] ?? label(name),
    scope: [subject, start && end ? `${start} to ${end}` : (start ?? end)]
      .filter(Boolean)
      .join(' · '),
    facts,
    ...(list && columns.length
      ? {
          table: {
            columns: columns.map(label),
            rows: list
              .slice(0, MAX_ROWS)
              .map((row) =>
                columns.map((key) => cell(key, row[key]) ?? { text: '—', sensitive: false }),
              ),
            more: Math.max(0, list.length - MAX_ROWS),
          },
        }
      : {}),
  }
}
