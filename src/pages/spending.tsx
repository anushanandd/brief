import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { type RefObject, useCallback, useEffect, useMemo, useState } from 'react'

import { PagedActivityList } from '../components/activity-list'
import { SpendingBarChart } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { ChevronRight, X } from '../components/icons'
import { PlatinumBenefits } from '../components/platinum-benefits'
import { RankedBars } from '../components/ranked-bars'
import { MoneyStrip } from '../components/report-links'
import { SpendingAccountEmptyState } from '../components/spending-account-empty-state'
import { SpendingPaceChart } from '../components/spending-pace-chart'
import { TripsCard } from '../components/trips-card'
import {
  AnimatedCurrency,
  Card,
  ChartChange,
  ChartRangeSelect,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useSpendingActions } from '../hooks/use-spending-actions'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { buildActivities } from '../lib/activity'
import { daysBetween } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import { countOf, formatProseCurrency, pluralize, formatDay } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildSpendingBars,
  allSpendingAccounts,
  buildSpendingView,
  platinumCreditSummary,
  spendingJudgments,
  spendingScope,
  spendingCategoryColor,
  type SpendingPeriod,
  spendingPeriodLabel,
  spendingPeriodForRange,
  spendingPeriodOptions,
  spendingPeriodReference,
  spendingRangeForPeriod,
} from '../lib/spending'
import { isSpendingTransaction, splitParts } from '../lib/transaction-kind'

const monthYearFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})

const spendingPeriodIndicators = spendingPeriodOptions.map(
  ({ key, label, value }): { label: string; accessibleLabel: string; value: SpendingPeriod } => ({
    label: key.toUpperCase(),
    accessibleLabel: label,
    value,
  }),
)

function useActivityData(requested: string | undefined) {
  const query = useFinance()
  const data = query.data
  const settings = financeSettings(data)
  const names = settings.accountNames
  const scope = useMemo(
    () =>
      data
        ? spendingScope(
            data.accounts,
            data.transactions,
            settings.spendingAccountId ?? '',
            requested,
          )
        : undefined,
    [data, requested, settings.spendingAccountId],
  )
  return {
    query,
    data,
    names,
    // The one account in view, or undefined for every account.
    account: scope?.selected,
    card: scope?.card,
    options: scope?.options ?? [],
    transactions: scope?.transactions ?? [],
  }
}

/**
 * Keeps the rows the chart links to in view inside the list's own scroll area, never the
 * page's.
 */
function useLinkedRowInView(scroll: RefObject<HTMLElement | null>, linked: object | null) {
  useEffect(() => {
    const container = scroll.current
    if (!linked || !container) return
    const row = container.querySelector<HTMLElement>('[data-highlighted]')
    if (!row) return
    const top = row.getBoundingClientRect().top - container.getBoundingClientRect().top
    const bottom = top + row.offsetHeight
    if (top >= 0 && bottom <= container.clientHeight) return
    container.scrollTo({
      top: container.scrollTop + top - 8,
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth',
    })
  }, [scroll, linked])
}

