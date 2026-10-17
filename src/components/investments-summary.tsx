import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import { useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { usePortfolioSectors } from '../hooks/use-portfolio-sectors'
import { useReports } from '../hooks/use-reports'
import { accountDisplayName } from '../lib/account-names'
import { getEarningsCalendar, getPortfolioToday, isTauri } from '../lib/api'
import { addDays, localDateKey } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  formatCurrency,
  formatPercent,
  formatProseCurrency,
  formatProsePercent,
  formatShare,
  formatSignedCurrency,
  pluralize,
  valueTone,
  formatShortDay,
} from '../lib/format'
import { useGraphWindow, type ChartPeriod, chartPeriodSeconds } from '../lib/graph-preferences'
import { rangeDates } from '../lib/investment-range'
import type { FinanceSnapshot, InvestmentSummary, ReportSummary, Statement } from '../lib/schema'
import { ChartRangeSelector } from './charts'
import { ExternalLink } from './external-link'
import { AlertCircle, ChevronRight } from './icons'
import { SecurityMark } from './security-mark'
import { Card, ChartChange, EmptyState, Metric, SectionHeading } from './ui'

const rangeNames: Record<ChartPeriod, string> = {
  day: 'Today',
  week: 'The past week',
  month: 'The past month',
  quarter: 'The past three months',
  year: 'The past year',
  all: 'All time',
}
// What's ahead looks a month ahead and lists the soonest few.
const COMING_DAYS = 31
const COMING_LISTED = 5

const money = (value: number) => formatProseCurrency(Math.abs(value))
const percent = formatProsePercent
/** Today's move at live prices, for the standout holding and its explanation. */
export function usePortfolioToday(revision: number | undefined, connected: boolean) {
  const native = typeof window !== 'undefined' && isTauri()
  return useQuery({
    queryKey: ['portfolio-today', revision],
    queryFn: getPortfolioToday,
    enabled: native && connected && revision != null,
    staleTime: 60_000,
    refetchInterval: 120_000,
    retry: false,
  })
}

/**
 * The page's one period, chosen from the main chart card with the app's chart range control.
 * It is the period Home and Accounts share, and every card here follows it.
 */
export function InvestmentRangeSelect() {
  const [graphWindow, setGraphWindow] = useGraphWindow()
  return <ChartRangeSelector value={graphWindow} onValueChange={setGraphWindow} />
}

/** D, W, M, Q, Y, and A choose the period, as on every page with a chart range. */
export function useInvestmentRangeShortcuts() {
  const [, setGraphWindow] = useGraphWindow()
  useGraphWindowShortcuts(setGraphWindow)
}

/** The combined brokerages' statement for the page's period, reconciled in Rust. */
export function periodStatement(data: FinanceSnapshot, range: ChartPeriod) {
  return data.insights?.periods.find(({ window }) => window === range)
}

/** The period's change as on every chart: the investment gain, then the time-weighted return. */
export function PeriodChange({ span }: { span?: Statement }) {
  return (
    <div className="chart-summary-row">
      <ChartChange amount={span?.investmentGain ?? null} percent={span?.timeWeightedPct ?? null} />
    </div>
  )
}

/** A difference in points, signed with a true minus. */
export const formatPoints = (value: number) =>
  `${value > 0 ? '+' : value < 0 ? '−' : ''}${Math.abs(value).toFixed(2)} pts`

// Interest belongs to Money: it is mostly cash and savings accounts.
const reports = [
  { chart: 'dividends', label: 'Dividends' },
  { chart: 'realized', label: 'Realized P/L' },
] as const

/** One report's total over the page's period, opening that report for the same days. */
function ReportMetric({
  chart,
  label,
  summary,
  from,
  to,
  accountId,
}: {
  chart: (typeof reports)[number]['chart']
  label: string
  /** Rust's total for the same days; absent until it has one. */
  summary?: ReportSummary
  from?: string
  to: string
  accountId?: string
}) {
  // A sale without a known cost has no gain, so the total is unavailable rather than partial.
  const total = summary?.total ?? null
  const signed = chart === 'realized'
  return (
    <Link
      className="metric investments-report-metric"
      to="/analytics"
      search={{ chart, ...(from ? { from, to } : { range: 'all' }), account: accountId }}
      data-keyboard-row
      data-keyboard-open
    >
      <span>{label}</span>
      <strong className={total == null ? 'muted' : signed ? valueTone(total) : undefined}>
        {!summary ? (
          '—'
        ) : total == null ? (
          <span title="A sale in this period has no known cost">Unavailable</span>
        ) : (
          <span className="sensitive-value">
            {signed ? formatSignedCurrency(total) : formatCurrency(total)}
          </span>
        )}
      </strong>
    </Link>
  )
}

