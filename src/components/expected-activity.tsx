import { Link } from '@tanstack/react-router'
import type { Ref } from 'react'

import { buildActivities, type ActivityItem } from '../lib/activity'
import { analyticsAccountMatches } from '../lib/analytics'
import { daysBetween, localDateKey } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import { formatActivityName, formatCurrency, formatDay, formatShortDay } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { ActivityMark } from './activity-list'
import { Info, Repeat2 } from './icons'
import { Card, EmptyState, SectionHeading } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

/** A recurring series' mark: its latest activity's, or the repeat icon before it has one. */
export function RecurringMark({ activity }: { activity: ActivityItem | undefined }) {
  return activity ? (
    <ActivityMark activity={activity} />
  ) : (
    <span className="transaction-mark activity-mark-transaction" aria-hidden="true">
      <Repeat2 size={15} />
    </span>
  )
}

export function expectedActivityDateLabel(date: string, today: string) {
  const days = daysBetween(today, date)
  if (days === 0) return 'today'
  if (days === 1) return 'tomorrow'
  if (days > 1 && days <= 5) return `in ${days} days`
  return formatShortDay(date)
}

/** Activities for the transactions behind recurring series, keyed by activity id, for marks. */
export function recurringActivities(
  data: FinanceSnapshot,
  transactionIds: readonly (string | undefined)[],
) {
  const transactionsById = new Map(data.transactions.map((entry) => [entry.id, entry]))
  return new Map(
    buildActivities(
      {
        updatedAt: data.updatedAt,
        transactions: transactionIds.flatMap((id) => {
          const transaction = id ? transactionsById.get(id) : undefined
          return transaction ? [transaction] : []
        }),
        trades: [],
      },
      financeSettings(data).accountNames,
      getExternalLogosEnabled(),
    ).map((activity) => [activity.id, activity]),
  )
}

export const expectedActivityTone = (amount: number): 'positive' | 'negative' =>
  amount >= 0 ? 'positive' : 'negative'

export function ExpectedActivity({
  data,
  account,
  recurringIds,
  className,
  scrollRef,
}: {
  data: FinanceSnapshot
  account?: string
  /** Limits the forecast to series that include these native recurring identities. */
  recurringIds?: ReadonlySet<string>
  className?: string
  scrollRef?: Ref<HTMLUListElement>
}) {
  const today = data.insights?.today ?? localDateKey(Date.now() / 1000)
  const expected = (data.insights?.expected ?? [])
    .filter(
      (event) =>
        analyticsAccountMatches(account, event.accountId) &&
        (!recurringIds || recurringIds.has(event.recurringId)),
    )
    .slice(0, 16)
  const transactionsById = new Map(data.transactions.map((entry) => [entry.id, entry]))
  const activities = recurringActivities(
    data,
    expected.flatMap((event) => [event.matchedTransactionId, event.sourceTransactionId]),
  )
  const stale = daysBetween(data.updatedAt.slice(0, 10), today) > 3

  return (
    <Card
      className={`account-group-card expected-activity-card${className ? ` ${className}` : ''}`}
    >
      <SectionHeading
        title="Coming up"
        action={<span className="expected-horizon">Next 30 days</span>}
      />
      {expected.length ? (
        <ul
          ref={scrollRef}
          className="expected-activity-list"
          aria-label="Estimated recurring activity"
          tabIndex={0}
        >
          {expected.map((event) => {
            const variable =
              event.amountHigh - event.amountLow > Math.max(5, Math.abs(event.amount) * 0.1)
            const transaction =
              transactionsById.get(event.matchedTransactionId ?? '') ??
              transactionsById.get(event.sourceTransactionId)
            const activity = transaction ? activities.get(`spending:${transaction.id}`) : undefined
            const name = formatActivityName(event.title)
            return (
              <li className="expected-activity-item" key={event.id}>
                <RecurringMark activity={activity} />
                <div className="expected-activity-content">
                  <span className="financial-activity-copy">
                    <span className="expected-activity-title">
                      <Link
                        className="expected-activity-merchant"
                        to="/activities"
                        search={{
                          account: event.accountId,
                          q: `description:${event.title}`,
                        }}
                        title={event.title}
                        aria-label={`View previous ${name} activity${activity?.account ? ` in ${activity.account}` : ''}`}
                      >
                        {name}
                      </Link>
                      <Tooltip>
                        <TooltipTrigger
                          render={
                            <span
                              className="expected-activity-info"
                              role="img"
                              tabIndex={0}
                              aria-label={`Estimate details for ${name}`}
                            />
                          }
                        >
                          <Info size={14} aria-hidden="true" />
                        </TooltipTrigger>
                        <TooltipContent
                          className="expected-activity-tooltip"
                          sideOffset={6}
                          align="start"
                        >
                          <p>
                            Based on {event.observations} posted transactions. Last seen{' '}
                            {formatDay(event.lastSeen)}.
                          </p>
                          {event.state === 'awaiting-post' ? (
                            <p>Expected {formatDay(event.date)}; not yet posted.</p>
                          ) : event.state === 'pending' ? (
                            <p>Pending, not yet posted.</p>
                          ) : null}
                          {variable ? (
                            <p>
                              Observed{' '}
                              {formatCurrency(
                                event.amount < 0 ? -event.amountHigh : event.amountLow,
                              )}{' '}
                              to{' '}
                              {formatCurrency(
                                event.amount < 0 ? -event.amountLow : event.amountHigh,
                              )}
                              .
                            </p>
                          ) : null}
                        </TooltipContent>
                      </Tooltip>
                    </span>
                    <small
                      title={`${activity?.account ?? transaction?.account ?? 'Account'} · ${event.frequency}`}
                    >
                      {activity?.account ?? transaction?.account ?? 'Account'} · {event.frequency}
                      {event.kind === 'subscription'
                        ? ' subscription'
                        : event.kind === 'credit'
                          ? ' credit'
                          : ''}
                    </small>
                  </span>
                  <span className="financial-activity-meta">
                    <small>
                      {event.state === 'awaiting-post' ? (
                        'Awaiting posting'
                      ) : event.state === 'pending' ? (
                        'Pending'
                      ) : (
                        <time dateTime={event.date}>
                          {expectedActivityDateLabel(event.date, today)}
                        </time>
                      )}
                    </small>
                    <strong
                      className={`expected-activity-amount ${expectedActivityTone(event.amount)}`}
                      aria-label={`${event.state === 'pending' ? 'Pending' : 'Estimated'} amount: ${formatCurrency(event.amount)}`}
                    >
                      {variable ? '≈ ' : ''}
                      {formatCurrency(event.amount)}
                    </strong>
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      ) : (
        <EmptyState>
          {stale ? 'Refresh to see what’s coming up.' : 'Nothing expected soon.'}
        </EmptyState>
      )}
    </Card>
  )
}