export function SpendingPage() {
  const search = useSearch({ from: '/spending' })
  const navigate = useNavigate({ from: '/spending' })
  const { query, data, names, account, card, options, transactions } = useActivityData(
    search.account,
  )
  // The period lives in the URL so Money's strip opens every card on it.
  // Statements belong to the spending card; without one the page opens on the month.
  const hasStatements = Boolean(card)
  const period = spendingPeriodForRange(search.range) ?? (hasStatements ? 'statement' : 1)
  const setPeriod = useCallback(
    (next: SpendingPeriod) =>
      void navigate({
        search: (current) => ({ ...current, range: spendingRangeForPeriod(next) }),
        replace: true,
      }),
    [navigate],
  )
  const [periodOffset, setPeriodOffset] = useState(0)
  const choosePeriod = useCallback(
    (next: SpendingPeriod) => {
      setPeriod(next)
      setPeriodOffset(0)
    },
    [setPeriod],
  )
  // One category can focus the page: the headline, chart, and activity narrow to it.
  const [focus, setFocus] = useState<string | null>(null)
  const clearFocus = useCallback(() => setFocus(null), [])
  // Activity shared between the chart and the list: the day under the chart's pointer, or the
  // row under the list's.
  const [selectedTrip, setSelectedTrip] = useState<string>()
  const [linked, setLinked] = useState<{
    ids: ReadonlySet<string>
    from: 'chart' | 'list'
  } | null>(null)
  const activityScrollRef = useViewportScroll()
  const referenceIso = data?.updatedAt ?? new Date().toISOString()
  // Statement boundaries come from the issuer or native autopay evidence for this card.
  const statement =
    account && data?.insights?.benefits?.accountId === account.id
      ? data.insights.statement
      : undefined
  const periodReference = spendingPeriodReference(statement, referenceIso, periodOffset, period)
  const judgments = useMemo(
    () => spendingJudgments(data?.insights, account?.id ?? allSpendingAccounts),
    [account?.id, data?.insights],
  )
  const view = useMemo(
    () =>
      buildSpendingView(transactions, referenceIso, period, periodReference, judgments, statement),
    [judgments, period, periodReference, referenceIso, statement, transactions],
  )
  const focusView = useMemo(
    () =>
      focus
        ? buildSpendingView(
            // A split transaction contributes only the part in the focused category.
            transactions
              .flatMap(splitParts)
              .filter(
                (transaction) =>
                  transaction.category === focus && isSpendingTransaction(transaction),
              ),
            referenceIso,
            period,
            periodReference,
            judgments,
            statement,
            focus,
          )
        : null,
    [focus, judgments, period, periodReference, referenceIso, statement, transactions],
  )

  useLinkedRowInView(activityScrollRef, linked?.from === 'chart' ? linked : null)

  useSpendingActions({
    hasStatements,
    focus,
    period,
    periodOffset,
    choosePeriod,
    setPeriodOffset,
    clearFocus,
  })

  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const selectedPeriod =
    view.periodBasis === 'statement' || period === 'week'
      ? formatDay(view.end)
      : monthYearFormatter.format(new Date(`${view.end}T00:00:00Z`))
  const periodLabel = spendingPeriodLabel(period, periodOffset, selectedPeriod, view.periodBasis)
  const shown = focusView ?? view
  const headlineValue = focusView
    ? focusView.total
    : view.periodBasis === 'statement' && period === 'statement'
      ? view.statementBalance
      : view.total
  const spendingChange = period ? shown.total - shown.previousTotal : null
  const closingDate =
    period === 'statement' && periodOffset === 0 ? statement?.nextClose : undefined
  const closingDays = closingDate
    ? daysBetween(data.insights?.today ?? referenceIso.slice(0, 10), closingDate)
    : null
  const closingLabel =
    closingDays == null || closingDays < 0
      ? ''
      : closingDays === 0
        ? ' (closes today)'
        : ` (closes in ${closingDays} ${pluralize(closingDays, 'day')})`
  const bars = buildSpendingBars(shown.transactions, referenceIso, view.start, view.end, period)
  const segments = view.categories.map(({ name, value }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
  }))
  const chartCategories = bars.some(({ categories }) =>
    categories.some(({ name }) => name === 'Credits'),
  )
    ? [...segments, { name: 'Credits', value: 0, color: 'var(--positive)' }]
    : segments
  const credits = platinumCreditSummary(
    data.insights?.benefits,
    view.start,
    view.end,
    financeSettings(data).hiddenBenefitIds,
  )
  // A statement period without statements is the calendar month.
  const unit =
    period === 'statement' && view.periodBasis !== 'statement'
      ? 'month'
      : spendingPeriodOptions.find((option) => option.value === period)!.unit
  const activities = buildActivities(
    { ...data, transactions: view.transactions, trades: [] },
    names,
    getExternalLogosEnabled(),
  )
  const activityById = new Map(activities.map((activity) => [activity.id, activity]))
  const focusIds = focusView
    ? new Set(focusView.transactions.map(({ id }) => `spending:${id}`))
    : null
  const visibleActivities = focusIds ? activities.filter(({ id }) => focusIds.has(id)) : activities
  // Rows the chart highlights stay rendered so the list can show them.
  const highlightedThrough =
    linked?.from === 'chart'
      ? visibleActivities.findLastIndex(({ id }) => linked.ids.has(id)) + 1
      : 0
  const hasTypical = period !== 0 && view.categories.some(({ typical }) => typical != null)
  const categoryBars = view.categories.map(({ name, value, change, typical }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
    change: period === 0 ? null : change,
    typical: period === 0 ? null : typical,
  }))
  const focusColor = focus ? spendingCategoryColor(focus) : undefined

  return (
    <div className="page spending-workspace-page">
      <WorkspaceHeader
        title="Money"
        breadcrumbs={
          search.account
            ? [
                { label: 'Money', to: '/spending', search: { range: search.range } },
                {
                  label: account
                    ? accountDisplayName(account.id, account.name, names)
                    : 'All accounts',
                  to: '/spending',
                  search: { account: search.account, range: search.range },
                },
              ]
            : undefined
        }
      />
      <MoneyStrip
        data={data}
        current="/spending"
        period={period}
        spendingAccount={search.account}
        today={data.insights?.today}
      />
      {!options.length ? (
        <SpendingAccountEmptyState />
      ) : (
        <div className="spending-overview-grid">
          <div className="spending-main">
            <Card className="workspace-brief-card spending-brief-card spending-hero-card">
              <section
                className="spending-total-panel"
                aria-keyshortcuts={`${hasStatements ? 'S ' : ''}W M Q Y A ArrowLeft ArrowRight`}
              >
                <header className="workspace-brief-heading">
                  <div>
                    <div className="spending-balance-stack">
                      <h2
                        className="balance-label"
                        aria-description={
                          view.periodBasis === 'statement'
                            ? statement?.source === 'issuer'
                              ? 'Period from issuer-reported statement dates'
                              : 'Period inferred from recurring posted autopay dates'
                            : undefined
                        }
                      >
                        {focus ? `${periodLabel} · ${focus}` : `${periodLabel}${closingLabel}`}
                      </h2>
                      <AnimatedCurrency className="hero-number" value={headlineValue} />
                    </div>
                    <div className="chart-summary-row">
                      {spendingChange != null ? (
                        <ChartChange
                          amount={spendingChange}
                          percent={shown.percentChange}
                          favorable="decrease"
                          ariaLabel="Change versus prior period"
                        />
                      ) : null}
                      <span className="spending-daily-average sensitive-value">
                        {`${formatProseCurrency(shown.dailyAverage)} a day`}
                      </span>
                      {focus ? (
                        <button
                          type="button"
                          className="spending-focus-chip"
                          onClick={clearFocus}
                          aria-label={`Show all categories instead of ${focus}`}
                          aria-keyshortcuts="Escape"
                        >
                          <i style={{ background: focusColor }} aria-hidden="true" />
                          {focus}
                          <X size={12} aria-hidden="true" />
                        </button>
                      ) : null}
                    </div>
                  </div>
                  <ChartRangeSelect
                    command="spending.period."
                    label={`Spending range. Active period: ${periodLabel}`}
                    options={
                      hasStatements
                        ? spendingPeriodIndicators
                        : spendingPeriodIndicators.filter(({ value }) => value !== 'statement')
                    }
                    value={period}
                    onValueChange={choosePeriod}
                  />
                </header>
                <div
                  className="spending-chart-region"
                  role="region"
                  aria-label="Spending chart"
                  tabIndex={0}
                  data-range-focus-region
                >
                  {period === 0 ? (
                    <SpendingBarChart
                      data={bars}
                      label={focus ? `${periodLabel} · ${focus}` : periodLabel}
                      categories={
                        focus
                          ? chartCategories.filter(({ name }) => name === focus)
                          : chartCategories
                      }
                      activities={activityById}
                    />
                  ) : (
                    <SpendingPaceChart
                      pace={shown.pace}
                      color={focusColor}
                      highlightedIds={linked?.from === 'list' ? linked.ids : undefined}
                      onSelectRange={(from, to) =>
                        void navigate({
                          to: '/activities',
                          search: {
                            account: account?.id,
                            category: focus ?? undefined,
                            from,
                            to,
                          },
                        })
                      }
                      onScrubDay={(ids) =>
                        setLinked(ids ? { ids: new Set(ids), from: 'chart' } : null)
                      }
                      currentLabel={periodOffset === 0 ? `This ${unit}` : `Selected ${unit}`}
                      previousLabel={periodOffset === 0 ? `Last ${unit}` : `Prior ${unit}`}
                      activities={activityById}
                    />
                  )}
                </div>
                {period === 0 && chartCategories.length ? (
                  <ul
                    className="analytics-legend bar-chart-legend"
                    aria-label="Chart legend"
                    tabIndex={0}
                  >
                    {chartCategories.map(({ name, color }) => (
                      <li key={name}>
                        <i
                          className="analytics-category-dot"
                          style={{ background: color }}
                          aria-hidden="true"
                        />
                        {name}
                      </li>
                    ))}
                  </ul>
                ) : null}
              </section>
            </Card>

            <ScrollCueCard
              className="spending-transactions-preview spending-ledger-card"
              scrollSelector=".spending-activity-scroll"
            >
              <SectionHeading
                title={
                  <Link
                    to="/activities"
                    search={{
                      account: account?.id,
                      category: focus ?? undefined,
                      from: view.start,
                      to: view.end,
                    }}
                    className="section-heading-link"
                    data-command="navigation.open./activities"
                  >
                    Activity <ChevronRight size={14} aria-hidden="true" />
                  </Link>
                }
                action={
                  <span className="spending-ledger-count">
                    {countOf(visibleActivities.length, 'transaction')}
                  </span>
                }
              />
              <div
                ref={activityScrollRef}
                className="spending-activity-scroll"
                role="region"
                aria-label="Spending activity"
                tabIndex={0}
                data-keyboard-region
              >
                <PagedActivityList
                  minimumCount={highlightedThrough}
                  activities={visibleActivities}
                  referenceIso={data.updatedAt}
                  emptyMessage={
                    focus ? `No ${focus} activity in this range.` : 'No activity in this range.'
                  }
                  highlightedIds={linked?.from === 'chart' ? linked.ids : undefined}
                  onHighlight={
                    period === 0
                      ? undefined
                      : (activity) =>
                          setLinked(activity ? { ids: new Set([activity.id]), from: 'list' } : null)
                  }
                />
              </div>
            </ScrollCueCard>
          </div>

          <div className="spending-companion">
            <Card className="spending-category-card">
              <SectionHeading
                title="Categories"
                action={
                  period === 0 ? undefined : (
                    <span className="spending-category-scope">
                      {hasTypical ? (
                        <>
                          <i className="ranked-bars-typical-key" aria-hidden="true" />
                          Usual ·{' '}
                        </>
                      ) : null}
                      vs last {unit}
                    </span>
                  )
                }
              />
              <RankedBars
                items={categoryBars}
                label={`${periodLabel} spending by category`}
                emptyMessage="No spending in this range."
                selected={focus}
                onSelect={(name) => {
                  setFocus(name)
                  setLinked(null)
                }}
              />
            </Card>
            {account && account.id !== card?.id ? (
              // Another card shows its own benefits only when a product is assigned to it.
              data.insights?.otherCardBenefits?.some((other) => other.accountId === account.id) ? (
                <PlatinumBenefits preview accountId={account.id} />
              ) : null
            ) : card ? (
              <PlatinumBenefits preview credits={credits} />
            ) : (
              <SpendingAccountEmptyState />
            )}
            <TripsCard
              trips={(data.insights?.trips.trips ?? []).filter(
                (trip) => trip.end >= view.start && trip.start <= view.end,
              )}
              referenceIso={data.updatedAt}
              selectedId={selectedTrip}
              onSelect={(trip) => {
                setSelectedTrip(trip?.id)
                setFocus(null)
                setLinked(
                  trip
                    ? {
                        ids: new Set(trip.transactionIds.map((id) => `spending:${id}`)),
                        from: 'chart',
                      }
                    : null,
                )
              }}
            />
          </div>
        </div>
      )}
    </div>
  )
}
