import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { GroupedActivityList } from '../components/activity-list'
import { BenefitGrid, type BenefitInspection } from '../components/benefit-grid'
import { CashFlowSankey } from '../components/cash-flow-sankey'
import { PageError, PageLoading } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { ChevronRight, X } from '../components/icons'
import { RankedBars } from '../components/ranked-bars'
import { MoneyStrip } from '../components/report-links'
import { SettingsLink } from '../components/settings-dialog'
import {
  AnimatedCurrency,
  Button,
  ChartChange,
  Card,
  ChartRangeSelect,
  EmptyState,
  Metric,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities, type ActivityItem } from '../lib/activity'
import {
  analyticsCharts,
  analyticsDefinition,
  analyticsGroup,
  analyticsBarStack,
  analyticsGroupColors,
  analyticsEntries,
  analyticsReport,
  analyticsTotal,
  analyticsWindow,
  analyticsAccountMatches,
  toggleAnalyticsAccount,
  noAnalyticsAccounts,
  withoutReportView,
  type AnalyticsChart,
} from '../lib/analytics'
import { benefitYear, benefitYearSearch } from '../lib/benefit-grid'
import { localDateKey } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  countOf,
  formatCompactCurrency,
  formatCurrency,
  formatPercent,
  valueTone,
  formatActivityName,
  formatDay,
} from '../lib/format'
import {
  graphWindows,
  useChosenGraphWindow,
  useGraphWindow,
  type ChartPeriod,
  chartPeriodSeconds,
} from '../lib/graph-preferences'
import { briefMerchant } from '../lib/insights'
import { getExternalLogosEnabled } from '../lib/logos'
import { cashFlowBreakdown } from '../lib/money'
import type { FinanceSnapshot } from '../lib/schema'
import { resolveSpendingAccount } from '../lib/spending'

// One fixed order everywhere a chart is listed.
const orderedCharts = analyticsCharts.map(({ id }) => id)
const ranges = graphWindows.map(({ name, label, settingsLabel }) => ({
  value: name,
  label,
  accessibleLabel: settingsLabel,
}))

export function AnalyticsPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <AnalyticsWorkspace data={query.data} />
}

/** Today's local date, kept current across midnight and whenever the window regains focus. */
function useLocalDay() {
  const [day, setDay] = useState(() => localDateKey(Date.now() / 1000))
  useEffect(() => {
    const update = () => setDay(localDateKey(Date.now() / 1000))
    const timer = globalThis.setInterval(update, 60_000)
    globalThis.addEventListener('focus', update)
    return () => {
      globalThis.clearInterval(timer)
      globalThis.removeEventListener('focus', update)
    }
  }, [])
  return day
}

/**
 * The route's search with its range resolved: the URL's, else the range chosen on any page
 * this session, else the report's default. Choosing a range, by control or shortcut, shares it
 * with the other pages and clears custom dates.
 */
function useAnalyticsSearch() {
  const routeSearch = useSearch({ from: '/analytics' })
  const navigate = useNavigate({ from: '/analytics' })
  const [, setSharedWindow] = useGraphWindow()
  const chosenWindow = useChosenGraphWindow()
  const search = useMemo(
    () => ({
      ...routeSearch,
      range:
        routeSearch.range ??
        (routeSearch.from || routeSearch.to || chosenWindow === undefined
          ? undefined
          : graphWindows.find(({ secs }) => secs === chosenWindow)?.name),
    }),
    [routeSearch, chosenWindow],
  )
  const setRange = useCallback(
    (range: ChartPeriod) => {
      setSharedWindow(chartPeriodSeconds(range))
      void navigate({ search: { ...search, range, from: undefined, to: undefined } })
    },
    [navigate, search, setSharedWindow],
  )
  useGraphWindowShortcuts(
    useCallback(
      (next) => {
        const current = chartPeriodSeconds(search.range ?? 'month')
        const seconds = typeof next === 'function' ? next(current) : next
        const range = graphWindows.find(({ secs }) => secs === seconds)?.name
        if (range) setRange(range)
      },
      [search.range, setRange],
    ),
  )
  return { search, navigate, setRange }
}

