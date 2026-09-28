import { useEffect, useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { SpendingBarChart } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { PlatinumBenefits } from '../components/platinum-benefits'
import { SpendingAccountEmptyState } from '../components/spending-account-empty-state'
import {
  AnimatedCurrency,
  Card,
  ChartChange,
  ChartRangeSelect,
  Metric,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import { formatCurrency } from '../lib/format'
import { pageShortcutBlocked } from '../lib/keyboard'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildPlatinumCreditSummary,
  buildSpendingBars,
  buildSpendingView,
  isSpendingTransaction,
  resolveSpendingAccount,
  spendingCategoryColor,
  spendingMonthDirectionForKey,
  type SpendingPeriod,
  spendingPeriodLabel,
  spendingPeriodForKey,
  spendingPeriodReference,
} from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

const monthYearFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})
const statementEndFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  timeZone: 'UTC',
})

const spendingPeriodIndicators: Array<{
  label: string
  accessibleLabel: string
  value: SpendingPeriod
}> = [
  { label: 'S', accessibleLabel: 'Statement', value: 'statement' },
  { label: 'W', accessibleLabel: 'Week', value: 'week' },
  { label: 'M', accessibleLabel: 'Month', value: 1 },
  { label: 'Q', accessibleLabel: 'Quarter', value: 3 },
  { label: 'Y', accessibleLabel: 'Year', value: 12 },
  { label: 'A', accessibleLabel: 'All time', value: 0 },
]

function useActivityData() {
  const query = useFinance()
  const data = query.data
  const names = getAccountDisplayNames()
  const account = data ? resolveSpendingAccount(data.accounts, getSpendingAccountId()) : undefined
  const transactions = useMemo(
    () =>
      data && account ? data.transactions.filter(({ accountId }) => accountId === account.id) : [],
    [account, data],
  )
  return { query, data, names, account, transactions }
}

