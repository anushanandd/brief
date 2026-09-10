import { Link } from '@tanstack/react-router'
import {
  ArrowLeftRight,
  BadgeCheck,
  ChevronRight,
  ReceiptText,
  RefreshCw,
  Settings,
  WalletCards,
} from 'lucide-react'
import { useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { ActivityWorkspaceHeader } from '../components/activity-workspace-header'
import { PageError, PageLoading } from '../components/data-state'
import { TransactionList } from '../components/transaction-list'
import { Card, Change, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities, movementsFromChange } from '../lib/activity'
import { formatCurrency } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import {
  buildSpendingView,
  formatActivityDate,
  identifySubscriptions,
  platinumBenefitOptions,
  resolveSpendingAccount,
  spendingCategoryColor,
  type SpendingPeriod,
  transactionDateKey,
} from '../lib/spending'
import { getHiddenPlatinumBenefitIds, getSpendingAccountId } from '../lib/spending-preferences'

const periodOptions: Array<{ value: SpendingPeriod; label: string }> = [
  { value: 1, label: '1 month' },
  { value: 3, label: '3 months' },
  { value: 12, label: '12 months' },
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

function RankedSpendingList({
  items,
  empty,
  showCategoryColors = false,
}: {
  items: Array<{ name: string; value: number; change: number; percent: number }>
  empty: string
  showCategoryColors?: boolean
}) {
  return (
    <div className="spending-rank-list">
      {items.slice(0, 8).map((item) => (
        <div className="spending-rank-row" key={item.name}>
          <div className="spending-rank-heading">
            <strong>{item.name}</strong>
            <span>{formatCurrency(item.value)}</span>
          </div>
          <div className="spending-rank-track" aria-hidden="true">
            <span
              style={{
                width: `${Math.max(2, item.percent)}%`,
                background: showCategoryColors ? spendingCategoryColor(item.name) : undefined,
              }}
            />
          </div>
          <small>
            {item.change === 0
              ? 'No change from comparable period'
              : `${formatCurrency(Math.abs(item.change))} ${item.change > 0 ? 'more' : 'less'} than comparable period`}
          </small>
        </div>
      ))}
      {!items.length ? <p className="overview-recent-empty">{empty}</p> : null}
    </div>
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

export function ActivitiesPage() {
  const { query, data, names, transactions } = useActivityData()
  const activities = useMemo(
    () => (data ? buildActivities(data, names, getExternalLogosEnabled()) : []),
    [data, names],
  )
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const movements = (
    data.accountMovements.length ? data.accountMovements : movementsFromChange(data.lastChange)
  ).toReversed()
  const spending = buildSpendingView(transactions, data.updatedAt, 1)
  const subscriptions = identifySubscriptions(transactions, data.updatedAt)
  const hiddenBenefits = getHiddenPlatinumBenefitIds()
  const trackedBenefits = platinumBenefitOptions.filter(
    ({ id }) => !hiddenBenefits.includes(id),
  ).length

  return (
    <div className="page activities-page">
      <ActivityWorkspaceHeader />

      <nav className="workspace-destination-grid" aria-label="Activity details">
        <Link to="/activities/spending" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-payment" aria-hidden="true">
            <WalletCards size={17} />
          </span>
          <span>
            <strong>Spending</strong>
            <small>{formatCurrency(spending.total)} this month</small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
        <Link to="/activities/subscriptions" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-payment" aria-hidden="true">
            <RefreshCw size={17} />
          </span>
          <span>
            <strong>Subscriptions</strong>
            <small>{subscriptions.length} possible recurring charges</small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
        <Link to="/activities/trades" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-transfer" aria-hidden="true">
            <ArrowLeftRight size={17} />
          </span>
          <span>
            <strong>Trades</strong>
            <small>{data.trades.length} imported executions</small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
        <Link to="/activities/changes" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-income" aria-hidden="true">
            <ReceiptText size={17} />
          </span>
          <span>
            <strong>Changes</strong>
            <small>{movements.length} balance observations</small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
        <Link to="/activities/benefits" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-refund" aria-hidden="true">
            <BadgeCheck size={17} />
          </span>
          <span>
            <strong>Benefits</strong>
            <small>{trackedBenefits} Platinum credits tracked</small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
      </nav>

      <Card className="transactions-card activity-ledger-card">
        <SectionHeading
          title="Latest activity"
          detail={`${Math.min(activities.length, 50)} most recent events across connected accounts.`}
        />
        <ActivityList activities={activities.slice(0, 50)} referenceIso={data.updatedAt} />
      </Card>
    </div>
  )
}

export function SpendingActivityPage() {
  const { query, data, names, account, transactions } = useActivityData()
  const [period, setPeriod] = useState<SpendingPeriod>(1)
  const view = useMemo(
    () => buildSpendingView(transactions, data?.updatedAt ?? new Date().toISOString(), period),
    [data?.updatedAt, period, transactions],
  )
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const accountName = account ? accountDisplayName(account.id, account.name, names) : undefined
  const periodLabel = period === 1 ? 'This month' : `Last ${period} months`

  return (
    <div className="page activities-page spending-workspace-page">
      <ActivityWorkspaceHeader title="Spending" />
      {!account ? (
        <EmptySpendingAccount />
      ) : (
        <>
          <Card className="workspace-brief-card spending-brief-card">
            <header className="workspace-brief-heading">
              <div>
                <span className="balance-label">
                  {periodLabel} · {accountName}
                </span>
                <strong className="hero-number">{formatCurrency(view.total)}</strong>
                <div className="hero-change">
                  {view.percentChange == null ? (
                    <span className="muted">No comparable prior spending</span>
                  ) : (
                    <Change
                      value={view.percentChange}
                      label="vs comparable period"
                      favorable="decrease"
                    />
                  )}
                </div>
              </div>
              <div className="period-control" role="radiogroup" aria-label="Spending period">
                {periodOptions.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    role="radio"
                    aria-checked={period === option.value}
                    className={period === option.value ? 'active' : undefined}
                    onClick={() => setPeriod(option.value)}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </header>
            <div className="workspace-metric-grid workspace-metric-grid-three">
              <div className="workspace-metric">
                <span>Daily pace</span>
                <strong>{formatCurrency(view.dailyAverage)}</strong>
                <small>Across the elapsed period</small>
              </div>
              <div className="workspace-metric">
                <span>Pending</span>
                <strong>{formatCurrency(view.pendingTotal)}</strong>
                <small>Included in the current total</small>
              </div>
              <div className="workspace-metric">
                <span>Largest purchase</span>
                <strong>
                  {formatCurrency(view.biggest ? Math.abs(view.biggest.amount) : null)}
                </strong>
                <small>{view.biggest?.merchant ?? 'No spending yet'}</small>
              </div>
            </div>
          </Card>

          <div className="spending-analysis-grid">
            <Card className="spending-analysis-card">
              <SectionHeading title="Categories" detail="What is shaping this period." />
              <RankedSpendingList
                items={view.categories}
                empty="No category spending."
                showCategoryColors
              />
            </Card>
            <Card className="spending-analysis-card">
              <SectionHeading title="Merchants" detail="Where spending is concentrated." />
              <RankedSpendingList items={view.merchants} empty="No merchant spending." />
            </Card>
          </div>

          <Card className="transactions-card activity-ledger-card">
            <SectionHeading
              title="Transactions in period"
              detail={`${view.transactions.length} imported transactions from ${view.start} through ${view.end}.`}
              action={
                <Link to="/activities/transactions" className="section-heading-link">
                  Search all <ChevronRight size={14} aria-hidden="true" />
                </Link>
              }
            />
            <TransactionList
              transactions={view.transactions.slice(0, 14)}
              referenceIso={data.updatedAt}
              detailed
            />
          </Card>
        </>
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
      <ActivityWorkspaceHeader title="Trades" />
      <Card className="workspace-brief-card activity-count-brief">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Trades in {year}</span>
            <strong className="hero-number">{trades.length}</strong>
          </div>
          <p>Execution history supplied by connected investment providers</p>
        </header>
        <div className="workspace-metric-grid workspace-metric-grid-three">
          <div className="workspace-metric">
            <span>Buys</span>
            <strong>{buys}</strong>
          </div>
          <div className="workspace-metric">
            <span>Sales</span>
            <strong>{sales}</strong>
          </div>
          <div className="workspace-metric">
            <span>Accounts</span>
            <strong>{accounts}</strong>
          </div>
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
      <ActivityWorkspaceHeader title="Changes" />
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
          <div className="workspace-metric">
            <span>Recorded observations</span>
            <strong>{movements.length}</strong>
          </div>
          <div className="workspace-metric">
            <span>Increases</span>
            <strong>{increases}</strong>
          </div>
          <div className="workspace-metric">
            <span>Decreases</span>
            <strong>{decreases}</strong>
          </div>
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
