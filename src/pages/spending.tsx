import { Link } from '@tanstack/react-router'
import { ChevronRight, Settings } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { DonutChart, SpendingLineChart, type DonutSegment } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { Card, Metric, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities, movementsFromChange } from '../lib/activity'
import { formatCompactCurrency, formatCurrency } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildPlatinumBenefitHistory,
  buildPlatinumBenefitTracker,
  buildSpendingView,
  formatActivityDate,
  getPlatinumBenefitActivity,
  resolveSpendingAccount,
  spendingCategoryColor,
  type SpendingPeriod,
  spendingPeriodLabel,
  spendingPeriodForKey,
  spendingPeriodReference,
  transactionDateKey,
} from '../lib/spending'
import { getHiddenPlatinumBenefitIds, getSpendingAccountId } from '../lib/spending-preferences'

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
  title: string
  value: SpendingPeriod
}> = [
  { label: 'M', title: 'Month', value: 1 },
  { label: 'Q', title: 'Quarter', value: 3 },
  { label: 'Y', title: 'Year', value: 12 },
  { label: 'A', title: 'All time', value: 0 },
]

function EmptySpendingAccount() {
  return (
    <Card className="activity-empty-card">
      <Settings size={20} aria-hidden="true" />
      <strong>Choose a spending account</strong>
      <p>Brief uses one credit account for spending and benefit views.</p>
      <Link className="button-base button-secondary button-default" to="/settings">
        Open Settings
      </Link>
    </Card>
  )
}

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
      const target = event.target
      if (
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }
      const nextPeriod = spendingPeriodForKey(event.key)
      if (nextPeriod != null) {
        event.preventDefault()
        setPeriod(nextPeriod)
      } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault()
        setMonthOffset((current) =>
          event.key === 'ArrowLeft' ? current - 1 : Math.min(0, current + 1),
        )
      }
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
  const segments: DonutSegment[] = view.categories.map(({ name, value }) => ({
    name,
    value,
    color: spendingCategoryColor(name),
  }))
  const hiddenBenefits = getHiddenPlatinumBenefitIds()
  const benefits = buildPlatinumBenefitTracker(transactions, periodReference).filter(
    ({ id }) => !hiddenBenefits.includes(id),
  )
  const benefitYear = Number(view.end.slice(0, 4))
  const benefitHistory = buildPlatinumBenefitHistory(
    getPlatinumBenefitActivity(transactions, data.updatedAt),
    benefitYear,
  ).benefits.filter(({ creditCount }) => creditCount > 0)
  const activityPreviewLimit = Math.max(2, benefits.length + benefitHistory.length + 2)
  const activities = buildActivities(
    { ...data, transactions: view.transactions, trades: [] },
    names,
    getExternalLogosEnabled(),
  )

  return (
    <div className="page spending-workspace-page">
      <WorkspaceHeader title="Spending" />
      {!account ? (
        <EmptySpendingAccount />
      ) : (
        <div className="spending-overview-grid">
          <Card className="workspace-brief-card spending-brief-card spending-hero-card">
            <div className="spending-primary-overview">
              <section
                className="spending-total-panel"
                aria-keyshortcuts="M Q Y A ArrowLeft ArrowRight"
              >
                <header className="workspace-brief-heading">
                  <div>
                    <h2
                      className="balance-label"
                      title={
                        view.periodBasis === 'statement'
                          ? 'Period inferred from recurring posted autopay dates'
                          : undefined
                      }
                    >
                      {periodLabel}
                    </h2>
                    <strong className="hero-number">{formatCurrency(headlineValue)}</strong>
                  </div>
                </header>
                <div
                  className="spending-period-indicators"
                  role="group"
                  aria-label={`Active spending range: ${periodLabel}`}
                >
                  {spendingPeriodIndicators.map((indicator) => (
                    <span
                      key={indicator.label}
                      className={period === indicator.value ? 'active' : undefined}
                      aria-current={period === indicator.value ? 'true' : undefined}
                      title={
                        view.periodBasis === 'statement' && indicator.value
                          ? `${indicator.value} ${indicator.value === 1 ? 'statement' : 'statements'}`
                          : indicator.title
                      }
                    >
                      {indicator.label}
                    </span>
                  ))}
                </div>
                <SpendingLineChart
                  data={view.trend.map(({ date, current }) => ({ date, value: current }))}
                  activityMarkers={view.activityMarkers}
                  startingValue={view.startingBalance}
                  label={periodLabel}
                />
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
                    detail={view.biggest?.merchant ?? 'No spending yet'}
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
                    <small className="home-donut-empty">No category spending in this period.</small>
                  ) : null}
                </div>
              </div>
            </div>
          </Card>

          <Card className="spending-benefits-card">
            <SectionHeading title="Amex benefit tracker" />
            <div className="spending-benefit-list">
              {benefits.map((benefit) => {
                const progress = benefit.cap
                  ? Math.min(100, (benefit.creditedAmount / benefit.cap) * 100)
                  : 0
                return (
                  <div className="spending-benefit-row" key={benefit.id}>
                    <span>
                      <strong>{benefit.name}</strong>
                      <small>
                        {benefit.remainingAmount === 0
                          ? 'Current benefit matched'
                          : `${formatCurrency(benefit.remainingAmount)} available`}
                      </small>
                    </span>
                    <strong>
                      {formatCurrency(benefit.creditedAmount)} / {formatCurrency(benefit.cap)}
                    </strong>
                    <div aria-hidden="true">
                      <span style={{ width: `${Math.max(progress, progress ? 2 : 0)}%` }} />
                    </div>
                  </div>
                )
              })}
              {!benefits.length ? (
                <p className="overview-recent-empty">No benefits selected in Settings.</p>
              ) : null}
            </div>
          </Card>

          <Card className="spending-benefit-history-card">
            <SectionHeading title="By benefit" detail={`Matched credits in ${benefitYear}`} />
            <div className="spending-benefit-credit-list">
              {benefitHistory.map((benefit) => (
                <div className="spending-benefit-credit-row" key={benefit.id}>
                  <span>
                    <strong>{benefit.name}</strong>
                    <small>
                      {benefit.creditCount} matched{' '}
                      {benefit.creditCount === 1 ? 'credit' : 'credits'}
                    </small>
                  </span>
                  <strong className="positive">{formatCurrency(benefit.creditedAmount)}</strong>
                </div>
              ))}
              {!benefitHistory.length ? (
                <p className="overview-recent-empty">No matched credits in {benefitYear}.</p>
              ) : null}
            </div>
          </Card>

          <Card className="spending-transactions-preview">
            <SectionHeading
              title={
                <Link to="/spending/transactions" className="section-heading-link">
                  Latest activity <ChevronRight size={14} aria-hidden="true" />
                </Link>
              }
            />
            <ActivityList
              activities={activities.slice(0, activityPreviewLimit)}
              referenceIso={data.updatedAt}
              compact
            />
          </Card>
        </div>
      )}
    </div>
  )
}

