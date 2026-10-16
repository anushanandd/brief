import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { type RefObject, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { PagedActivityList } from '../components/activity-list'
import { SpendingBarChart } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { ChevronRight, X } from '../components/icons'
import { groupByMerchant, MerchantGroups } from '../components/merchant-groups'
import { PlatinumBenefits } from '../components/platinum-benefits'
import { RankedBars } from '../components/ranked-bars'
import { MoneyStrip } from '../components/report-links'
import { SpendingAccountEmptyState } from '../components/spending-account-empty-state'
import { SpendingPaceChart } from '../components/spending-pace-chart'
import { SubscriptionsCard } from '../components/subscriptions-card'
import { TransactionDialog } from '../components/transaction-dialog'
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
import { type SpendingComparison, useSpendingActions } from '../hooks/use-spending-actions'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { buildActivities, type ActivityItem } from '../lib/activity'
import { daysBetween } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import { formatProseCurrency, pluralize, formatDay } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import type { SpendingOwner } from '../lib/schema'
import {
  buildSpendingBars,
  allSpendingAccounts,
  buildSpendingView,
  spendingFor,
  spendingJudgments,
  spendingOwnerOptions,
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

function useActivityData(requested: string | undefined, requestedOwner: SpendingOwner) {
  const query = useFinance()
  const data = query.data
  const settings = financeSettings(data)
  const names = settings.accountNames
  // Whose spending to count is a choice only once a purchase is someone else's.
  const hasOthers = useMemo(
    () => Boolean(data?.transactions.some(({ classification }) => classification.othersSpending)),
    [data],
  )
  const owner = hasOthers ? requestedOwner : 'me'
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
  const transactions = useMemo(
    () => spendingFor(scope?.transactions ?? [], owner),
    [owner, scope?.transactions],
  )
  return {
    query,
    data,
    names,
    // The one account in view, or undefined for every account.
    account: scope?.selected,
    card: scope?.card,
    options: scope?.options ?? [],
    hasOthers,
    owner,
    transactions,
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
  const { query, data, names, account, card, options, hasOthers, owner, transactions } =
    useActivityData(search.account, search.for ?? 'me')
  const setOwner = useCallback(
    (next: SpendingOwner) =>
      void navigate({
        search: (current) => ({ ...current, for: next === 'me' ? undefined : next }),
        replace: true,
      }),
    [navigate],
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
  const [linked, setLinked] = useState<{
    ids: ReadonlySet<string>
    from: 'chart' | 'list'
  } | null>(null)
  // The activity in the details dialog, and the list it opened from, which ↑ and ↓ step through.
  const [opened, setOpened] = useState<{ id: string; list: readonly string[] } | null>(null)
  const opener = useRef<HTMLElement | null>(null)
  const closeDetails = useCallback(() => {
    const row = opener.current
    setOpened(null)
    // The row that opened the dialog takes focus back, so keyboard review continues in place.
    requestAnimationFrame(() => row?.focus({ preventScroll: true }))
  }, [])
  const stepDetails = useCallback(
    (direction: 1 | -1) =>
      setOpened((current) => {
        const next = current?.list[current.list.indexOf(current.id) + direction]
        return current && next ? { ...current, id: next } : current
      }),
    [],
  )
  // Both lists reach the foot of the window, so neither card ends in a blank band.
  const activityScrollRef = useViewportScroll()
  const merchantScrollRef = useViewportScroll()
  const referenceIso = data?.updatedAt ?? new Date().toISOString()
  // Statement boundaries come from the issuer or native autopay evidence for this card.
  const statement =
    account && data?.insights?.benefits?.accountId === account.id
      ? data.insights.statement
      : undefined
  const periodReference = spendingPeriodReference(statement, referenceIso, periodOffset, period)
  const judgments = useMemo(
    () => spendingJudgments(data?.insights, account?.id ?? allSpendingAccounts, owner),
    [account?.id, data?.insights, owner],
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

  // Categories compare with the last period or with the average of earlier ones.
  const [chosenComparison, setComparison] = useState<SpendingComparison>('previous')
  const canCompareTypical = period !== 0 && view.categories.some(({ typical }) => typical != null)
  const comparison = canCompareTypical ? chosenComparison : 'previous'

  useSpendingActions({
    hasStatements,
    hasOthers,
    owner,
    setOwner,
    focus,
    period,
    periodOffset,
    choosePeriod,
    setPeriodOffset,
    clearFocus,
    comparison,
    canCompareTypical,
    setComparison,
  })

  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const selectedPeriod =
    view.periodBasis === 'statement' || period === 'week'
      ? formatDay(view.end)
      : monthYearFormatter.format(new Date(`${view.end}T00:00:00Z`))
  const periodLabel = spendingPeriodLabel(period, periodOffset, selectedPeriod, view.periodBasis)
  const shown = focusView ?? view
  // The headline is always what was spent; a statement's balance reads beside its name, and is
  // never only someone else's.
  const statementBalance =
    view.periodBasis === 'statement' && period === 'statement' && owner !== 'other'
      ? view.statementBalance
      : null
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
        ? ' · closes today'
        : ` · closes in ${closingDays} ${pluralize(closingDays, 'day')}`
  const bars = buildSpendingBars(shown.transactions, view.start, view.end, period)
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
  // Your own spending still sums everything on the card, so its statement balance stays whole,
  // but lists only what is yours.
  const visibleActivities = activities.filter(
    ({ id, other }) => (!focusIds || focusIds.has(id)) && (owner !== 'me' || !other),
  )
  const merchantOrder = groupByMerchant(visibleActivities).flatMap((group) => group.activities)
  const openFrom = (list: ActivityItem[]) => (activity: ActivityItem) => {
    if (document.activeElement instanceof HTMLElement) opener.current = document.activeElement
    setOpened({ id: activity.id, list: list.map(({ id }) => id) })
  }
  // Built from the snapshot's own transaction, so the dialog reads as it does on Activity
  // whichever spending this page counts.
  const openedActivity = opened
    ? buildActivities(
        {
          ...data,
          transactions: data.transactions.filter(({ id }) => `spending:${id}` === opened.id),
          trades: [],
        },
        names,
        getExternalLogosEnabled(),
      )[0]
    : undefined
  // Rows the chart highlights stay rendered so the list can show them.
  const highlightedThrough =
    linked?.from === 'chart'
      ? visibleActivities.findLastIndex(({ id }) => linked.ids.has(id)) + 1
      : 0
  const categoryBars = view.categories.map(({ name, value, change, previousValue, typical }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
    change:
      period === 0
        ? null
        : comparison === 'typical'
          ? typical == null
            ? null
            : Math.round((value - typical) * 100) / 100
          : change,
    mark: period === 0 ? null : comparison === 'typical' ? typical : previousValue,
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
                aria-keyshortcuts={`${hasStatements ? 'S ' : ''}W M Q Y A ${hasOthers ? 'F ' : ''}ArrowLeft ArrowRight`}
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
                        {focus ? (
                          `${periodLabel} · ${focus}`
                        ) : statementBalance != null ? (
                          <>
                            {periodOffset === 0
                              ? 'Statement balance '
                              : `${periodLabel} · balance `}
                            <span className="sensitive-value">
                              {formatProseCurrency(statementBalance)}
                            </span>
                            {closingLabel}
                          </>
                        ) : (
                          `${periodLabel}${closingLabel}`
                        )}
                      </h2>
                      <AnimatedCurrency className="hero-number" value={shown.total} />
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
                  <span className="spending-heading-controls">
                    {hasOthers ? (
                      <ChartRangeSelect
                        command="spending.for."
                        label={`Whose spending. Showing: ${spendingOwnerOptions.find(({ value }) => value === owner)!.label}`}
                        options={spendingOwnerOptions.map(({ value, label }) => ({
                          label,
                          accessibleLabel: label,
                          value,
                        }))}
                        value={owner}
                        onValueChange={setOwner}
                      />
                    ) : null}
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
                  </span>
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
                            for: hasOthers && owner !== 'all' ? owner : undefined,
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

            {/* Where the money went beside the latest of it, each scrolling in its own card. */}
            <div className="spending-activity-columns">
              <ScrollCueCard
                className="spending-transactions-preview spending-ledger-card"
                scrollSelector=".spending-activity-scroll"
              >
                <SectionHeading title="Where it went" />
                <div
                  ref={merchantScrollRef}
                  className="spending-activity-scroll"
                  role="region"
                  aria-label="Where it went"
                  tabIndex={0}
                  data-keyboard-region
                >
                  <MerchantGroups
                    activities={visibleActivities}
                    referenceIso={data.updatedAt}
                    emptyMessage={
                      focus ? `No ${focus} activity in this range.` : 'No activity in this range.'
                    }
                    onOpen={openFrom(merchantOrder)}
                  />
                </div>
              </ScrollCueCard>
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
                        for: hasOthers && owner !== 'all' ? owner : undefined,
                        from: view.start,
                        to: view.end,
                      }}
                      className="section-heading-link"
                      data-command="navigation.open./activities"
                    >
                      Activities <ChevronRight size={14} aria-hidden="true" />
                    </Link>
                  }
                />
                <div
                  ref={activityScrollRef}
                  className="spending-activity-scroll"
                  role="region"
                  aria-label="Spending activities"
                  tabIndex={0}
                  data-keyboard-region
                >
                  <PagedActivityList
                    minimumCount={highlightedThrough}
                    activities={visibleActivities}
                    onOpen={openFrom(visibleActivities)}
                    selectedId={opened?.id}
                    referenceIso={data.updatedAt}
                    emptyMessage={
                      focus ? `No ${focus} activity in this range.` : 'No activity in this range.'
                    }
                    highlightedIds={linked?.from === 'chart' ? linked.ids : undefined}
                    onHighlight={
                      period === 0
                        ? undefined
                        : (activity) =>
                            setLinked(
                              activity ? { ids: new Set([activity.id]), from: 'list' } : null,
                            )
                    }
                  />
                </div>
              </ScrollCueCard>
            </div>
          </div>

          <div className="spending-companion">
            <Card className="spending-category-card">
              <SectionHeading
                title="Categories"
                action={
                  period === 0 ? undefined : canCompareTypical ? (
                    <div
                      className="spending-tabs"
                      role="tablist"
                      aria-label="Compare categories with"
                    >
                      {(
                        [
                          ['previous', `Last ${unit}`],
                          ['typical', 'Average'],
                        ] as const
                      ).map(([value, label]) => (
                        <button
                          type="button"
                          role="tab"
                          key={value}
                          aria-selected={comparison === value}
                          tabIndex={-1}
                          data-command={`spending.compare.${value}`}
                          onClick={() => setComparison(value)}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  ) : (
                    <span className="spending-category-scope">
                      <i className="ranked-bars-mark-key" aria-hidden="true" />
                      {`vs last ${unit}`}
                    </span>
                  )
                }
              />
              <RankedBars
                comparison={comparison === 'typical' ? 'the average' : `the last ${unit}`}
                markLabel={
                  comparison === 'typical'
                    ? 'on average by this point'
                    : `by this point last ${unit}`
                }
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
                <PlatinumBenefits accountId={account.id} />
              ) : null
            ) : card ? (
              <PlatinumBenefits />
            ) : (
              <SpendingAccountEmptyState />
            )}
            <SubscriptionsCard data={data} className="spending-subscriptions-card" />
          </div>
        </div>
      )}
      <TransactionDialog
        activity={openedActivity}
        data={data}
        onClose={closeDetails}
        onStep={stepDetails}
      />
    </div>
  )
}