/** This tax year's estimate, labeled with its year since taxes keep their own period. */
function TaxesMetric({ data }: { data: FinanceSnapshot }) {
  const year = data.insights?.taxes.years[0]
  const estimate = year?.estimate
  return (
    <Link
      className="metric investments-report-metric"
      to="/taxes"
      data-keyboard-row
      data-keyboard-open
    >
      <span>{year ? `Taxes · ${year.year}` : 'Taxes'}</span>
      <strong className={estimate ? undefined : 'muted'}>
        {estimate ? (
          <span className="sensitive-value">
            {estimate.low === estimate.high
              ? formatCurrency(estimate.high)
              : `${formatCurrency(estimate.low)}–${formatCurrency(estimate.high)}`}
          </span>
        ) : year ? (
          'No estimate'
        ) : (
          'None yet'
        )}
      </strong>
    </Link>
  )
}

/**
 * The period beside the chart: where the change came from and the benchmark, then what the
 * period paid and realized, each opening its report for the same days.
 */
export function PeriodCard({
  data,
  span,
  range,
  accountId,
}: {
  data: FinanceSnapshot
  span?: Statement
  range: ChartPeriod
  accountId?: string
}) {
  const day = data.insights?.today ?? localDateKey(Date.parse(data.updatedAt) / 1000)
  const { from, to } = rangeDates(range, day, span?.start)
  // Rust sums the reports over the same days as the period's statement.
  const summaries = useReports(data, {
    ...(from ? { from, to } : { range: 'all' }),
    ...(accountId ? { accounts: [accountId] } : {}),
  }).data?.summaries
  return (
    <Card className="investments-side-card" aria-label={`${rangeNames[range]} in detail`}>
      <SectionHeading title={rangeNames[range]} />
      <div className="investments-side-stats">
        {span ? (
          <>
            <Metric
              label="Investment gain"
              value={
                <span className="sensitive-value">{formatSignedCurrency(span.investmentGain)}</span>
              }
              tone={valueTone(span.investmentGain)}
            />
            <Metric
              label={span.netFlows < 0 ? 'Withdrawn' : 'Added'}
              value={
                <span className="sensitive-value">{formatCurrency(Math.abs(span.netFlows))}</span>
              }
            />
            <Metric
              label="How your investments did"
              value={formatPercent(span.timeWeightedPct)}
              tone={valueTone(span.timeWeightedPct)}
            />
            <Metric
              label="How your money did"
              value={formatPercent(span.moneyWeightedPct)}
              tone={span.moneyWeightedPct == null ? 'muted' : valueTone(span.moneyWeightedPct)}
            />
            <Metric label="S&P 500" value={formatPercent(span.benchmarkPct)} tone="muted" />
          </>
        ) : null}
      </div>
      <nav className="investments-side-stats investments-reports" aria-label="Reports">
        {reports.map(({ chart, label }) => (
          <ReportMetric
            key={chart}
            chart={chart}
            label={label}
            summary={summaries?.find((summary) => summary.chart === chart)}
            from={from}
            to={to}
            accountId={accountId}
          />
        ))}
        <TaxesMetric data={data} />
      </nav>
    </Card>
  )
}

