import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { GroupedActivityList } from '../components/activity-list'
import { CashFlowSankey } from '../components/cash-flow-sankey'
import { PageError, PageLoading } from '../components/data-state'
import {
  ArrowDownUp,
  BadgeDollarSign,
  ChartNoAxesCombined,
  CreditCard,
  Percent,
  Receipt,
  Sprout,
  X,
  type IconComponent,
} from '../components/icons'
import { PlatinumBenefits } from '../components/platinum-benefits'
import {
  AnimatedCurrency,
  Button,
  ChartChange,
  ChartRangeSelect,
  Card,
  EmptyState,
  Metric,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip'
import { WorkspaceHeader } from '../components/workspace-header'
import { useActiveSelectionScroll } from '../hooks/use-active-selection-scroll'
import { useFinance } from '../hooks/use-finance'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
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
  localDateKey,
  analyticsAccountMatches,
  toggleAnalyticsAccount,
  noAnalyticsAccounts,
  type AnalyticsRange,
  type AnalyticsChart,
} from '../lib/analytics'
import { useAnalyticsOrder } from '../lib/analytics-order-preferences'
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
import { cashFlowBreakdown } from '../lib/money'
import type { FinanceSnapshot } from '../lib/schema'
import { resolveSpendingAccount } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

const icons: Record<AnalyticsChart, IconComponent> = {
  'cash-flow': ArrowDownUp,
  income: BadgeDollarSign,
  'amex-credits': CreditCard,
  dividends: Sprout,
  interest: Percent,
  fees: Receipt,
  realized: ChartNoAxesCombined,
}
const iconColors: Record<AnalyticsChart, string> = {
  'cash-flow': 'var(--chart-secondary)',
  income: 'var(--positive)',
  'amex-credits': 'var(--positive)',
  dividends: 'var(--positive)',
  interest: 'var(--positive)',
  fees: 'var(--negative)',
  realized: 'var(--chart-benchmark)',
}
const rangeValues = ['week', 'month', 'quarter', 'year', 'all'] as const
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
  const switcherRef = useActiveSelectionScroll()
  const [today, setToday] = useState(() => localDateKey(Date.now() / 1000))
  useEffect(() => {
    const update = () => setToday(localDateKey(Date.now() / 1000))
    const timer = globalThis.setInterval(update, 60_000)
    globalThis.addEventListener('focus', update)
    return () => {
      globalThis.clearInterval(timer)
      globalThis.removeEventListener('focus', update)
    }
  }, [])
  const activityScrollRef = useViewportScroll()
  const orderedCharts = useAnalyticsOrder()
  const search = useSearch({ from: '/analytics' })
  const navigate = useNavigate({ from: '/analytics' })
  const selected = search.chart ?? orderedCharts[0]
  const spendingAccountId = getSpendingAccountId()
  const spendingAccount = resolveSpendingAccount(data.accounts, spendingAccountId)
  const benefitTransactions = spendingAccount
    ? data.transactions.filter(({ accountId }) => accountId === spendingAccount.id)
    : []
  const names = getAccountDisplayNames()
  const externalLogosEnabled = getExternalLogosEnabled()
  const window = useMemo(() => analyticsWindow(data, search, today), [data, search, today])
  const reports = useMemo(
    () =>
      orderedCharts.map((id) => {
        const chart = analyticsCharts.find((item) => item.id === id)!
        return {
          ...chart,
          ...analyticsReport(
            analyticsEntries(data, chart.id, spendingAccountId, search.account, names),
            window,
          ),
        }
      }),
    [data, search, spendingAccountId, window, names, orderedCharts],
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
  const needsSpendingAccount = selected === 'amex-credits' && !spendingAccount
  const incompatibleBenefitAccount =
    selected === 'amex-credits' &&
    search.account !== undefined &&
    !analyticsAccountMatches(search.account, spendingAccountId)
  const unavailable =
    needsSpendingAccount || missingAccount || incompatibleBenefitAccount || !window.valid
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
  const activityCard = (
    <ScrollCueCard
      className="account-preview-card analytics-activities-card"
      scrollSelector=".analytics-activity-scroll"
    >
      <SectionHeading
        title={
          <Link className="section-heading-link" to="/activities" search={activitySearch}>
            {selected === 'amex-credits' ? 'Credit activity' : 'Activities'}
          </Link>
        }
      />
      <div
        ref={activityScrollRef}
        className="analytics-activity-scroll"
        role="region"
        aria-label={selected === 'amex-credits' ? 'Credit activity' : 'Analytics activities'}
        tabIndex={0}
        data-keyboard-region
      >
        <GroupedActivityList
          activities={report.entries
            .flatMap((entry) => {
              const activity = activities.get(entry.id)
              return activity
                ? [
                    {
                      ...activity,
                      title: selected === 'income' ? briefMerchant(activity.title) : activity.title,
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
    </ScrollCueCard>
  )

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
        ref={switcherRef}
        className="account-switcher"
        aria-label="Analytics charts"
        aria-keyshortcuts="ArrowLeft ArrowRight"
        data-keyboard-region
      >
        <div className="account-switcher-grid">
          {reports.map((item) => {
            const Icon = icons[item.id]
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
                <span
                  className="transaction-mark"
                  style={{
                    color: missing ? 'var(--text-muted)' : iconColors[item.id],
                    backgroundColor: `color-mix(in srgb, ${missing ? 'var(--text-muted)' : iconColors[item.id]} 14%, var(--surface-raised))`,
                  }}
                  aria-hidden="true"
                >
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
        <div
          className={`account-overview-primary-grid${selected === 'amex-credits' ? ' analytics-credits-grid' : ''}`}
        >
          <div className="analytics-primary">
            <ScrollCueCard
              className="account-overview-chart-card analytics-chart-card"
              scrollSelector=".cash-flow-sankey-scroll"
            >
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
                </div>
                <ChartRangeSelect<string>
                  label="Analytics period"
                  value={window.custom ? '' : window.range}
                  options={
                    window.custom
                      ? [{ value: '', label: 'Custom', accessibleLabel: 'Custom' }, ...ranges]
                      : ranges
                  }
                  onValueChange={(value) => {
                    const range = ranges.find((option) => option.value === value)
                    if (range) setRange(range.value)
                  }}
                />
                <div className="chart-summary-row">
                  <ChartChange
                    amount={periodChange}
                    percent={unavailable ? null : report.percent}
                    favorable={selected === 'fees' ? 'decrease' : 'increase'}
                    ariaLabel="Change versus previous period"
                  />
                </div>
              </header>
              {unavailable ? (
                <EmptyState>
                  {needsSpendingAccount ? (
                    <>
                      Choose a spending account in <Link to="/settings">Settings</Link> to chart
                      Amex credits.
                    </>
                  ) : incompatibleBenefitAccount ? (
                    <>
                      Amex credits use the saved spending account.{' '}
                      <Button
                        size="compact"
                        variant="ghost"
                        onClick={() => void navigate({ search: { ...search, account: undefined } })}
                      >
                        Clear account filter
                      </Button>
                    </>
                  ) : missingAccount ? (
                    'This account is unavailable. Select another account.'
                  ) : (
                    'The start date must be on or before the end date.'
                  )}
                </EmptyState>
              ) : !report.entries.length ? (
                <EmptyState>No matching posted activity in this period.</EmptyState>
              ) : selected === 'cash-flow' && flow ? (
                <CashFlowSankey values={flow} />
              ) : (
                <AnalyticsBars
                  groupColors={groupColors}
                  report={report}
                  search={activitySearch}
                  activities={activities}
                />
              )}
            </ScrollCueCard>
            {selected === 'amex-credits' ? (
              spendingAccount ? (
                <PlatinumBenefits transactions={benefitTransactions} snapshotIso={data.updatedAt} />
              ) : null
            ) : (
              activityCard
            )}
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
            {selected === 'amex-credits' ? (
              activityCard
            ) : (
              <>
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
                          onClick={() =>
                            void navigate({ search: { ...search, account: undefined } })
                          }
                        >
                          Clear
                        </Button>
                      ) : undefined
                    }
                  />
                  <div className="analytics-breakdown" role="group" aria-label="Chart accounts">
                    {accountRows.map((item) => (
                      <label
                        className="analytics-breakdown-row analytics-account-row"
                        key={item.id}
                      >
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
                          {formatCurrency(
                            needsSpendingAccount || !window.valid ? null : item.value,
                          )}
                        </strong>
                      </label>
                    ))}
                    {!accountRows.length ? (
                      <EmptyState>No accounts with matching activity in this period.</EmptyState>
                    ) : null}
                  </div>
                </Card>
              </>
            )}
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
    <div className="analytics-plot analytics-period-plot">
      <div className="analytics-bars" aria-label="Period totals">
        <div className="analytics-bar-axis" aria-hidden="true">
          <span>{formatCompactCurrency(max)}</span>
          <span>{formatCompactCurrency(min)}</span>
        </div>
        <div className="analytics-bars-scroll">
          <div
            className={`analytics-bars-grid${dense ? ' is-dense' : ''}`}
            style={{
              gridTemplateColumns: `repeat(${report.buckets.length}, minmax(${dense ? 0 : '32px'}, 1fr))`,
            }}
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
                                    {entries.slice(0, 8).map((entry) => {
                                      const activity = activities.get(entry.id)
                                      const merchant = formatActivityName(
                                        briefMerchant(activity?.title ?? entry.group),
                                      )
                                      return (
                                        <li key={entry.id}>
                                          <span>
                                            {merchant} · {dateLabel(entry.date)}
                                          </span>
                                          <strong>{formatCurrency(entry.value)}</strong>
                                        </li>
                                      )
                                    })}
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
      <ul className="analytics-legend bar-chart-legend" aria-label="Chart legend" tabIndex={0}>
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
  )
}
