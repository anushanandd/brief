import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { type RefObject, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { PagedActivityList } from '../components/activity-list'
import { CardBenefits } from '../components/card-benefits'
import { SpendingBarChart } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { ChevronRight, X } from '../components/icons'
import { MerchantGroups, type MerchantGroup } from '../components/merchant-groups'
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
import {
  type SpendingBreakdown,
  type SpendingComparison,
  useSpendingActions,
} from '../hooks/use-spending-actions'
import { useSpendingPeriod } from '../hooks/use-spending-period'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { buildActivities, type ActivityItem } from '../lib/activity'
import { daysBetween } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  formatCalendarDate,
  formatMonthYear,
  formatProseCurrency,
  pluralize,
  formatDay,
} from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import type { SpendingOwner } from '../lib/schema'
import {
  allSpendingAccounts,
  spendingChartBars,
  spendingChartPace,
  spendingFigures,
  spendingOwnerOptions,
  spendingCategoryColor,
  spendingPage,
  type SpendingPeriod,
  spendingPeriodLabel,
  spendingPeriodOptions,
  spendingPeriodRequest,
  spendingRangeForPeriod,
  type SpendingSearch,
  spendingView,
} from '../lib/spending'

const spendingPeriodIndicators = spendingPeriodOptions.map(
  ({ key, label, value }): { label: string; accessibleLabel: string; value: SpendingPeriod } => ({
    label: key.toUpperCase(),
    accessibleLabel: label,
    value,
  }),
)