function AnalyticsWorkspace({ data }: { data: FinanceSnapshot }) {
  // Reports end on the day the insights were computed for, the same day Spending and Home
  // use, so one strip never shows two months. The clock takes over only without insights.
  const clock = useLocalDay()
  const today = data.insights?.today ?? clock
  const activityScrollRef = useViewportScroll()
  const { search, navigate, setRange } = useAnalyticsSearch()
  const selected = search.chart ?? orderedCharts[0]
  const definition = analyticsDefinition(selected)
  // Benefits runs on one calendar year, and the strip above it follows that year.
  const benefitsYear = definition.benefits ? benefitYear(search, today) : undefined
  const pageSearch = useMemo(
    () => (benefitsYear === undefined ? search : benefitYearSearch(search, benefitsYear, today)),
    [benefitsYear, search, today],
  )
  const settings = financeSettings(data)
  const spendingAccountId = settings.spendingAccountId ?? ''
  const spendingAccount = resolveSpendingAccount(data.accounts, spendingAccountId)
  const names = settings.accountNames
  const externalLogosEnabled = getExternalLogosEnabled()
  const reportWindow = useMemo(
    () => analyticsWindow(data, pageSearch, today),
    [data, pageSearch, today],
  )
  const reports = useMemo(
    () =>
      orderedCharts.map((id) => {
        const chart = analyticsCharts.find((item) => item.id === id)!
        return {
          ...chart,
          ...analyticsReport(
            analyticsEntries(data, chart.id, spendingAccountId, search.account, names),
            reportWindow,
          ),
        }
      }),
    [data, search, spendingAccountId, reportWindow, names],
  )
  const report = reports.find(({ id }) => id === selected)!
  // Reports sit under Money or Investments; the switcher and arrows stay within one.
  const group = analyticsGroup(selected)
  const groupReports = reports.filter(({ id }) => analyticsGroup(id) === group)
  const home =
    group === 'money'
      ? ({ label: 'Money', to: '/spending' } as const)
      : ({ label: 'Investments', to: '/holdings' } as const)
  const accounts = data.accounts.filter(
    ({ id, currency }) => id !== 'all' && (!currency || currency === 'USD'),
  )
  const missingAccount = Boolean(
    search.account &&
    search.account !== noAnalyticsAccounts &&
    !accounts.some(({ id }) => analyticsAccountMatches(search.account, id)),
  )
  const needsSpendingAccount = definition.benefits && !spendingAccount
  const incompatibleBenefitAccount =
    definition.benefits &&
    search.account !== undefined &&
    !analyticsAccountMatches(search.account, spendingAccountId)
  const unavailable =
    needsSpendingAccount || missingAccount || incompatibleBenefitAccount || !reportWindow.valid
  const activities = useMemo(
    () =>
      new Map(buildActivities(data, names, externalLogosEnabled).map((item) => [item.id, item])),
    [data, names, externalLogosEnabled],
  )
  const groupColors = useMemo(
    () =>
      analyticsGroupColors(
        analyticsEntries(data, selected, spendingAccountId, undefined, names).map(
          (entry) => entry.group,
        ),
      ),
    [data, selected, spendingAccountId, names],
  )
  const accountReport = useMemo(
    () =>
      analyticsReport(
        analyticsEntries(data, selected, spendingAccountId, undefined, names),
        reportWindow,
      ),
    [data, selected, spendingAccountId, reportWindow, names],
  )
  const accountRows = accounts.flatMap(({ id, name, type, institution, domain }) => {
    const entries = accountReport.entries.filter(({ accountId }) => accountId === id)
    return entries.length
      ? [
          {
            id,
            type,
            institution,
            domain,
            label: accountDisplayName(id, name, names),
            value: analyticsTotal(entries),
          },
        ]
      : []
  })
  const expectedRecurringIds = useMemo(() => {
    const entryIds = new Set(
      analyticsEntries(data, selected, spendingAccountId, search.account, names).map(
        ({ id }) => id,
      ),
    )
    return new Set(
      data.transactions.flatMap(({ id, recurringId }) =>
        recurringId && entryIds.has(`spending:${id}`) ? [recurringId] : [],
      ),
    )
  }, [data, names, search.account, selected, spendingAccountId])
  const periodChange =
    !unavailable && report.total != null && report.previous != null
      ? (Math.round(report.total * 100) - Math.round(report.previous * 100)) / 100
      : null
  const activitySearch = {
    analysis: selected,
    account: search.account,
    from: reportWindow.start,
    to: reportWindow.end,
  }
  const flowIds = new Set(report.entries.map(({ id }) => id))
  const flow = definition.flow
    ? cashFlowBreakdown(data.transactions.filter(({ id }) => flowIds.has(`spending:${id}`)))
    : null
  const expectedActivityCard = (
    <ExpectedActivity
      data={data}
      recurringIds={expectedRecurringIds}
      className="account-overview-summary-card analytics-expected-card"
    />
  )
  const activityCard = (
    <ScrollCueCard
      className="account-preview-card analytics-activities-card"
      scrollSelector=".analytics-activity-scroll"
    >
      <SectionHeading
        title={
          <Link
            className="section-heading-link"
            to="/activities"
            search={activitySearch}
            data-command="navigation.open./activities"
          >
            {definition.benefits ? 'Credit activity' : 'Activities'}{' '}
            <ChevronRight size={14} aria-hidden="true" />
          </Link>
        }
      />
      <div
        ref={activityScrollRef}
        className="analytics-activity-scroll"
        role="region"
        aria-label={definition.benefits ? 'Credit activity' : 'Analytics activities'}
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
                      title: definition.briefTitles
                        ? briefMerchant(activity.title)
                        : activity.title,
                      date: entry.date,
                    },
                  ]
                : []
            })
            .toSorted((left, right) => right.date.localeCompare(left.date))}
          referenceIso={data.updatedAt}
          emptyMessage="No posted activity in this period."
        />
      </div>
    </ScrollCueCard>
  )

  return (
    <div className="page accounts-workspace-page analytics-page">
      <WorkspaceHeader
        title={home.label}
        breadcrumbs={[
          { label: home.label, to: home.to, search: {} },
          { label: report.label, to: '/analytics', search },
        ]}
      />
      {/* Money's reports share its strip. Investments reports open from the portfolio's period
          card and lead back through the breadcrumb, since its page is headed by holdings. */}
      {group === 'money' ? (
        <MoneyStrip data={data} current={selected} search={pageSearch} today={today} />
      ) : (
        <ReportStepper reports={groupReports.map(({ id }) => id)} selected={selected} />
      )}
      {definition.benefits &&
      spendingAccount &&
      !incompatibleBenefitAccount &&
      benefitsYear !== undefined ? (
        <BenefitGrid
          data={data}
          year={benefitsYear}
          today={today}
          inspection={{ benefit: search.benefit, period: search.period, review: search.review }}
          activities={activities}
          onYear={(year) =>
            void navigate({
              search: benefitYearSearch(
                { ...search, benefit: undefined, period: undefined, review: undefined },
                year,
                today,
              ),
            })
          }
          onInspect={(next: BenefitInspection) =>
            void navigate({
              search: {
                ...search,
                benefit: next.benefit,
                period: next.period,
                review: next.review,
              },
              replace: true,
            })
          }
        />
      ) : (
        <div className="account-overview-dashboard">
          <div className="account-overview-primary-grid">
            <div className="analytics-primary">
              <ScrollCueCard
                className="account-overview-chart-card analytics-chart-card"
                scrollSelector=".cash-flow-sankey-scroll"
                role="region"
                aria-label={`${report.label} chart`}
                tabIndex={0}
                data-range-focus-region
              >
                <header className="home-balance-header">
                  <div className="home-balance-main">
                    <h2 className="balance-label">{definition.heading ?? report.label}</h2>
                    <div className="home-balance-value">
                      <AnimatedCurrency
                        className="hero-number"
                        value={unavailable ? null : report.total}
                      />
                    </div>
                  </div>
                  <ChartRangeSelect<string>
                    label="Analytics period"
                    command="graph.range."
                    value={reportWindow.custom ? '' : reportWindow.range}
                    options={
                      reportWindow.custom
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
                      favorable={definition.lowerIsBetter ? 'decrease' : 'increase'}
                      ariaLabel="Change versus previous period"
                    />
                  </div>
                </header>
                {unavailable ? (
                  <EmptyState>
                    {needsSpendingAccount ? (
                      <>
                        Choose a spending account in{' '}
                        <SettingsLink target="spending-account">Settings</SettingsLink> to chart
                        Amex credits.
                      </>
                    ) : incompatibleBenefitAccount ? (
                      <>
                        Amex credits use the saved spending account.{' '}
                        <Button
                          size="compact"
                          variant="ghost"
                          onClick={() =>
                            void navigate({ search: { ...search, account: undefined } })
                          }
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
                  <EmptyState>No posted activity in this period.</EmptyState>
                ) : flow ? (
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
              {definition.benefits ? null : (
                <ReportBreakdown
                  view={search.view ?? 'breakdown'}
                  group={search.view ? search.group : undefined}
                  onChange={(nextView, nextGroup) =>
                    void navigate({
                      search: (current) => ({
                        ...current,
                        view: nextView === 'transactions' ? nextView : undefined,
                        group: nextView === 'transactions' ? nextGroup : undefined,
                      }),
                      replace: true,
                    })
                  }
                  report={report}
                  colors={groupColors}
                  activities={activities}
                  unavailable={unavailable}
                  referenceIso={data.updatedAt}
                  activitySearch={activitySearch}
                  briefTitles={Boolean(definition.briefTitles)}
                />
              )}
            </div>
            <div className="analytics-companion">
              <Card className="account-overview-summary-card">
                <SectionHeading title="Overview" />
                <div className="account-summary-metrics">
                  <Metric
                    label={definition.records?.count ?? 'Posted records'}
                    value={unavailable ? '—' : String(report.entries.length)}
                  />
                  <Metric
                    label="Previous period"
                    value={formatCurrency(unavailable ? null : report.previous)}
                  />
                  <Metric
                    label={definition.records?.average ?? 'Average amount'}
                    value={formatCurrency(unavailable ? null : report.average)}
                  />
                  <Metric
                    label={definition.records?.largest ?? 'Largest amount'}
                    value={formatCurrency(unavailable ? null : report.largest)}
                  />
                  <Metric
                    label="Active days"
                    value={unavailable ? '—' : String(report.activeDays)}
                  />
                  <Metric
                    label="Period change"
                    value={formatPercent(unavailable ? null : report.percent)}
                    tone={valueTone(
                      unavailable || report.percent == null
                        ? null
                        : definition.lowerIsBetter
                          ? -report.percent
                          : report.percent,
                    )}
                  />
                </div>
              </Card>
              {definition.benefits ? (
                <>
                  {activityCard}
                  {expectedActivityCard}
                </>
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
                          <AccountMark
                            type={item.type}
                            institution={item.institution}
                            domain={item.domain}
                          />
                          <span className="account-row-name">
                            <strong>{item.label}</strong>
                            <small>{item.type}</small>
                          </span>
                          <strong>
                            {formatCurrency(
                              needsSpendingAccount || !reportWindow.valid ? null : item.value,
                            )}
                          </strong>
                        </label>
                      ))}
                      {!accountRows.length ? (
                        <EmptyState>No matching accounts in this period.</EmptyState>
                      ) : null}
                    </div>
                  </Card>
                  {expectedActivityCard}
                </>
              )}
            </div>
          </div>
        </div>
      )}
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
                                    data-command="navigation.open./activities"
                                    to="/activities"
                                    search={{ ...search, from: bucket.from, to: bucket.to }}
                                    aria-label={`${group}, ${formatDay(bucket.from)} through ${formatDay(bucket.to)}: ${formatCurrency(total)}. View period activity`}
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
                                            {merchant} · {formatDay(entry.date)}
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

/** ⌘← and ⌘→ through a destination's reports where no strip heads them. */
function ReportStepper({
  reports,
  selected,
}: {
  reports: AnalyticsChart[]
  selected: AnalyticsChart
}) {
  const navigate = useNavigate({ from: '/analytics' })
  useGraphAccountShortcuts((direction) => {
    const index = reports.indexOf(selected)
    void navigate({
      search: (current) => ({
        ...withoutReportView(current),
        chart: reports[(index + direction + reports.length) % reports.length],
      }),
    })
  })
  return null
}

type ReportView = 'breakdown' | 'transactions'
const reportViews: Array<{ value: ReportView; label: string; accessibleLabel: string }> = [
  { value: 'breakdown', label: 'Breakdown', accessibleLabel: 'Breakdown' },
  { value: 'transactions', label: 'Transactions', accessibleLabel: 'Transactions' },
]

/**
 * The report's lower card: its groups largest first, or the transactions behind them. Choosing
 * a group switches to its transactions, so the activity is listed once rather than under every
 * expanded row.
 */
function ReportBreakdown({
  view,
  group: focusGroup,
  onChange,
  report,
  colors,
  activities,
  unavailable,
  referenceIso,
  activitySearch,
  briefTitles,
}: {
  view: ReportView
  group?: string
  /** The view and group live in the URL, so a report reopens, and links, as it was left. */
  onChange: (view: ReportView, group?: string) => void
  report: ReturnType<typeof analyticsReport> & { label: string }
  colors: ReadonlyMap<string, string>
  activities: ReadonlyMap<string, ActivityItem>
  unavailable: boolean
  referenceIso: string
  activitySearch: { analysis: AnalyticsChart; account?: string; from: string; to: string }
  /** Income lists its payers by a short name, as its chart does. */
  briefTitles: boolean
}) {
  const latest = useRef({ onChange, focusGroup })
  latest.current = { onChange, focusGroup }
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        ...reportViews.map(({ value, accessibleLabel }) => ({
          id: `report.view.${value}`,
          title: `Show report ${accessibleLabel.toLowerCase()}`,
          search: 'report breakdown transactions activity groups',
          group: 'View',
          scope: 'page' as const,
          run: () => latest.current.onChange(value, latest.current.focusGroup),
        })),
        {
          id: 'report.group.clear',
          title: 'Show every group’s transactions',
          search: 'report clear group filter transactions',
          group: 'View',
          scope: 'page',
          run: () => latest.current.onChange('transactions'),
        },
      ],
      [],
    ),
  )
  const transactions = report.entries
    .filter(({ group }) => !focusGroup || group === focusGroup)
    .flatMap((entry) => {
      const activity = activities.get(entry.id)
      return activity
        ? [
            {
              ...activity,
              title: briefTitles ? briefMerchant(activity.title) : activity.title,
              date: entry.date,
            },
          ]
        : []
    })
    .toSorted((left, right) => right.date.localeCompare(left.date))

  return (
    <Card className="account-overview-summary-card analytics-breakdown-card">
      <SectionHeading
        title={view === 'breakdown' ? 'Breakdown' : 'Transactions'}
        action={
          <ChartRangeSelect<ReportView>
            label={`Report view: ${view}`}
            command="report.view."
            value={view}
            options={reportViews}
            onValueChange={(next) =>
              onChange(next, next === 'transactions' ? focusGroup : undefined)
            }
          />
        }
      />
      {view === 'breakdown' ? (
        <RankedBars
          items={report.breakdown.map(({ label, value }) => ({
            name: label,
            value: unavailable ? 0 : (value ?? 0),
            color: colors.get(label) ?? 'var(--text-tertiary)',
          }))}
          label={`${report.label} by group`}
          emptyMessage="No posted activity in this period."
          selected={null}
          onSelect={(name) => {
            if (name) onChange('transactions', name)
          }}
        />
      ) : (
        <>
          {focusGroup ? (
            <div className="report-group-scope">
              <button
                type="button"
                className="query-chip"
                aria-label={`Remove group filter: ${focusGroup}`}
                data-command="report.group.clear"
                onClick={() => onChange('transactions')}
              >
                <span>Group</span>
                {focusGroup}
                <X size={12} aria-hidden="true" />
              </button>
              <span className="report-group-count">
                {countOf(transactions.length, 'transaction')}
              </span>
            </div>
          ) : null}
          <div
            className="analytics-activity-scroll report-transactions-scroll"
            role="region"
            aria-label={`${report.label} transactions`}
            tabIndex={0}
            data-keyboard-region
          >
            <GroupedActivityList
              activities={unavailable ? [] : transactions}
              referenceIso={referenceIso}
              emptyMessage="No posted activity in this period."
            />
          </div>
          <Link
            className="ranked-bars-details-more report-open-activity"
            to="/activities"
            search={activitySearch}
            data-command="navigation.open./activities"
          >
            Open in Activity <ChevronRight size={14} aria-hidden="true" />
          </Link>
        </>
      )}
    </Card>
  )
}