export function TradesActivityPage() {
  const { query, data, names } = useActivityData()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const year = data.updatedAt.slice(0, 4)
  const trades = data.trades
    .filter(({ date }) => transactionDateKey(date, data.updatedAt).startsWith(year))
    .toSorted((left, right) => right.date.localeCompare(left.date))
  const buys = trades.filter(({ type }) => type.toUpperCase().includes('BUY')).length
  const sales = trades.filter(({ type }) => type.toUpperCase().includes('SELL')).length
  const accounts = new Set(trades.map(({ accountId }) => accountId).filter(Boolean)).size
  const activities = buildActivities(
    { ...data, transactions: [], trades },
    names,
    getExternalLogosEnabled(),
  )

  return (
    <div className="page activities-page">
      <WorkspaceHeader title="Trades" parent={{ label: 'Spending', to: '/spending' }} />
      <Card className="workspace-brief-card activity-count-brief">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Trades in {year}</span>
            <strong className="hero-number">{trades.length}</strong>
          </div>
          <p>Execution history supplied by connected investment providers</p>
        </header>
        <div className="workspace-metric-grid workspace-metric-grid-three">
          <Metric label="Buys" value={buys} />
          <Metric label="Sales" value={sales} />
          <Metric label="Accounts" value={accounts} />
        </div>
      </Card>
      <Card className="transactions-card activity-ledger-card">
        <SectionHeading
          title="Trade ledger"
          detail="Amounts are cash effects; realized estimates appear only when lots reconcile."
        />
        <ActivityList
          activities={activities}
          referenceIso={data.updatedAt}
          emptyMessage="No imported trades this year."
        />
      </Card>
    </div>
  )
}

export function ChangesActivityPage() {
  const { query, data, names } = useActivityData()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const movements = (
    data.accountMovements.length ? data.accountMovements : movementsFromChange(data.lastChange)
  ).toReversed()
  const increases = movements.filter(({ change }) => change > 0).length
  const decreases = movements.filter(({ change }) => change < 0).length
  const latest = data.lastChange

  return (
    <div className="page activities-page">
      <WorkspaceHeader title="Changes" parent={{ label: 'Spending', to: '/spending' }} />
      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Latest net-worth change</span>
            <strong
              className={`hero-number ${latest?.netWorthChange ? (latest.netWorthChange > 0 ? 'positive' : 'negative') : 'muted'}`}
            >
              {formatCurrency(latest?.netWorthChange)}
            </strong>
          </div>
          <p>
            {latest
              ? `Observed ${formatActivityDate(latest.observedAt, data.updatedAt)}`
              : 'No committed comparison is available yet'}
          </p>
        </header>
        <div className="workspace-metric-grid workspace-metric-grid-three">
          <Metric label="Recorded observations" value={movements.length} />
          <Metric label="Increases" value={increases} />
          <Metric label="Decreases" value={decreases} />
        </div>
      </Card>
      <Card className="key-changes-card activity-ledger-card">
        <SectionHeading
          title="Balance change ledger"
          detail="Changes are observations, not transaction or market-return claims."
        />
        <div className="key-change-list">
          {movements.slice(0, 100).map((change) => (
            <article className="key-change-row" key={change.id}>
              <span className="key-change-copy">
                <strong>{accountDisplayName(change.accountId, change.name, names)}</strong>
                <small>Observed {formatActivityDate(change.observedAt, data.updatedAt)}</small>
              </span>
              <strong
                className={
                  change.change > 0 ? 'positive' : change.change < 0 ? 'negative' : 'muted'
                }
              >
                {formatCurrency(change.change)}
              </strong>
            </article>
          ))}
          {!movements.length ? (
            <p className="key-change-empty">No recorded balance changes.</p>
          ) : null}
        </div>
      </Card>
    </div>
  )
}