function useActivityData(search: SpendingSearch) {
  const query = useFinance()
  const data = query.data
  const { account, range, for: owner } = search
  // The same page the route loaded its period for (`route-data`).
  const page = useMemo(
    () => (data ? spendingPage(data, { account, range, for: owner }) : undefined),
    [data, account, range, owner],
  )
  return {
    query,
    data,
    names: page?.names ?? financeSettings(data).accountNames,
    // The one account in view, or undefined for every account.
    account: page?.account,
    card: page?.card,
    options: page?.options ?? [],
    hasOthers: page?.hasOthers ?? false,
    owner: page?.owner ?? 'me',
    hasStatements: page?.hasStatements ?? false,
    period: page?.period ?? 1,
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
  const { query, data, names, account, card, options, hasOthers, owner, hasStatements, period } =
    useActivityData(search)
  const setOwner = useCallback(
    (next: SpendingOwner) =>
      void navigate({
        search: (current) => ({ ...current, for: next === 'me' ? undefined : next }),
        replace: true,
      }),
    [navigate],
  )
  const setAccount = useCallback(
    (next: string) =>
      void navigate({
        search: (current) => ({
          ...current,
          account: next === allSpendingAccounts ? undefined : next,
        }),
        replace: true,
      }),
    [navigate],
  )
  // The period lives in the URL so Money's strip opens every card on it.
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
  // Each account counts its periods back from its own latest, so a choice starts there.
  const chooseAccount = useCallback(
    (next: string) => {
      setAccount(next)
      setPeriodOffset(0)
    },
    [setAccount],
  )
  const accountChoices = useMemo(
    () => [
      { value: allSpendingAccounts, label: 'All accounts' },
      ...options.map(({ id, name }) => ({
        value: id,
        label: accountDisplayName(id, name, names),
      })),
    ],
    [names, options],
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
  // Rust reads the period: its figures, list, merchants, pace, and each category's view.
  const periodQuery = useSpendingPeriod(
    data,
    spendingPeriodRequest({ account, owner, period }, periodOffset),
  )
  const detail = periodQuery.data ?? undefined

  useLinkedRowInView(activityScrollRef, linked?.from === 'chart' ? linked : null)

  // Categories compare with the last period or with the average of earlier ones.
  const [chosenComparison, setComparison] = useState<SpendingComparison>('previous')
  const [breakdown, setBreakdown] = useState<SpendingBreakdown>('categories')
  const periodCategories = detail?.categories ?? []
  const canCompareTypical = period !== 0 && periodCategories.some(({ typical }) => typical != null)
  const comparison = canCompareTypical ? chosenComparison : 'previous'

  useSpendingActions({
    accounts: accountChoices,
    account: account?.id ?? allSpendingAccounts,
    chooseAccount,
    hasStatements,
    hasOthers,
    owner,
    setOwner,
    focus,
    period,
    periodOffset,
    // Back stops where the saved history does, and waits for the period it is leaving.
    hasEarlier: Boolean(detail?.earlier) && !periodQuery.isPlaceholderData,
    choosePeriod,
    setPeriodOffset,
    clearFocus,
    comparison,
    canCompareTypical,
    setComparison,
    breakdown,
    setBreakdown,
  })

  if (query.isLoading || (options.length > 0 && periodQuery.isLoading)) return <PageLoading />
  if (query.isError || periodQuery.isError || !data) return <PageError />

  const today = data.insights?.today ?? referenceIso.slice(0, 10)
  const start = detail?.start ?? today
  const end = detail?.end ?? today
  const basis = detail?.kind === 'statement' ? 'statement' : 'calendar'
  const selectedPeriod =
    basis === 'statement' || period === 'week'
      ? formatDay(end)
      : formatCalendarDate(end, { month: 'long', year: 'numeric' })
  const periodLabel = spendingPeriodLabel(period, periodOffset, selectedPeriod, basis)
  const shown = detail ? spendingView(detail, focus ?? undefined) : undefined
  const figures = spendingFigures(detail, focus ?? undefined)
  // The headline is always what was spent; a statement's balance reads beside its name, and is
  // never only someone else's.
  const statementBalance =
    basis === 'statement' && period === 'statement' && owner !== 'other'
      ? (detail?.balance ?? null)
      : null
  const spendingChange = period ? (figures?.change ?? null) : null
  const closingDate =
    period === 'statement' && periodOffset === 0 ? statement?.nextClose : undefined
  const closingDays = closingDate ? daysBetween(today, closingDate) : null
  const closingLabel =
    closingDays == null || closingDays < 0
      ? ''
      : closingDays === 0
        ? ' · closes today'
        : ` · closes in ${closingDays} ${pluralize(closingDays, 'day')}`
  const bars = spendingChartBars(detail?.bars ?? [], focus ?? undefined)
  // Without a period from Rust (no insights yet), the chart reads as empty.
  const pace = shown
    ? spendingChartPace(shown.pace)
    : { start, end, current: [], previous: [], projected: null, days: [] }
  const segments = periodCategories.map(({ name, total }) => ({
    name,
    value: total,
    color: spendingCategoryColor(name),
  }))
  const chartCategories = bars.some(({ categories }) =>
    categories.some(({ name }) => name === 'Credits'),
  )
    ? [...segments, { name: 'Credits', value: 0, color: 'var(--positive)' }]
    : segments
  // A statement period without statements is the calendar month.
  const unit =
    period === 'statement' && basis !== 'statement'
      ? 'month'
      : spendingPeriodOptions.find((option) => option.value === period)!.unit
  // Every transaction in the period, so the charts can name any purchase they mark.
  const listedIds = new Set(detail?.transactionIds)
  const activities = buildActivities(
    { ...data, transactions: data.transactions.filter(({ id }) => listedIds.has(id)), trades: [] },
    names,
    getExternalLogosEnabled(),
  )
  const activityById = new Map(activities.map((activity) => [activity.id, activity]))
  const activitiesFor = (ids: readonly string[]) =>
    ids.flatMap((id) => activityById.get(`spending:${id}`) ?? [])
  // Your own spending still sums everything on the card, so its statement balance stays whole,
  // but lists only what is yours; Rust lists what the choice and the focus count.
  const visibleActivities = activitiesFor(shown?.transactionIds ?? [])
  const merchantGroups: MerchantGroup[] = (shown?.merchants ?? []).map(
    ({ key, name, total, transactionIds }) => ({
      key,
      name,
      total,
      activities: activitiesFor(transactionIds),
    }),
  )
  const merchantOrder = merchantGroups.flatMap((group) => group.activities)
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
  const categoryBars = periodCategories.map(
    ({ name, total, change, previous, typical, aboveTypical }) => ({
      name,
      value: total,
      color: spendingCategoryColor(name),
      change: period === 0 ? null : comparison === 'typical' ? aboveTypical : change,
      mark: period === 0 ? null : comparison === 'typical' ? typical : previous,
    }),
  )
  const focusColor = focus ? spendingCategoryColor(focus) : undefined
  const placeBars = (detail?.places ?? []).map(({ name, total }) => ({
    name,
    value: total,
    color: 'var(--data-5)',
  }))
  const shownBreakdown = placeBars.length ? breakdown : 'categories'
  // A total reaching back to Wallet's first record says the history stops there.
  const historyFrom = data.wallet?.historyFrom
  const historyStart = historyFrom != null && start <= historyFrom ? historyFrom : undefined

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
                          basis === 'statement'
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
                            {periodOffset === 0 ? 'Balance ' : `${periodLabel} · balance `}
                            <span className="sensitive-value">
                              {formatProseCurrency(statementBalance)}
                            </span>
                            {closingLabel}
                          </>
                        ) : (
                          `${periodLabel}${closingLabel}`
                        )}
                      </h2>
                      <AnimatedCurrency className="hero-number" value={figures?.total} />
                    </div>
                    <div className="chart-summary-row">
                      {spendingChange != null ? (
                        <ChartChange
                          amount={spendingChange}
                          percent={figures?.changePct ?? null}
                          favorable="decrease"
                          ariaLabel="Change versus prior period"
                        />
                      ) : null}
                      {figures ? (
                        <span className="spending-daily-average sensitive-value">
                          {`${formatProseCurrency(figures.daily)} a day`}
                        </span>
                      ) : null}
                      {historyStart ? (
                        <span className="spending-daily-average">
                          {`Card history starts ${formatMonthYear(historyStart)}`}
                        </span>
                      ) : null}
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
                    {options.length > 1 ? (
                      <ChartRangeSelect
                        command="spending.account."
                        label={`Spending account. Showing: ${accountChoices.find(({ value }) => value === (account?.id ?? allSpendingAccounts))?.label}`}
                        options={accountChoices.map(({ value, label }) => ({
                          label,
                          accessibleLabel: label,
                          value,
                        }))}
                        value={account?.id ?? allSpendingAccounts}
                        onValueChange={chooseAccount}
                      />
                    ) : null}
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
                      pace={pace}
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
                    groups={merchantGroups}
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
                        from: start,
                        to: end,
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
                title={
                  placeBars.length ? (
                    <span className="spending-tabs" role="tablist" aria-label="Spending by">
                      {(
                        [
                          ['categories', 'Categories'],
                          ['places', 'Places'],
                        ] as const
                      ).map(([value, label]) => (
                        <button
                          type="button"
                          role="tab"
                          key={value}
                          aria-selected={shownBreakdown === value}
                          aria-keyshortcuts="Shift+B"
                          tabIndex={-1}
                          data-command="spending.breakdown.toggle"
                          onClick={() => setBreakdown(value)}
                        >
                          {label}
                        </button>
                      ))}
                    </span>
                  ) : (
                    'Categories'
                  )
                }
                action={
                  shownBreakdown === 'places' ? undefined : period ===
                    0 ? undefined : canCompareTypical ? (
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
              {shownBreakdown === 'places' ? (
                <RankedBars
                  items={placeBars}
                  label={`${periodLabel} spending by place`}
                  emptyMessage="No spending with a known place in this range."
                />
              ) : (
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
              )}
            </Card>
            {account && account.id !== card?.id ? (
              // Another card shows its own benefits only when a product is assigned to it.
              data.insights?.otherCardBenefits?.some((other) => other.accountId === account.id) ? (
                <CardBenefits accountId={account.id} />
              ) : null
            ) : card ? (
              <CardBenefits />
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
