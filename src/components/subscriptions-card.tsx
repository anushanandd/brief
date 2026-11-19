import { useState } from 'react'

import { snapshotToday } from '../lib/dates'
import { formatActivityName, formatCurrency, formatShortDay } from '../lib/format'
import { useExternalLogosEnabled } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { expectedActivityDateLabel, RecurringMark, recurringActivities } from './expected-activity'
import { TransactionDialog } from './transaction-dialog'
import { Card, EmptyState, SectionHeading } from './ui'

/** Every active subscription: what it costs, how often, when it charges next, and price changes. */
export function SubscriptionsCard({
  data,
  className,
}: {
  data: FinanceSnapshot
  className?: string
}) {
  const today = snapshotToday(data)
  const subscriptions = data.insights?.subscriptions
  const items = subscriptions?.items ?? []
  const activities = recurringActivities(
    data,
    items.map(({ sourceTransactionId }) => sourceTransactionId),
    useExternalLogosEnabled(),
  )
  // Without insights, as with data too old to forecast from, the list is unknown.
  const stale = data.insights?.forecastStale ?? true
  const [opened, setOpened] = useState<string>()

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
        <ul
          className="expected-activity-list"
          aria-label="Active subscriptions"
          tabIndex={0}
          data-keyboard-region
        >
          {items.map((item) => {
            const activity = activities.get(`spending:${item.sourceTransactionId}`)
            const name = formatActivityName(item.title)
            return (
              // The row opens its latest charge, whose details list the rest of them.
              <li
                className="list-row expected-activity-item"
                key={item.recurringId}
                data-keyboard-row={activity ? true : undefined}
                data-keyboard-open={activity ? true : undefined}
                tabIndex={activity ? -1 : undefined}
                onClick={activity ? () => setOpened(activity.id) : undefined}
              >
                <RecurringMark activity={activity} />
                <div className="expected-activity-content">
                  <span className="financial-activity-copy">
                    <span className="expected-activity-title">
                      <span className="expected-activity-merchant">{name}</span>
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
                          {` on ${formatShortDay(item.priceChange.changedOn)}`}
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
          {stale ? 'Refresh to see subscriptions.' : 'No subscriptions found.'}
        </EmptyState>
      )}
      <TransactionDialog
        activity={opened ? activities.get(opened) : undefined}
        data={data}
        onClose={() => setOpened(undefined)}
      />
    </Card>
  )
}
