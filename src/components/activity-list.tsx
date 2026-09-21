import {
  ArrowLeftRight,
  ArrowUpRight,
  BadgeDollarSign,
  DollarSign,
  ReceiptText,
  WalletCards,
} from 'lucide-react'
import { memo, useEffect, useRef, useState } from 'react'

import { groupActivitiesByDate, type ActivityItem } from '../lib/activity'
import { safeExternalUrl } from '../lib/api'
import { formatCurrency, formatActivityName } from '../lib/format'
import { formatActivityDate } from '../lib/spending'
import { BrandMark } from './brand-mark'
import { ExternalLink } from './external-link'
import { EmptyState } from './ui'

const activityMeta = {
  spending: WalletCards,
  transaction: ReceiptText,
  income: DollarSign,
  credit: BadgeDollarSign,
  transfer: ArrowLeftRight,
  trade: ArrowLeftRight,
} as const

export const ActivityList = memo(function ActivityList({
  activities,
  referenceIso,
  emptyMessage = 'No recent activity.',
  compact = false,
  showDescriptions = false,
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  compact?: boolean
  showDescriptions?: boolean
}) {
  return (
    <div className="financial-activity-list">
      {activities.map((activity) => {
        const Icon = activityMeta[activity.kind]
        const date = formatActivityDate(activity.date, referenceIso)
        const transferTone =
          activity.kind === 'transfer'
            ? activity.amount > 0
              ? ' activity-mark-transfer-in'
              : activity.amount < 0
                ? ' activity-mark-transfer-out'
                : ''
            : ''
        const website = activity.website ? safeExternalUrl(activity.website) : undefined
        const content = (
          <>
            {activity.kind === 'spending' || activity.kind === 'transaction' ? (
              <BrandMark
                className={`transaction-mark activity-mark-${activity.kind}`}
                fallback={<Icon size={15} />}
                label={`${activity.title} transaction logo`}
                src={activity.logoUrl}
              />
            ) : (
              <span
                className={`transaction-mark activity-mark-${activity.kind}${transferTone}`}
                aria-hidden="true"
              >
                <Icon size={15} />
              </span>
            )}
            <span className="financial-activity-copy">
              <span className="financial-activity-title">
                <strong>{formatActivityName(activity.title)}</strong>
                {website ? (
                  <span className="financial-activity-external">
                    <ArrowUpRight aria-hidden="true" size={13} />
                  </span>
                ) : null}
              </span>
              <small>{activity.detail}</small>
              {showDescriptions && activity.description ? (
                <small className="transaction-description">{activity.description}</small>
              ) : null}
            </span>
            <span className="financial-activity-meta">
              <small>
                {compact && activity.date.slice(0, 4) === referenceIso.slice(0, 4)
                  ? date.replace(/, \d{4}$/, '')
                  : date}
                {activity.pending ? ' · Pending' : ''}
              </small>
              <strong
                className={
                  activity.amount > 0 ? 'positive' : activity.amount === 0 ? 'muted' : undefined
                }
              >
                {formatCurrency(activity.amount)}
              </strong>
            </span>
          </>
        )
        return website ? (
          <ExternalLink
            className="financial-activity-row financial-activity-link"
            data-keyboard-row
            data-keyboard-open
            href={website}
            target="_blank"
            rel="noopener noreferrer"
            key={activity.id}
          >
            {content}
          </ExternalLink>
        ) : (
          <div className="financial-activity-row" key={activity.id} data-keyboard-row tabIndex={-1}>
            {content}
          </div>
        )
      })}
      {!activities.length ? <EmptyState>{emptyMessage}</EmptyState> : null}
    </div>
  )
})

export function GroupedActivityList(props: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
}) {
  // Reset the batch when filtering changes the actual rows, not their array identity.
  return (
    <ProgressiveActivityList key={props.activities.map(({ id }) => id).join('\0')} {...props} />
  )
}

function ProgressiveActivityList({
  activities,
  referenceIso,
  emptyMessage = 'No activity matches these filters.',
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
}) {
  const [count, setCount] = useState(60)
  const sentinel = useRef<HTMLButtonElement>(null)
  const hasMore = count < activities.length
  useEffect(() => {
    const target = sentinel.current
    if (!target || !hasMore || typeof IntersectionObserver === 'undefined') return undefined
    let root = target.parentElement
    while (root && !/(auto|scroll)/.test(getComputedStyle(root).overflowY))
      root = root.parentElement
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) setCount((current) => current + 60)
      },
      { root, rootMargin: '900px 0px' },
    )
    observer.observe(target)
    return () => observer.disconnect()
  }, [count, hasMore])

  if (!activities.length) {
    return <ActivityList activities={[]} referenceIso={referenceIso} emptyMessage={emptyMessage} />
  }

  return (
    <>
      {groupActivitiesByDate(activities.slice(0, count), referenceIso).map((group, index) => {
        const headingId = `activity-date-group-${index}`
        return (
          <section className="activity-date-group" aria-labelledby={headingId} key={group.label}>
            <h3 id={headingId}>{group.label}</h3>
            <ActivityList activities={group.activities} referenceIso={referenceIso} />
          </section>
        )
      })}
      {hasMore ? (
        <button
          ref={sentinel}
          type="button"
          className="activity-load-more"
          aria-label="Load more activities"
          onClick={() => setCount((current) => current + 60)}
        >
          ↓
        </button>
      ) : null}
    </>
  )
}