/** Only what calls for a decision, and only when something does. */
export function InvestmentsAttention({ data }: { data: FinanceSnapshot }) {
  const attention = data.insights?.investmentAttention
  // Exposure shares this request; a sector at a third or more of stocks is a decision too.
  const sectors = usePortfolioSectors(data, Boolean(data.insights?.allocation))
  const concentrated = sectors.data?.concentrated
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(id, data.accounts.find((account) => account.id === id)?.name ?? id, names)
  const items: Array<{
    key: string
    ticker?: string
    kind: string
    title: string
    detail: ReactNode
  }> = [
    ...(attention?.flags ?? []).map((flag) =>
      flag.kind === 'concentrated'
        ? {
            key: `concentrated:${flag.ticker}`,
            ticker: flag.ticker,
            kind: 'Concentration',
            title: `${flag.ticker} is ${formatShare(flag.percent)} of investments`,
            detail: <>Over a fifth of your investments in one company.</>,
          }
        : flag.kind === 'longTermSoon'
          ? {
              key: `long-term:${flag.ticker}:${flag.accountId}:${flag.date}`,
              ticker: flag.ticker,
              kind: 'Tax timing',
              title: `${flag.units} ${flag.ticker} ${pluralize(flag.units, 'share goes', 'shares go')} long-term ${formatShortDay(flag.date)}`,
              detail: (
                <>
                  <span className="sensitive-value">{money(flag.gain)}</span> gain, taxed short-term
                  if sold sooner.
                </>
              ),
            }
          : {
              key: `loss:${flag.ticker}:${flag.accountId}`,
              ticker: flag.ticker,
              kind: 'Tax loss',
              title: `${flag.ticker} is ${percent(flag.percent)} below cost`,
              detail: (() => {
                const harvest = data.insights?.taxes.harvest.find(
                  (item) => item.ticker === flag.ticker && item.accountId === flag.accountId,
                )
                return (
                  <>
                    <span className="sensitive-value">{money(flag.loss)}</span> loss to harvest in{' '}
                    {accountName(flag.accountId)}
                    {harvest?.taxSavings != null ? (
                      <>
                        , saves ~
                        <span className="sensitive-value">{money(harvest.taxSavings)}</span>
                      </>
                    ) : null}
                    .{harvest?.washUntil ? ` Sell after ${formatShortDay(harvest.washUntil)}.` : ''}
                  </>
                )
              })(),
            },
    ),
    ...(concentrated
      ? [
          {
            key: `sector:${concentrated.label}`,
            kind: 'Sector concentration',
            title: `${concentrated.label} is ${formatShare(concentrated.percent)} of your holdings`,
            detail: <>One sector carries much of your stocks’ risk.</>,
          },
        ]
      : []),
  ]
  // Nothing to decide leaves no card: the page stays about the portfolio.
  if (!items.length) return null
  return (
    <Card className="portfolio-card investments-attention-card">
      <SectionHeading title="Needs you" />
      <ul className="investments-attention" aria-label="Needs you" data-keyboard-region>
        {items.map((item) => {
          const copy = (
            <span className="investments-attention-copy">
              <small>{item.kind}</small>
              <strong>{item.title}</strong>
              <span>{item.detail}</span>
            </span>
          )
          return (
            <li key={item.key}>
              {item.ticker ? (
                <Link
                  className="investments-attention-row"
                  to="/holdings/$ticker"
                  params={{ ticker: item.ticker }}
                  data-keyboard-row
                  data-keyboard-open
                >
                  <SecurityMark ticker={item.ticker} small />
                  {copy}
                  <ChevronRight size={15} aria-hidden="true" />
                </Link>
              ) : (
                <div className="investments-attention-row">
                  <span className="investments-attention-mark" aria-hidden="true">
                    <AlertCircle size={14} />
                  </span>
                  {copy}
                </div>
              )}
            </li>
          )
        })}
      </ul>
    </Card>
  )
}

const statusLabels = {
  scheduled: 'Scheduled',
  confirmed: 'Confirmed',
  estimated: 'Estimated',
  revised: 'Revised',
  retracted: 'Retracted',
} as const
type ComingItem = {
  key: string
  date: string
  ticker: string | null
  title: string
  amount?: number
  status: keyof typeof statusLabels
  url?: string | null
}

/** One scheduled event: a holding's opens that holding, a market date opens its source. */
function ComingRow({ item }: { item: ComingItem }) {
  const content = (
    <>
      <time dateTime={item.date}>{formatShortDay(item.date)}</time>
      <span>
        {item.title} <small>{statusLabels[item.status]}</small>
      </span>
      {item.amount != null ? (
        <span className="sensitive-value positive">{`+${formatCurrency(item.amount)}`}</span>
      ) : null}
    </>
  )
  if (item.ticker)
    return (
      <Link
        className="investments-coming-row"
        to="/holdings/$ticker"
        params={{ ticker: item.ticker }}
        data-keyboard-row
        data-keyboard-open
      >
        {content}
      </Link>
    )
  if (item.url)
    return (
      <ExternalLink
        className="investments-coming-row"
        href={item.url}
        data-keyboard-row
        data-keyboard-open
      >
        {content}
      </ExternalLink>
    )
  return <div className="investments-coming-row">{content}</div>
}

/**
 * What's scheduled in the next month, each with whether it's confirmed or estimated: dividends
 * Rust expects from announced corporate actions, earnings dates for the largest holdings, and
 * the market's own dates (holidays, Fed decisions, index rebalances) from the summary.
 */
