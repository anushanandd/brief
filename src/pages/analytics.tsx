import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import {
  ArrowDownUp,
  BadgeDollarSign,
  ChartNoAxesCombined,
  CreditCard,
  Percent,
  Receipt,
  Sprout,
  X,
} from 'lucide-react'
import { useCallback, useMemo } from 'react'

import { AccountMark } from '../components/account-mark'
import { GroupedActivityList } from '../components/activity-list'
import { CashFlowSankey } from '../components/cash-flow-sankey'
import { PageError, PageLoading } from '../components/data-state'
import {
  AnimatedCurrency,
  Change,
  Button,
  Card,
  EmptyState,
  Metric,
  RangeSelector,
  SectionHeading,
} from '../components/ui'
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import {
  analyticsCharts,
  analyticsBarStack,
  analyticsGroupColors,
  analyticsEntries,
  analyticsReport,
  analyticsTotal,
  analyticsWindow,
  analyticsAccountMatches,
  toggleAnalyticsAccount,
  noAnalyticsAccounts,
  type AnalyticsRange,
  type AnalyticsChart,
} from '../lib/analytics'
import {
  formatCompactCurrency,
  formatCurrency,
  formatPercent,
  valueTone,
  formatActivityName,
} from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { briefMerchant } from '../lib/insights'
import { getExternalLogosEnabled } from '../lib/logos'
import { cashFlowBreakdown, expectedMoneyEvents } from '../lib/money'
import type { FinanceSnapshot } from '../lib/schema'
import { getSpendingAccountId } from '../lib/spending-preferences'

const icons = [
  BadgeDollarSign,
  CreditCard,
  Sprout,
  Percent,
  Receipt,
  ChartNoAxesCombined,
  ArrowDownUp,
]
const rangeValues = ['week', 'month', 'quarter', 'all'] as const
const ranges = graphWindows.map((range, index) => ({
  value: rangeValues[index],
  label: range.label,
  accessibleLabel: range.settingsLabel,
}))
const dateLabel = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
  })

export function AnalyticsPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <AnalyticsWorkspace data={query.data} />
}

