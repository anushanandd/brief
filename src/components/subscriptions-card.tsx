import { Link } from '@tanstack/react-router'

import { daysBetween, localDateKey } from '../lib/dates'
import { formatActivityName, formatCurrency } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'
import { expectedActivityDateLabel, RecurringMark, recurringActivities } from './expected-activity'
import { Card, EmptyState, SectionHeading } from './ui'

export function SubscriptionsCard({
  data,
  className,
}: {
  data: FinanceSnapshot
  className?: string
}) {
  const today = data.insights?.today ?? localDateKey(Date.now() / 1000)
  const subscriptions = data.insights?.subscriptions
  const items = subscriptions?.items ?? []
  const activities = recurringActivities(
    data,
    items.map(({ sourceTransactionId }) => sourceTransactionId),
  )
  const stale = daysBetween(data.updatedAt.slice(0, 10), today) > 3

  return (
    <Card
      className={`account-group-card expected-activity-card subscriptions-card${className ? ` ${className}` : ''}`}
    >
      <SectionHeading
        title="Subscriptions"
        action={
          items.length && subscriptions ? (
            <span className="expected-horizon">
              <span className="sensitive-value">
                {formatCurrency(Math.abs(subscriptions.monthly))}
              </span>
              /mo
            </span>
          ) : undefined
        }
      />
      {items.length ? (
        <ul className="expected-activity-list" aria-label="Active subscriptions" tabIndex={0}>
          {items.map((item) => {
            const activity = activities.get(`spending:${item.sourceTransactionId}`)
            const name = formatActivityName(item.title)
            return (
              <li className="expected-activity-item" key={item.recurringId}>
                <RecurringMark activity={activity} />
                <div className="expected-activity-content">
                  <span className="financial-activity-copy">
                    <span className="expected-activity-title">
                      <Link
                        className="expected-activity-merchant"
                        to="/activities"
                        search={{ account: item.accountId, q: `description:${item.title}` }}
                        aria-label={`View ${name} charges`}
                        title={item.title}
                      >
                        {name}
                      </Link>
                    </span>
                    <small>
                      {item.frequency}
                      {item.priceChange ? (
                        <span className="subscription-price-change">
                          {Math.abs(item.priceChange.latest) > Math.abs(item.priceChange.previous)
                            ? ' · up from '
                            : ' · down from '}
                          <span className="sensitive-value">
                            {formatCurrency(Math.abs(item.priceChange.previous))}
                          </span>
                        </span>
                      ) : null}
                    </small>
                  </span>
                  <span className="financial-activity-meta">
                    <small>
                      {item.pending ? (
                        'Pending'
                      ) : (
                        <time dateTime={item.nextDate}>
                          {expectedActivityDateLabel(item.nextDate, today)}
                        </time>
                      )}
                    </small>
                    <strong
                      className="expected-activity-amount negative sensitive-value"
                      aria-label={`${name} charge: ${formatCurrency(item.amount)}`}
                    >
                      {formatCurrency(item.amount)}
                    </strong>
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      ) : (
        <EmptyState>
          {stale || !data.insights ? 'Refresh to see subscriptions.' : 'No subscriptions found.'}
        </EmptyState>
      )}
    </Card>
  )
}