export function InvestmentsComingUp({
  data,
  earningsConnected,
  summary,
}: {
  data: FinanceSnapshot
  earningsConnected: boolean
  summary?: InvestmentSummary
}) {
  const native = typeof window !== 'undefined' && isTauri()
  const today = data.insights?.today ?? localDateKey(Date.parse(data.updatedAt) / 1000)
  const horizon = addDays(today, COMING_DAYS)
  const watch = (data.insights?.investmentAttention?.earningsWatch ?? []).toSorted()
  const earnings = useQuery({
    queryKey: ['earnings-calendar', watch],
    queryFn: () => getEarningsCalendar(watch),
    enabled: native && earningsConnected && watch.length > 0,
    staleTime: 6 * 60 * 60 * 1000,
    retry: false,
  })
  const items: ComingItem[] = [
    ...(data.insights?.dividends.upcoming ?? []).flatMap((dividend) => {
      const date = dividend.payableDate ?? dividend.exDate
      return date >= today && date <= horizon
        ? [
            {
              key: `dividend:${dividend.ticker}:${dividend.exDate}`,
              date,
              ticker: dividend.ticker,
              // Without a pay date, the date shown is when the stock goes ex-dividend.
              title: `${dividend.ticker} ${dividend.payableDate ? '' : 'ex-'}${dividend.special ? 'special dividend' : 'dividend'}`,
              amount: dividend.amount,
              status: 'confirmed' as const,
            },
          ]
        : []
    }),
    // Report dates from the earnings calendar are the companies' expected dates.
    ...(earnings.data ?? [])
      .filter((event) => event.reportDate >= today && event.reportDate <= horizon)
      .map((event) => ({
        key: `earnings:${event.symbol}:${event.reportDate}`,
        date: event.reportDate,
        ticker: event.symbol,
        title: `${event.symbol} earnings`,
        status: 'estimated' as const,
      })),
    // Dividends are listed above with their amounts.
    ...(summary?.ahead ?? [])
      .filter(
        (event) => !/dividend/i.test(event.kind) && event.date >= today && event.date <= horizon,
      )
      .map((event) => ({
        key: `ahead:${event.kind}:${event.ticker ?? ''}:${event.date}`,
        date: event.date,
        ticker: event.ticker,
        title: event.title,
        status: event.status,
        url: event.url,
      })),
  ].toSorted(
    (left, right) => left.date.localeCompare(right.date) || left.key.localeCompare(right.key),
  )
  if (!items.length) return null
  return (
    <Card className="portfolio-card investments-coming-card">
      <SectionHeading title="What’s ahead" />
      <ul className="investments-coming" aria-label="What’s ahead" data-keyboard-region>
        {items.slice(0, COMING_LISTED).map((item) => (
          <li key={item.key}>
            <ComingRow item={item} />
          </li>
        ))}
      </ul>
    </Card>
  )
}

const performanceRows: Array<{ window: ChartPeriod; label: string }> = [
  { window: 'week', label: 'Week' },
  { window: 'month', label: 'Month' },
  { window: 'quarter', label: '3 months' },
  { window: 'year', label: 'Year' },
  { window: 'all', label: 'All time' },
]

/**
 * Time-weighted returns for every period against the S&P 500 over the same days. The page's
 * period is marked; choosing another row changes the page's period.
 */
export function InvestmentsPerformance({
  data,
  range,
}: {
  data: FinanceSnapshot
  range: ChartPeriod
}) {
  const [, setGraphWindow] = useGraphWindow()
  const windows = data.insights?.periods ?? []
  const rows = performanceRows.flatMap((row) => {
    const span = windows.find(({ window }) => window === row.window)
    return span ? [{ ...row, span }] : []
  })
  return (
    <Card className="portfolio-card investments-performance-card">
      <SectionHeading title="Performance" />
      {!rows.length ? (
        <EmptyState>No return history yet.</EmptyState>
      ) : (
        <div className="investments-performance" role="table" aria-label="Returns by period">
          <div className="investments-performance-head" role="row">
            <span role="columnheader">Period</span>
            <span role="columnheader">You</span>
            <span role="columnheader">S&amp;P 500</span>
            <span role="columnheader">Difference</span>
          </div>
          <ul data-keyboard-region>
            {rows.map(({ window, label, span }) => {
              const difference = span.activePct
              return (
                <li key={window} role="row">
                  <button
                    type="button"
                    className="investments-performance-row"
                    aria-pressed={window === range}
                    aria-label={`Show ${label.toLowerCase()}`}
                    onClick={() => setGraphWindow(chartPeriodSeconds(window))}
                    data-keyboard-row
                  >
                    <span role="cell">{label}</span>
                    <span role="cell" className={valueTone(span.timeWeightedPct)}>
                      {formatPercent(span.timeWeightedPct)}
                    </span>
                    <span role="cell" className="muted">
                      {formatPercent(span.benchmarkPct)}
                    </span>
                    <span role="cell" className={valueTone(difference)}>
                      {difference == null ? '—' : formatPoints(difference)}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>
        </div>
      )}
    </Card>
  )
}
