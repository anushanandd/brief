import { Link } from '@tanstack/react-router'

import type { ActivityItem } from '../lib/activity'
import { formatActivityName, formatCurrency, formatShortDay } from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'
import { ActivityMark } from './activity-list'
import { recurringActivities } from './expected-activity'
import { Repeat2 } from './icons'

/** Expected activity for the next two weeks, from native insights. */
export function ComingUp({ data }: { data: FinanceSnapshot }) {
  // Native insights choose the window, including anything overdue and still expected.
  const coming = new Set(data.insights?.brief.coming)
  const events = (data.insights?.expected ?? []).filter(({ id }) => coming.has(id)).slice(0, 5)
  const activities = recurringActivities(
    data,
    events.map(({ sourceTransactionId }) => sourceTransactionId),
  )
  return (
    <section className="coming-up" aria-labelledby="coming-up-title">
      <header className="coming-up-heading">
        <h3 id="coming-up-title">Coming up</h3>
        <Link
          className="coming-up-link"
          to="/activities"
          data-command="navigation.open./activities"
        >
          See all
        </Link>
      </header>
      {events.length ? (
        <ul className="coming-up-list">
          {events.map((event) => (
            <li
              key={event.id}
              className="coming-up-row"
              title={`${event.frequency}${event.state === 'pending' ? ' · pending' : ''}`}
            >
              <time dateTime={event.date}>{formatShortDay(event.date)}</time>
              <EventMark activity={activities.get(`spending:${event.sourceTransactionId}`)} />
              <span className="coming-up-copy">
                <strong>{formatActivityName(event.title)}</strong>
              </span>
              <span className={`sensitive-value ${event.amount < 0 ? 'negative' : 'positive'}`}>
                {event.amountHigh - event.amountLow > Math.max(5, Math.abs(event.amount) * 0.1)
                  ? '≈ '
                  : ''}
                {event.amount > 0 ? '+' : ''}
                {formatCurrency(event.amount)}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="coming-up-quiet">
          {data.insights
            ? `Nothing expected in the next ${data.insights.brief.comingDays} days.`
            : 'Unavailable for this snapshot.'}
        </p>
      )}
    </section>
  )
}

function EventMark({ activity }: { activity?: ActivityItem }) {
  return activity ? (
    <ActivityMark activity={activity} />
  ) : (
    <span className="transaction-mark" aria-hidden="true">
      <Repeat2 size={15} />
    </span>
  )
}
