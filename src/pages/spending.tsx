import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { DonutChart, SpendingLineChart, type DonutSegment } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { PlatinumBenefits } from '../components/platinum-benefits'
import { SpendingAccountEmptyState } from '../components/spending-account-empty-state'
import {
  AnimatedCurrency,
  Button,
  Card,
  Change,
  EmptyState,
  Metric,
  RangeSelector,
  SectionHeading,
} from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import { formatCompactCurrency, formatCurrency, valueTone, formatActivityName } from '../lib/format'
import { pageShortcutBlocked } from '../lib/keyboard'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildSpendingView,
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
  { label: 'M', accessibleLabel: '1 month', value: 1 },
  { label: 'Q', accessibleLabel: '3 months', value: 3 },
  { label: 'Y', accessibleLabel: '1 year', value: 12 },
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
  const [period, setPeriod] = useState<SpendingPeriod>(1)
  const [monthOffset, setMonthOffset] = useState(0)
  const referenceIso = data?.updatedAt ?? new Date().toISOString()
  const periodReference = spendingPeriodReference(transactions, referenceIso, monthOffset)
  const currentAccountBalance =
    monthOffset === 0 && account?.value != null ? Math.abs(account.value) : undefined
  const view = useMemo(
    () =>
      buildSpendingView(
        transactions,
        referenceIso,
        period,
        periodReference,
        'statement',
        currentAccountBalance,
      ),
    [currentAccountBalance, period, periodReference, referenceIso, transactions],
  )

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (pageShortcutBlocked(event)) return
      const monthDirection = spendingMonthDirectionForKey(event)
      if (monthDirection != null) {
        event.preventDefault()
        setMonthOffset((current) =>
          monthDirection === -1 ? current - 1 : Math.min(0, current + 1),
        )
        return
      }
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
      const nextPeriod = spendingPeriodForKey(event.key)
      if (nextPeriod == null) return
      event.preventDefault()
      setPeriod(nextPeriod)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const selectedPeriod =
    view.periodBasis === 'statement'
      ? statementEndFormatter.format(new Date(`${view.end}T00:00:00Z`))
      : monthYearFormatter.format(new Date(`${view.end}T00:00:00Z`))
  const periodLabel = spendingPeriodLabel(period, monthOffset, selectedPeriod, view.periodBasis)
  const headlineValue =
    view.periodBasis === 'statement' && period === 1 ? view.statementBalance : view.total
  const spendingChange = period ? view.total - view.previousTotal : null
  const segments: DonutSegment[] = view.categories.map(({ name, value }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
  }))
  const activities = buildActivities(
    { ...data, transactions: view.transactions, trades: [] },
    names,
    getExternalLogosEnabled(),
  )

  return (
    <div className="page spending-workspace-page">
      <WorkspaceHeader title="Spending" />
      {!account ? (
        <SpendingAccountEmptyState />
      ) : (
        <div className="spending-overview-grid">
          <Card className="workspace-brief-card spending-brief-card spending-hero-card">
            <div className="spending-primary-overview">
              <section
                className="spending-total-panel"
                aria-keyshortcuts="M Q Y A ArrowLeft ArrowRight Meta+ArrowLeft Meta+ArrowRight"
              >
                <header className="workspace-brief-heading">
                  <div>
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
                    <div className="chart-summary-row">
                      {spendingChange != null ? (
                        <div className="hero-change">
                          <span className={valueTone(-spendingChange)}>
                            {formatCurrency(spendingChange)} vs prior period
                          </span>
                          <Change value={view.percentChange} favorable="decrease" />
                        </div>
                      ) : null}
                      <div className="spending-period-indicators">
                        <Button
                          size="icon-compact"
                          variant="ghost"
                          className="icon-only-subtle"
                          aria-label="Previous spending period"
                          onClick={() => setMonthOffset((current) => current - 1)}
                        >
                          <ChevronLeft size={16} aria-hidden="true" />
                        </Button>
                        <RangeSelector
                          label={`Spending range. Active period: ${periodLabel}`}
                          options={spendingPeriodIndicators.map((indicator) => ({
                            ...indicator,
                            accessibleLabel:
                              view.periodBasis === 'statement' && indicator.value
                                ? `${indicator.value} ${indicator.value === 1 ? 'statement' : 'statements'}`
                                : indicator.accessibleLabel,
                          }))}
                          value={period}
                          onValueChange={setPeriod}
                        />
                        <Button
                          size="icon-compact"
                          variant="ghost"
                          className="icon-only-subtle"
                          aria-label="Next spending period"
                          disabled={monthOffset === 0}
                          onClick={() => setMonthOffset((current) => Math.min(0, current + 1))}
                        >
                          <ChevronRight size={16} aria-hidden="true" />
                        </Button>
                      </div>
                    </div>
                  </div>
                </header>
                <div className="spending-chart-region">
                  <SpendingLineChart
                    data={view.trend.map(({ date, current }) => ({ date, value: current }))}
                    activityMarkers={view.activityMarkers}
                    startingValue={view.startingBalance}
                    label={periodLabel}
                  />
                </div>
              </section>

              <div className="spending-category-panel">
                <div
                  className="workspace-metric-grid spending-category-metrics"
                  aria-label="Spending metrics"
                >
                  <Metric
                    label="Pending"
                    value={formatCurrency(view.pendingTotal)}
                    detail={
                      view.periodBasis === 'statement' && period === 1
                        ? 'Included in the statement balance'
                        : 'Included in the current total'
                    }
                  />
                  <Metric
                    label="Largest expense"
                    value={formatCurrency(view.biggest ? Math.abs(view.biggest.amount) : null)}
                    detail={
                      view.biggest ? formatActivityName(view.biggest.merchant) : 'No spending yet'
                    }
                  />
                </div>
                <div className="spending-category-chart">
                  <DonutChart
                    segments={segments}
                    label="Spending by category"
                    centerValue={formatCompactCurrency(view.total)}
                    centerLabel="spent"
                  />
                  {!segments.length ? (
                    <EmptyState>No category spending in this period.</EmptyState>
                  ) : null}
                </div>
              </div>
            </div>
          </Card>

          <PlatinumBenefits preview transactions={transactions} snapshotIso={data.updatedAt} />

          <Card className="spending-transactions-preview">
            <SectionHeading title="Recent activity" />
            <ActivityList
              activities={activities.slice(0, 10)}
              referenceIso={data.updatedAt}
              compact
            />
          </Card>
        </div>
      )}
    </div>
  )
}