export function SpendingPage() {
  const { query, data, names, account, transactions } = useActivityData()
  const [period, setPeriod] = useState<SpendingPeriod>('statement')
  const [periodOffset, setPeriodOffset] = useState(0)
  const activityScrollRef = useViewportScroll()
  const referenceIso = data?.updatedAt ?? new Date().toISOString()
  const periodReference = spendingPeriodReference(transactions, referenceIso, periodOffset, period)
  const accountBalance = account?.value != null ? Math.abs(account.value) : undefined
  const currentAccountBalance = periodOffset === 0 ? accountBalance : undefined
  const view = useMemo(
    () =>
      buildSpendingView(transactions, referenceIso, period, periodReference, currentAccountBalance),
    [currentAccountBalance, period, periodReference, referenceIso, transactions],
  )
  const currentStatement = useMemo(
    () => buildSpendingView(transactions, referenceIso, 'statement', referenceIso, accountBalance),
    [accountBalance, referenceIso, transactions],
  )

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (pageShortcutBlocked(event)) return
      const monthDirection = spendingMonthDirectionForKey(event)
      if (monthDirection != null && period !== 0) {
        event.preventDefault()
        setPeriodOffset((current) =>
          monthDirection === -1 ? current - 1 : Math.min(0, current + 1),
        )
        return
      }
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
      const nextPeriod = spendingPeriodForKey(event.key)
      if (nextPeriod == null) return
      event.preventDefault()
      setPeriod(nextPeriod)
      setPeriodOffset(0)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [period])
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const selectedPeriod =
    view.periodBasis === 'statement' || period === 'week'
      ? statementEndFormatter.format(new Date(`${view.end}T00:00:00Z`))
      : monthYearFormatter.format(new Date(`${view.end}T00:00:00Z`))
  const periodLabel = spendingPeriodLabel(period, periodOffset, selectedPeriod, view.periodBasis)
  const headlineValue =
    view.periodBasis === 'statement' && period === 'statement' ? view.statementBalance : view.total
  const spendingChange = period ? view.total - view.previousTotal : null
  const bars = buildSpendingBars(view.transactions, referenceIso, view.start, view.end, period)
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
  const credits = buildPlatinumCreditSummary(transactions, referenceIso, view.start, view.end)
  const largestAllTime = transactions
    .filter(isSpendingTransaction)
    .toSorted((left, right) => Math.abs(right.amount) - Math.abs(left.amount))[0]
  const activities = buildActivities(
    { ...data, transactions: view.transactions, trades: [] },
    names,
    getExternalLogosEnabled(),
  )
  const activityById = new Map(activities.map((activity) => [activity.id, activity]))

  return (
    <div className="page spending-workspace-page">
      <WorkspaceHeader title="Spending" />
      {!account ? (
        <SpendingAccountEmptyState />
      ) : (
        <div className="spending-overview-grid">
          <Card className="workspace-brief-card spending-brief-card spending-hero-card">
            <section
              className="spending-total-panel"
              aria-keyshortcuts="S W M Q Y A ArrowLeft ArrowRight"
            >
              <header className="workspace-brief-heading">
                <div>
                  <div className="spending-balance-stack">
                    <h2
                      className="balance-label"
                      aria-description={
                        view.periodBasis === 'statement'
                          ? 'Period inferred from recurring posted autopay dates'
                          : undefined
                      }
                    >
                      {periodLabel}
                    </h2>
                    <AnimatedCurrency className="hero-number" value={headlineValue} />
                  </div>
                  <div className="chart-summary-row">
                    {spendingChange != null ? (
                      <ChartChange
                        amount={spendingChange}
                        percent={view.percentChange}
                        favorable="decrease"
                        ariaLabel="Change versus prior period"
                      />
                    ) : null}
                  </div>
                </div>
                <ChartRangeSelect
                  label={`Spending range. Active period: ${periodLabel}`}
                  options={spendingPeriodIndicators}
                  value={period}
                  onValueChange={(value) => {
                    setPeriod(value)
                    setPeriodOffset(0)
                  }}
                />
              </header>
              <div className="spending-chart-region">
                <SpendingBarChart
                  data={bars}
                  label={periodLabel}
                  categories={chartCategories}
                  activities={activityById}
                />
              </div>
              {chartCategories.length ? (
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

          <Card className="spending-category-card spending-category-panel">
            <SectionHeading title="Metrics" />
            <div
              className="account-summary-metrics spending-category-metrics"
              aria-label="Spending metrics"
            >
              <Metric
                label="Statement estimate"
                value={formatCurrency(currentStatement.estimatedClosingTotal)}
              />
              <Metric label="Daily average" value={formatCurrency(view.dailyAverage)} />
              <Metric label="Credits earned" value={formatCurrency(credits.earned)} />
              <Metric label="Est. credits missed" value={formatCurrency(credits.missed)} />
              <Metric
                label="Largest"
                value={formatCurrency(view.biggest ? Math.abs(view.biggest.amount) : null)}
              />
              <Metric
                label="All-time largest"
                value={formatCurrency(largestAllTime ? Math.abs(largestAllTime.amount) : null)}
              />
            </div>
          </Card>

          <ScrollCueCard
            className="spending-transactions-preview"
            scrollSelector=".spending-activity-scroll"
          >
            <SectionHeading title="Activity" />
            <div
              ref={activityScrollRef}
              className="spending-activity-scroll"
              role="region"
              aria-label="Spending activity"
              tabIndex={0}
              data-keyboard-region
            >
              <ActivityList
                activities={activities}
                referenceIso={data.updatedAt}
                emptyMessage="No activity in this range."
                compact
              />
            </div>
          </ScrollCueCard>

          <div className="spending-companion">
            <PlatinumBenefits preview transactions={transactions} snapshotIso={data.updatedAt} />
            <ExpectedActivity data={data} account={account.id} />
          </div>
        </div>
      )}
    </div>
  )
}
