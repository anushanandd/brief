import { Link } from '@tanstack/react-router'

import { ActivityMark } from '../components/activity-list'
import { PageError, PageLoading } from '../components/data-state'
import { ExpectedActivity, recurringActivities } from '../components/expected-activity'
import { Repeat2 } from '../components/icons'
import { MoneyStrip } from '../components/report-links'
import { SubscriptionsCard } from '../components/subscriptions-card'
import { AnimatedCurrency, Card, EmptyState, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { countOf, formatActivityName, formatCurrency } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'

const changedOn = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  })

/**
 * Money's recurring view: what subscriptions cost a month, which changed price, and every bill,
 * paycheck, and subscription expected next. Rust finds the series and their prices; this page
 * only lays them out.
 */
export function RecurringPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <Recurring data={query.data} />
}

function Recurring({ data }: { data: FinanceSnapshot }) {
  const subscriptions = data.insights?.subscriptions
  const items = subscriptions?.items ?? []
  const repriced = items
    .filter((item) => item.priceChange)
    .toSorted((left, right) =>
      right.priceChange!.changedOn.localeCompare(left.priceChange!.changedOn),
    )
  const activities = recurringActivities(
    data,
    repriced.map(({ sourceTransactionId }) => sourceTransactionId),
  )

  return (
    <div className="page recurring-page">
      <WorkspaceHeader
        title="Money"
        breadcrumbs={[
          { label: 'Money', to: '/spending', search: {} },
          { label: 'Recurring', to: '/recurring' },
        ]}
      />
      <MoneyStrip data={data} current="/recurring" today={data.insights?.today} />
      <div className="recurring-grid">
        <div className="recurring-main">
          <Card className="recurring-summary-card">
            <header className="recurring-summary">
              <h2 className="balance-label">Subscriptions each month</h2>
              <AnimatedCurrency
                className="hero-number"
                value={subscriptions ? Math.abs(subscriptions.monthly) : null}
              />
              <p className="recurring-summary-line">
                {subscriptions
                  ? `${countOf(items.length, 'subscription')}${
                      repriced.length ? ` · ${countOf(repriced.length, 'price change')}` : ''
                    }`
                  : 'Unavailable for this snapshot.'}
              </p>
            </header>
            {repriced.length ? (
              <section className="recurring-changes" aria-labelledby="recurring-changes-title">
                <SectionHeading title={<span id="recurring-changes-title">Price changes</span>} />
                <ul aria-label="Price changes" data-keyboard-region>
                  {repriced.map((item) => {
                    const change = item.priceChange!
                    const name = formatActivityName(item.title)
                    const activity = activities.get(`spending:${item.sourceTransactionId}`)
                    const rose = Math.abs(change.latest) > Math.abs(change.previous)
                    return (
                      <li key={item.recurringId}>
                        <Link
                          className="recurring-change-row"
                          to="/activities"
                          search={{ open: change.transactionId }}
                          data-keyboard-row
                          data-keyboard-open
                        >
                          {activity ? (
                            <ActivityMark activity={activity} />
                          ) : (
                            <span className="transaction-mark" aria-hidden="true">
                              <Repeat2 size={15} />
                            </span>
                          )}
                          <span className="recurring-change-copy">
                            <strong>{name}</strong>
                            <small>
                              {rose ? 'Up' : 'Down'} from{' '}
                              <span className="sensitive-value">
                                {formatCurrency(Math.abs(change.previous))}
                              </span>{' '}
                              on {changedOn(change.changedOn)} · {item.frequency}
                            </small>
                          </span>
                          <strong className="sensitive-value" data-copy-amount>
                            {formatCurrency(Math.abs(change.latest))}
                          </strong>
                        </Link>
                      </li>
                    )
                  })}
                </ul>
              </section>
            ) : subscriptions && items.length ? (
              <EmptyState>No price changes.</EmptyState>
            ) : null}
          </Card>
          <SubscriptionsCard data={data} className="recurring-subscriptions-card" />
        </div>
        <ExpectedActivity data={data} className="recurring-expected-card" />
      </div>
    </div>
  )
}