function AnalyticsWorkspace({ data }: { data: FinanceSnapshot }) {
  const search = useSearch({ from: '/analytics' })
  const navigate = useNavigate({ from: '/analytics' })
  const selected = search.chart ?? 'income'
  const spendingAccountId = getSpendingAccountId()
  const names = getAccountDisplayNames()
  const externalLogosEnabled = getExternalLogosEnabled()
  const window = useMemo(() => analyticsWindow(data, search), [data, search])
  const reports = useMemo(
    () =>
      analyticsCharts
        .map((chart) => ({
          ...chart,
          ...analyticsReport(
            analyticsEntries(data, chart.id, spendingAccountId, search.account, names),
            window,
          ),
        }))
        .toSorted((a, b) => (b.total ?? -Infinity) - (a.total ?? -Infinity)),
    [data, search, spendingAccountId, window, names],
  )
  const report = reports.find(({ id }) => id === selected)!
  const select = (chart: AnalyticsChart) => void navigate({ search: { ...search, chart } })
  const setRange = (range: AnalyticsRange) =>
    void navigate({ search: { ...search, range, from: undefined, to: undefined } })
  useGraphAccountShortcuts((direction) => {
    const index = reports.findIndex(({ id }) => id === selected)
    select(reports[(index + direction + reports.length) % reports.length].id)
  })
  useGraphWindowShortcuts(
    useCallback(
      (next) => {
        const current =
          graphWindows[rangeValues.findIndex((value) => value === (search.range ?? 'month'))]
            ?.secs ?? 0
        const seconds = typeof next === 'function' ? next(current) : next
        const range = rangeValues[graphWindows.findIndex(({ secs }) => secs === seconds)]
        if (range) void navigate({ search: { ...search, range, from: undefined, to: undefined } })
      },
      [navigate, search],
    ),
  )
  const accounts = data.accounts.filter(
    ({ id, currency }) => id !== 'all' && (!currency || currency === 'USD'),
  )
  const missingAccount = Boolean(
    search.account &&
    search.account !== noAnalyticsAccounts &&
    !accounts.some(({ id }) => analyticsAccountMatches(search.account, id)),
  )
  const needsSpendingAccount =
    selected === 'amex-credits' &&
    !data.accounts.some(({ id, type }) => id === spendingAccountId && type === 'credit')
  const unavailable = needsSpendingAccount || missingAccount || !window.valid
  const activities = useMemo(
    () =>
      new Map(buildActivities(data, names, externalLogosEnabled).map((item) => [item.id, item])),
    [data, names, externalLogosEnabled],
  )
  const groupColors = useMemo(
    () =>
      analyticsGroupColors(
        analyticsEntries(data, selected, spendingAccountId, undefined, names).map(
          ({ group }) => group,
        ),
      ),
    [data, selected, spendingAccountId, names],
  )
  const accountReport = useMemo(
    () =>
      analyticsReport(
        analyticsEntries(data, selected, spendingAccountId, undefined, names),
        window,
      ),
    [data, selected, spendingAccountId, window, names],
  )
  const accountRows = accounts.flatMap(({ id, name, type }) => {
    const entries = accountReport.entries.filter(({ accountId }) => accountId === id)
    return entries.length
      ? [{ id, type, label: accountDisplayName(id, name, names), value: analyticsTotal(entries) }]
      : []
  })
  const periodChange =
    !unavailable && report.total != null && report.previous != null
      ? (Math.round(report.total * 100) - Math.round(report.previous * 100)) / 100
      : null
  const activitySearch = {
    analysis: selected,
    account: search.account,
    from: window.start,
    to: window.end,
  }
  const flowIds = new Set(report.entries.map(({ id }) => id))
  const flow =
    selected === 'cash-flow'
      ? cashFlowBreakdown(data.transactions.filter(({ id }) => flowIds.has(`spending:${id}`)))
      : null
  const expected =
    selected === 'cash-flow'
      ? expectedMoneyEvents(data).filter((event) =>
          analyticsAccountMatches(search.account, event.accountId),
        )
      : []

  return (
    <div className="page accounts-workspace-page analytics-page">
      <WorkspaceHeader
        title="Analytics"
        breadcrumbs={[
          { label: 'Analytics', to: '/analytics', search: {} },
          { label: report.label, to: '/analytics', search },
        ]}
      />
      <nav
        className="account-switcher"
        aria-label="Analytics charts"
        aria-keyshortcuts="Meta+ArrowLeft Meta+ArrowRight"
      >
        <div className="account-switcher-grid">
          {reports.map((item) => {
            const Icon = icons[analyticsCharts.findIndex(({ id }) => id === item.id)]
            const missing =
              missingAccount ||
              !window.valid ||
              (item.id === 'amex-credits' &&
                !data.accounts.some(
                  ({ id, type }) => id === spendingAccountId && type === 'credit',
                ))
            return (
              <Link
                key={item.id}
                to="/analytics"
                search={{ ...search, chart: item.id }}
                activeOptions={{ exact: true }}
                className="account-switcher-button"
                aria-current={item.id === selected ? 'page' : undefined}
                data-keyboard-row
                data-keyboard-open
              >
                <span className="transaction-mark" aria-hidden="true">
                  <Icon size={15} />
                </span>
                <span className="account-switcher-copy">
                  <span>{item.label}</span>
                  <strong className="account-switcher-value">
                    <span>{formatCurrency(missing ? null : item.total)}</span>
                    {item.percent != null && !missing ? (
                      <span
                        className={valueTone(item.id === 'fees' ? -item.percent : item.percent)}
                        aria-label={`${formatPercent(item.percent)} versus previous imported period`}
                      >
                        {formatPercent(item.percent)}
                      </span>
                    ) : null}
                  </strong>
                </span>
              </Link>
            )
          })}
        </div>
      </nav>
      <div className="account-overview-dashboard">
        <div className="account-overview-primary-grid">
          <div className="analytics-primary">
            <Card className="account-overview-chart-card analytics-chart-card">
              <header className="home-balance-header">
                <div className="home-balance-main">
                  <h2 className="balance-label">
                    {selected === 'realized' ? 'Estimated realized P/L' : report.label}
                  </h2>
                  <div className="home-balance-value">
                    <AnimatedCurrency
                      className="hero-number"
                      value={unavailable ? null : report.total}
                    />
                  </div>
                  <div className="chart-summary-row">
                    <div className="hero-change" aria-label="Change versus previous period">
                      <span
                        className={valueTone(
                          periodChange == null
                            ? null
                            : selected === 'fees'
                              ? -periodChange
                              : periodChange,
                        )}
                      >
                        {formatCurrency(periodChange)}
                      </span>
                      <Change
                        value={unavailable ? null : report.percent}
                        favorable={selected === 'fees' ? 'decrease' : 'increase'}
                      />
                    </div>
                    <RangeSelector<string>
                      label="Analytics period"
                      value={window.custom ? '' : window.range}
                      options={ranges}
                      onValueChange={(value) => {
                        const range = ranges.find((option) => option.value === value)
                        if (range) setRange(range.value)
                      }}
                    />
                  </div>
                </div>
              </header>
              {unavailable ? (
                <EmptyState>
                  {needsSpendingAccount ? (
                    <>
                      Choose a spending account in <Link to="/settings">Settings</Link> to chart
                      Amex credits.
                    </>
                  ) : missingAccount ? (
                    'This account is unavailable. Select another account.'
                  ) : (
                    'The start date must be on or before the end date.'
                  )}
                </EmptyState>
              ) : !report.entries.length ? (
                <EmptyState>No matching posted activity in this period.</EmptyState>
              ) : (
                <AnalyticsBars
                  groupColors={groupColors}
                  report={report}
                  search={activitySearch}
                  activities={activities}
                />
              )}
            </Card>
            {flow && !unavailable ? (
              <Card className="account-group-card">
                <SectionHeading title="Sources and uses" />
                <CashFlowSankey values={flow} />
              </Card>
            ) : null}
            <Card className="account-preview-card analytics-activities-card">
              <SectionHeading
                title={
                  <Link className="section-heading-link" to="/activities" search={activitySearch}>
                    Activities
                  </Link>
                }
              />
              <div
                className="analytics-activity-scroll"
                role="region"
                aria-label="Analytics activities"
                tabIndex={0}
              >
                <GroupedActivityList
                  activities={report.entries
                    .flatMap((entry) => {
                      const activity = activities.get(entry.id)
                      return activity
                        ? [
                            {
                              ...activity,
                              title:
                                selected === 'income'
                                  ? briefMerchant(activity.title)
                                  : activity.title,
                              date: entry.date,
                            },
                          ]
                        : []
                    })
                    .toSorted((left, right) => right.date.localeCompare(left.date))}
                  referenceIso={data.updatedAt}
                  emptyMessage="No matching posted activity in this period."
                />
              </div>
            </Card>
          </div>
          <div className="analytics-companion">
            <Card className="account-overview-summary-card">
              <SectionHeading title="Overview" />
              <div className="account-summary-metrics">
                <Metric
                  label={selected === 'realized' ? 'Imported sales' : 'Posted records'}
                  value={unavailable ? '—' : String(report.entries.length)}
                />
                <Metric
                  label="Previous period"
                  value={formatCurrency(unavailable ? null : report.previous)}
                />
                <Metric
                  label={selected === 'realized' ? 'Average sale P/L' : 'Average amount'}
                  value={formatCurrency(unavailable ? null : report.average)}
                />
                <Metric
                  label={selected === 'realized' ? 'Largest sale P/L' : 'Largest amount'}
                  value={formatCurrency(unavailable ? null : report.largest)}
                />
                <Metric label="Active days" value={unavailable ? '—' : String(report.activeDays)} />
                <Metric
                  label="Period change"
                  value={formatPercent(unavailable ? null : report.percent)}
                  tone={valueTone(
                    unavailable || report.percent == null
                      ? null
                      : selected === 'fees'
                        ? -report.percent
                        : report.percent,
                  )}
                />
              </div>
            </Card>
            <Card className="account-overview-summary-card analytics-accounts-card">
              <SectionHeading
                title="Accounts"
                action={
                  search.account !== undefined ? (
                    <Button
                      icon={X}
                      aria-label="Clear account selection"
                      size="compact"
                      variant="ghost"
                      onClick={() => void navigate({ search: { ...search, account: undefined } })}
                    >
                      Clear
                    </Button>
                  ) : undefined
                }
              />
              <div className="analytics-breakdown" role="group" aria-label="Chart accounts">
                {accountRows.map((item) => (
                  <label className="analytics-breakdown-row analytics-account-row" key={item.id}>
                    <input
                      type="checkbox"
                      checked={analyticsAccountMatches(search.account, item.id)}
                      onChange={() =>
                        void navigate({
                          search: {
                            ...search,
                            account: toggleAnalyticsAccount(
                              search.account,
                              item.id,
                              accountRows.map(({ id }) => id),
                            ),
                          },
                        })
                      }
                    />
                    <AccountMark type={item.type} />
                    <span className="account-row-name">
                      <strong>{item.label}</strong>
                      <small>{item.type}</small>
                    </span>
                    <strong>
                      {formatCurrency(needsSpendingAccount || !window.valid ? null : item.value)}
                    </strong>
                  </label>
                ))}
                {!accountRows.length ? (
                  <EmptyState>No accounts with matching activity in this period.</EmptyState>
                ) : null}
              </div>
            </Card>
            {selected === 'cash-flow' ? (
              <Card className="account-group-card">
                <SectionHeading title="Expected activity" />
                {expected.length ? (
                  <div className="analytics-breakdown">
                    {expected.map((event) => (
                      <div className="analytics-breakdown-row" key={event.id}>
                        <span>
                          {formatActivityName(event.title)}
                          <small>{dateLabel(event.date)} · High confidence</small>
                        </span>
                        <strong>{formatCurrency(event.amount)}</strong>
                      </div>
                    ))}
                  </div>
                ) : (
                  <EmptyState>
                    More repeated activity is needed before Brief can predict what comes next.
                  </EmptyState>
                )}
              </Card>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  )
}

function AnalyticsBars({
  groupColors,
  report,
  search,
  activities,
}: {
  groupColors: Map<string, string>
  activities: Map<string, ReturnType<typeof buildActivities>[number]>
  report: ReturnType<typeof analyticsReport>
  search: { analysis: AnalyticsChart; account?: string; from: string; to: string }
}) {
  const stacks = report.buckets.map(({ entries }) => analyticsBarStack(entries))
  const max = Math.max(0, ...stacks.map(({ positive }) => positive))
  const min = Math.min(0, ...stacks.map(({ negative }) => negative))
  const span = max - min || 1
  const zero = max === 0 && min === 0 ? 100 : (max / span) * 100
  const dense = report.buckets.length > 12
  return (
    <div className="analytics-plot">
      <div className="analytics-bars" aria-label="Period totals">
        <div className="analytics-bar-axis" aria-hidden="true">
          <span>{formatCompactCurrency(max)}</span>
          <span>{formatCompactCurrency(min)}</span>
        </div>
        <div className="analytics-bars-scroll">
          <div
            className={`analytics-bars-grid${dense ? ' is-dense' : ''}`}
            style={{ gridTemplateColumns: `repeat(${report.buckets.length}, minmax(32px, 1fr))` }}
          >
            {report.buckets.map((bucket, index) => {
              const label = new Date(`${bucket.from}T12:00:00Z`).toLocaleDateString('en-US', {
                timeZone: 'UTC',
                ...(report.unit === 'day'
                  ? ({ month: 'short', day: 'numeric' } as const)
                  : report.unit === 'year'
                    ? ({ year: 'numeric' } as const)
                    : ({ month: 'short', year: '2-digit' } as const)),
              })
              return (
                <div key={bucket.from} className="monthly-bar-column analytics-bar-column">
                  <div className="analytics-bar-track">
                    <span className="analytics-bar-zero" style={{ top: `${zero}%` }} />
                    {[...new Set(stacks[index].segments.map(({ group }) => group))].flatMap(
                      (group) =>
                        [false, true].map((negative) => {
                          const entries = stacks[index].segments.filter(
                            (entry) =>
                              entry.group === group &&
                              entry.value! < 0 === negative &&
                              entry.value !== 0,
                          )
                          if (!entries.length) return null
                          const low = Math.min(...entries.flatMap(({ start, end }) => [start, end]))
                          const high = Math.max(
                            ...entries.flatMap(({ start, end }) => [start, end]),
                          )
                          const total =
                            entries.reduce(
                              (sum, entry) => sum + Math.round(entry.value! * 100),
                              0,
                            ) / 100
                          return (
                            <Tooltip key={`${group}:${negative}`}>
                              <TooltipTrigger
                                render={
                                  <Link
                                    className="analytics-bar-category"
                                    to="/activities"
                                    search={{ ...search, from: bucket.from, to: bucket.to }}
                                    aria-label={`${group}, ${dateLabel(bucket.from)} through ${dateLabel(bucket.to)}: ${formatCurrency(total)}. View period activity`}
                                    style={{
                                      top: `${((max - high) / span) * 100}%`,
                                      height: `${((high - low) / span) * 100}%`,
                                    }}
                                  >
                                    {entries.map((entry) => (
                                      <span
                                        key={entry.id}
                                        aria-hidden="true"
                                        className={`analytics-bar-fill ${negative ? 'is-negative' : ''}`}
                                        style={{
                                          top: `${((high - Math.max(entry.start, entry.end)) / (high - low)) * 100}%`,
                                          height: `${(Math.abs(entry.end - entry.start) / (high - low)) * 100}%`,
                                          background: groupColors.get(group),
                                        }}
                                      />
                                    ))}
                                  </Link>
                                }
                              />
                              <TooltipContent
                                className="treemap-tooltip sankey-tooltip"
                                sideOffset={8}
                              >
                                <div className="treemap-tooltip-heading">
                                  <strong>
                                    {formatActivityName(group)} · {label}
                                  </strong>
                                  <span>{formatCurrency(total)}</span>
                                </div>
                                <div className="treemap-tooltip-composition">
                                  <ul>
                                    {entries.slice(0, 8).map((entry) => (
                                      <li key={entry.id}>
                                        <span>
                                          {formatActivityName(
                                            briefMerchant(
                                              activities.get(entry.id)?.title ?? entry.group,
                                            ),
                                          )}{' '}
                                          · {dateLabel(entry.date)}
                                        </span>
                                        <strong>{formatCurrency(entry.value)}</strong>
                                      </li>
                                    ))}
                                    {entries.length > 8 ? (
                                      <li className="muted">
                                        +{entries.length - 8} more · Open activity to view all
                                      </li>
                                    ) : null}
                                  </ul>
                                </div>
                              </TooltipContent>
                            </Tooltip>
                          )
                        }),
                    )}
                    {bucket.value == null ? (
                      <span className="analytics-bar-missing" aria-label="Amount unavailable">
                        —
                      </span>
                    ) : null}
                  </div>
                  <small>
                    {!dense || index % 5 === 0 || index === report.buckets.length - 1
                      ? label
                      : null}
                  </small>
                </div>
              )
            })}
          </div>
        </div>
      </div>
      <div className="analytics-legend-panel">
        <ul className="analytics-legend" aria-label="Chart legend" tabIndex={0}>
          {[...new Set(report.entries.map(({ group }) => group))].toSorted().map((group) => (
            <li key={group}>
              <i
                className="analytics-category-dot"
                style={{ background: groupColors.get(group) }}
                aria-hidden="true"
              />
              {formatActivityName(group)}
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}
