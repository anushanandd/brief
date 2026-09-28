import { Link } from '@tanstack/react-router'
import { useEffect, useState, type Ref } from 'react'

import { getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import { analyticsAccountMatches, localDateKey } from '../lib/analytics'
import { formatActivityName, formatCurrency } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import { expectedMoneyEvents, recurringMoneyKey } from '../lib/money'
import type { FinanceSnapshot } from '../lib/schema'
import { ActivityMark } from './activity-list'
import { Info, Repeat2 } from './icons'
import { Card, EmptyState, SectionHeading } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const dateLabel = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
  })

export function expectedActivityDateLabel(date: string, today: string) {
  const days = Math.round((Date.parse(date) - Date.parse(today)) / 86_400_000)
  if (days === 0) return 'today'
  if (days === 1) return 'tomorrow'
  if (days > 1 && days <= 5) return `in ${days} days`
  return new Date(`${date}T12:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  })
}

export const expectedActivityTone = (amount: number): 'positive' | 'negative' =>
  amount >= 0 ? 'positive' : 'negative'

export function ExpectedActivity({
  data,
  account,
  className,
  scrollRef,
}: {
  data: FinanceSnapshot
  account?: string
  className?: string
  scrollRef?: Ref<HTMLUListElement>
}) {
  const [today, setToday] = useState(() => localDateKey(Date.now() / 1000))
  useEffect(() => {
    const update = () => setToday(localDateKey(Date.now() / 1000))
    const timer = globalThis.setInterval(update, 60_000)
    globalThis.addEventListener('focus', update)
    return () => {
      globalThis.clearInterval(timer)
      globalThis.removeEventListener('focus', update)
    }
  }, [])
  const scopedData = {
    ...data,
    transactions: data.transactions.filter((entry) =>
      analyticsAccountMatches(account, entry.accountId),
    ),
  }
  const expected = expectedMoneyEvents(scopedData, today)
  const activities = new Map(
    buildActivities(
      { updatedAt: data.updatedAt, transactions: scopedData.transactions, trades: [] },
      getAccountDisplayNames(),
      getExternalLogosEnabled(),
    ).map((activity) => [activity.id, activity]),
  )
  const stale = Date.parse(today) - Date.parse(data.updatedAt.slice(0, 10)) > 3 * 86_400_000

  return (
    <Card
      className={`account-group-card expected-activity-card${className ? ` ${className}` : ''}`}
    >
      <SectionHeading
        title="Expected activity"
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
              scopedData.transactions.find((entry) => entry.id === event.matchedTransactionId) ??
              scopedData.transactions.find(
                (entry) =>
                  recurringMoneyKey(entry) === event.recurringId &&
                  (entry.postedOn ?? entry.date).slice(0, 10) === event.lastSeen,
              ) ??
              scopedData.transactions.find(
                (entry) => recurringMoneyKey(entry) === event.recurringId,
              )
            const activity = transaction ? activities.get(`spending:${transaction.id}`) : undefined
            const name = formatActivityName(event.title)
            return (
              <li className="expected-activity-item" key={event.id}>
                {activity ? (
                  <ActivityMark activity={activity} />
                ) : (
                  <span className="transaction-mark activity-mark-transaction" aria-hidden="true">
                    <Repeat2 size={15} />
                  </span>
                )}
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
                            {dateLabel(event.lastSeen)}.
                          </p>
                          {event.state === 'awaiting-post' ? (
                            <p>Expected {dateLabel(event.date)}; not yet posted.</p>
                          ) : event.state === 'pending' ? (
                            <p>Pending transaction observed; awaiting posting.</p>
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
                    <small>
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
          {stale
            ? 'Refresh saved activity to check what is expected next.'
            : 'No currently supported recurring activity is expected soon.'}
        </EmptyState>
      )}
    </Card>
  )
}
