import {
  ArrowLeftRight,
  BadgeCheck,
  BadgeDollarSign,
  DollarSign,
  ReceiptText,
  WalletCards,
} from 'lucide-react'

import type { ActivityItem } from '../lib/activity'
import { isTauri, openExternalUrl, safeExternalUrl } from '../lib/api'
import { formatCurrency } from '../lib/format'
import { formatActivityDate } from '../lib/spending'
import { BrandMark } from './brand-mark'

const activityMeta = {
  spending: WalletCards,
  transaction: ReceiptText,
  income: DollarSign,
  credit: BadgeDollarSign,
  transfer: ArrowLeftRight,
  trade: ArrowLeftRight,
} as const

export function ActivityList({
  activities,
  referenceIso,
  emptyMessage = 'No recent activity.',
  compact = false,
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  compact?: boolean
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
                <strong>{activity.title}</strong>
                {website ? (
                  <span className="financial-activity-verified" aria-label="Verified website">
                    <BadgeCheck aria-hidden="true" size={13} />
                  </span>
                ) : null}
              </span>
              <small>{activity.detail}</small>
            </span>
            <span className="financial-activity-meta">
              <small>
                {compact ? date.replace(/, \d{4}$/, '') : date}
                {!compact && activity.pending ? ' · Pending' : ''}
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
          <a
            className="financial-activity-row financial-activity-link"
            data-keyboard-row
            data-keyboard-open
            href={website}
            target="_blank"
            rel="noopener noreferrer"
            key={activity.id}
            onClick={(event) => {
              if (!isTauri()) return
              event.preventDefault()
              void openExternalUrl(website)
            }}
          >
            {content}
          </a>
        ) : (
          <div className="financial-activity-row" key={activity.id} data-keyboard-row tabIndex={-1}>
            {content}
          </div>
        )
      })}
      {!activities.length ? <p className="overview-recent-empty">{emptyMessage}</p> : null}
    </div>
  )
}
