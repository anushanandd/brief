import type { ReportRequest } from './api'
import { validDate } from './dates'
import { type ChartPeriod, chartPeriods } from './graph-preferences'
import type { Report } from './schema'

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
  /** On Benefits: the benefit open in the inspector, and the first day of its period. */
  benefit?: string
  period?: string
  /** On Benefits: the matches waiting for a decision, open in the inspector. */
  review?: true
}

/** A report's search without its lower card's view or inspector, for a link to another report. */
export const withoutReportView = ({
  view: _view,
  group: _group,
  benefit: _benefit,
  period: _period,
  review: _review,
  ...search
}: AnalyticsSearch) => search
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
    ...(typeof search.benefit === 'string' && /^[a-z0-9-]{1,64}$/.test(search.benefit)
      ? {
          benefit: search.benefit,
          ...(validDate(search.period) ? { period: validDate(search.period) } : {}),
        }
      : {}),
    ...(search.review === true || search.review === 'true' ? { review: true as const } : {}),
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

/**
 * What a report's search asks Rust for: its window (a chart period, the month by default, or the
 * custom days), its accounts (an explicit empty choice is none), and the report to read in full.
 */
export function reportRequest(search: AnalyticsSearch, chart?: AnalyticsChart): ReportRequest {
  return {
    ...(chart ? { chart } : {}),
    range: search.range ?? 'month',
    ...(search.from ? { from: search.from } : {}),
    ...(search.to ? { to: search.to } : {}),
    ...(search.account === undefined
      ? {}
      : { accounts: search.account.split(',').filter((id) => id && id !== noAnalyticsAccounts) }),
  }
}

/** A report with nothing in it, for while Rust has none to show. */
export const emptyReport = (chart: AnalyticsChart): Report => ({
  chart,
  total: null,
  previous: null,
  change: null,
  percent: null,
  entries: [],
  average: null,
  largest: null,
  activeDays: 0,
  breakdown: [],
  buckets: [],
  unit: 'day',
})

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
