import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react'

import {
  activityLocationLabel,
  activityLocationText,
  activityMethod,
  groupActivitiesByDate,
  type ActivityItem,
  type ActivitySort,
  type ActivitySortColumn,
} from '../lib/activity'
import { safeExternalUrl } from '../lib/api'
import { formatCurrency, formatActivityName, valueTone } from '../lib/format'
import { formatActivityDate } from '../lib/spending'
import { AccountMark } from './account-mark'
import { ActivityMethodMark } from './activity-method-mark'
import { BrandMark } from './brand-mark'
import { CategoryMark } from './category-mark'
import { ExternalLink } from './external-link'
import { ArrowUpRight, MapPinned } from './icons'
import { EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const activityColumns = [
  { id: 'description', label: 'Description' },
  { id: 'account', label: 'Account' },
  { id: 'category', label: 'Category' },
  { id: 'location', label: 'Location' },
  { id: 'date', label: 'Date' },
  { id: 'amount', label: 'Amount' },
] as const
const minimumDescriptionWidth = 180
const columnMaxWidths: Partial<Record<ActivitySortColumn, number>> = {
  category: 188,
  location: 232,
}

type TextWeight = 'body' | 'header'
export type ActivityFilterColumn = Exclude<ActivitySortColumn, 'amount'>

export function activityGridWidths(
  activities: ActivityItem[],
  referenceIso: string,
  measure: (text: string, weight: TextWeight) => number,
) {
  const widths = activityColumns.map(({ label }, index) =>
    Math.max(0, measure(label, 'header') + 24 + (index ? 12 : -32)),
  )
  const include = (index: number, text: string, extra: number) => {
    widths[index] = Math.max(widths[index], measure(text, 'body') + extra)
  }
  for (const activity of activities) {
    include(1, activity.account ?? '—', activity.account ? 56 : 24)
    include(2, activity.category, 56)
    const method = activityMethod(activity)
    const location = activityLocationText(activity.location)
    include(
      3,
      location ? activityLocationLabel(activity.location) : method || '—',
      24 + (method ? 30 : 0) + (activity.location?.address ? 24 : 0),
    )
    const date = formatActivityDate(activity.date, referenceIso).replace(/, \d{4}$/, '')
    include(4, date, 24)
    if (activity.pending) include(4, 'Pending', 24)
    include(5, formatCurrency(activity.amount), 24)
  }
  return widths.map((width, index) =>
    index === 0
      ? minimumDescriptionWidth
      : Math.ceil(Math.min(width + 4, columnMaxWidths[activityColumns[index].id] ?? Infinity)),
  )
}

export function ActivityMark({ activity }: { activity: ActivityItem }) {
  const fallback = (
    <CategoryMark
      category={activity.category}
      kind={activity.kind}
      mark={activity.mark}
      amount={activity.amount}
      className={`activity-mark-${activity.kind}`}
    />
  )
  return activity.logoUrl ? (
    <BrandMark
      className={`transaction-mark activity-mark-${activity.kind}`}
      fallback={fallback}
      label={`${activity.title} transaction logo`}
      src={activity.logoUrl}
    />
  ) : (
    fallback
  )
}

export const ActivityList = memo(function ActivityList({
  activities,
  referenceIso,
  emptyMessage = 'No recent activity.',
  compact = false,
  showDescriptions = false,
  showColumns = false,
  conciseTradeGain = false,
  accountTypes,
  onFilter,
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  compact?: boolean
  showDescriptions?: boolean
  showColumns?: boolean
  conciseTradeGain?: boolean
  accountTypes?: Readonly<Record<string, string>>
  onFilter?: (column: ActivityFilterColumn, activity: ActivityItem) => void
}) {
  return (
    <div
      className={`financial-activity-list${showColumns ? ' financial-activity-list-columns' : ''}`}
      data-keyboard-region
      role="group"
      aria-label="Activity entries"
      tabIndex={0}
    >
      {activities.map((activity) => {
        const date = formatActivityDate(activity.date, referenceIso)
        const shortDate = date.replace(/, \d{4}$/, '')
        const location = activity.location
        const locationText = activityLocationText(location)
        const locationPrimary = activityLocationLabel(location)
        const method = activityMethod(activity)
        const locationLabel = locationText ? locationPrimary : method || '—'
        const amount = formatCurrency(activity.amount)
        const website = activity.website ? safeExternalUrl(activity.website) : undefined
        const content = (
          <>
            <ActivityMark activity={activity} />
            <span className="financial-activity-copy">
              <span className="financial-activity-title">
                {showColumns ? (
                  <button
                    type="button"
                    className="financial-activity-filter-button financial-activity-description-button"
                    aria-label={`Filter by description: ${activity.title}`}
                    onClick={() => onFilter?.('description', activity)}
                    disabled={!onFilter}
                  >
                    <strong>{formatActivityName(activity.title)}</strong>
                  </button>
                ) : (
                  <strong>{formatActivityName(activity.title)}</strong>
                )}
                {website && showColumns ? (
                  <ExternalLink
                    className="financial-activity-external"
                    href={website}
                    target="_blank"
                    rel="noopener noreferrer"
                    aria-label={`Open ${activity.title} website`}
                  >
                    <ArrowUpRight aria-hidden="true" size={13} />
                  </ExternalLink>
                ) : website ? (
                  <span className="financial-activity-external">
                    <ArrowUpRight aria-hidden="true" size={13} />
                  </span>
                ) : null}
              </span>
              {!showColumns ? (
                <small>
                  {conciseTradeGain &&
                  activity.kind === 'trade' &&
                  activity.estimatedRealizedGain != null ? (
                    <>
                      {activity.account} ·{' '}
                      <span className={valueTone(activity.estimatedRealizedGain)}>
                        {activity.estimatedRealizedGain > 0 ? '+' : ''}
                        {formatCurrency(activity.estimatedRealizedGain)}
                      </span>
                    </>
                  ) : (
                    activity.detail
                  )}
                </small>
              ) : null}
              {showDescriptions && activity.description ? (
                <small className="transaction-description">{activity.description}</small>
              ) : null}
            </span>
            {showColumns ? (
              <>
                <button
                  type="button"
                  className="financial-activity-column financial-activity-identity"
                  title={activity.account}
                  aria-label={`Filter by account: ${activity.account ?? 'Unavailable'}`}
                  disabled={!onFilter || !activity.accountId}
                  onClick={() => onFilter?.('account', activity)}
                >
                  {activity.account ? (
                    <AccountMark type={accountTypes?.[activity.accountId ?? ''] ?? 'unknown'} />
                  ) : null}
                  <span>{activity.account ?? '—'}</span>
                </button>
                <button
                  type="button"
                  className="financial-activity-column financial-activity-identity"
                  title={activity.category}
                  aria-label={`Filter by category: ${activity.category}`}
                  disabled={!onFilter}
                  onClick={() => onFilter?.('category', activity)}
                >
                  <CategoryMark
                    category={activity.category}
                    kind={activity.kind}
                    mark={activity.mark}
                    amount={activity.amount}
                  />
                  <span>{activity.category}</span>
                </button>
                <span className="financial-activity-column financial-activity-location">
                  <button
                    type="button"
                    className="financial-activity-filter-button financial-activity-location-label"
                    aria-label={
                      locationText
                        ? `Filter by location: ${locationPrimary}`
                        : method
                          ? `Filter by method: ${method}`
                          : 'Location and method unavailable'
                    }
                    disabled={!onFilter || (!locationText && !method)}
                    onClick={() => onFilter?.(locationText ? 'location' : 'method', activity)}
                  >
                    {method ? <ActivityMethodMark method={method} /> : null}
                    <span>{locationLabel}</span>
                  </button>
                  {location?.address ? (
                    <Tooltip>
                      <TooltipTrigger
                        render={
                          <span
                            className="financial-activity-address-trigger"
                            role="img"
                            tabIndex={0}
                            aria-label={`Full address: ${locationText}`}
                          />
                        }
                      >
                        <MapPinned size={14} aria-hidden="true" />
                      </TooltipTrigger>
                      <TooltipContent sideOffset={6}>{locationText}</TooltipContent>
                    </Tooltip>
                  ) : null}
                </span>
                <button
                  type="button"
                  className="financial-activity-column financial-activity-date"
                  aria-label={`Filter by date: ${date}${activity.pending ? ', pending' : ''}`}
                  disabled={!onFilter}
                  onClick={() => onFilter?.('date', activity)}
                >
                  <time dateTime={activity.date}>{shortDate}</time>
                  {activity.pending ? <small>Pending</small> : null}
                </button>
                <span
                  className={`financial-activity-amount ${valueTone(activity.amount)}`}
                  aria-label={`Amount: ${amount}`}
                >
                  {amount}
                </span>
              </>
            ) : (
              <span className="financial-activity-meta">
                <small>
                  {compact && activity.date.slice(0, 4) === referenceIso.slice(0, 4)
                    ? shortDate
                    : date}
                  {activity.pending ? ' · Pending' : ''}
                </small>
                <strong className={valueTone(activity.amount)}>{amount}</strong>
              </span>
            )}
          </>
        )
        return website && !showColumns ? (
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
          <div
            className={`financial-activity-row${showColumns ? ' financial-activity-row-columns' : ''}`}
            key={activity.id}
            data-keyboard-row
            tabIndex={-1}
          >
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
  sizingActivities?: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  showColumns?: boolean
  accountTypes?: Readonly<Record<string, string>>
  sort?: ActivitySort
  onSort?: (column: ActivitySortColumn) => void
  onFilter?: (column: ActivityFilterColumn, activity: ActivityItem) => void
}) {
  // Reset the batch when filtering changes the actual rows, not their array identity.
  return (
    <ProgressiveActivityList key={props.activities.map(({ id }) => id).join('\0')} {...props} />
  )
}

function ProgressiveActivityList({
  activities,
  sizingActivities,
  referenceIso,
  emptyMessage = 'No activity matches these filters.',
  showColumns = false,
  accountTypes,
  sort,
  onSort,
  onFilter,
}: {
  activities: ActivityItem[]
  sizingActivities?: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  showColumns?: boolean
  accountTypes?: Readonly<Record<string, string>>
  sort?: ActivitySort
  onSort?: (column: ActivitySortColumn) => void
  onFilter?: (column: ActivityFilterColumn, activity: ActivityItem) => void
}) {
  const [count, setCount] = useState(60)
  const sentinel = useRef<HTMLButtonElement>(null)
  const table = useRef<HTMLDivElement>(null)
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

  useLayoutEffect(() => {
    const element = table.current
    if (!showColumns || !element || !activities.length) return undefined
    const canvas = document.createElement('canvas')
    const context = canvas.getContext('2d')
    const header = element.querySelector<HTMLElement>('.financial-activity-header')
    const row = element.querySelector<HTMLElement>('.financial-activity-row-columns')
    if (!context || !header || !row) return undefined
    const bodyFont = getComputedStyle(row).font
    const headerFont = getComputedStyle(header).font
    const measure = (value: string, weight: TextWeight) => {
      context.font = weight === 'header' ? headerFont : bodyFont
      return context.measureText(value).width
    }
    const widths = activityGridWidths(sizingActivities ?? activities, referenceIso, measure)
    element.style.setProperty(
      '--activity-grid-columns',
      `32px minmax(${widths[0]}px, 1fr) ${widths
        .slice(1)
        .map((width) => `${width}px`)
        .join(' ')}`,
    )
    element.style.setProperty(
      '--activity-grid-width',
      `${32 + widths.reduce((sum, width) => sum + width, 0) + 16}px`,
    )
    return undefined
  }, [activities, sizingActivities, referenceIso, showColumns])

  if (!activities.length) {
    return (
      <ActivityList
        activities={[]}
        referenceIso={referenceIso}
        emptyMessage={emptyMessage}
        showColumns={showColumns}
        accountTypes={accountTypes}
        onFilter={onFilter}
      />
    )
  }

  const content = (
    <>
      {showColumns ? (
        <div className="financial-activity-header">
          {activityColumns.map(({ id, label }) => {
            const active = sort?.column === id
            return (
              <button
                key={id}
                type="button"
                onClick={() => onSort?.(id)}
                disabled={!onSort}
                aria-label={`Sort by ${label}${active ? `, ${sort.direction}ending` : ''}`}
                aria-pressed={active}
              >
                <span>{label}</span>
                {active ? (
                  <span aria-hidden="true">{sort.direction === 'asc' ? '↑' : '↓'}</span>
                ) : null}
              </button>
            )
          })}
        </div>
      ) : null}
      {showColumns ? (
        <ActivityList
          activities={activities.slice(0, count)}
          referenceIso={referenceIso}
          showColumns
          accountTypes={accountTypes}
          onFilter={onFilter}
        />
      ) : (
        groupActivitiesByDate(activities.slice(0, count), referenceIso).map((group, index) => {
          const headingId = `activity-date-group-${index}`
          return (
            <section className="activity-date-group" aria-labelledby={headingId} key={group.label}>
              <h3 id={headingId}>{group.label}</h3>
              <ActivityList activities={group.activities} referenceIso={referenceIso} />
            </section>
          )
        })
      )}
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
  return showColumns ? (
    <div className="financial-activity-table" ref={table}>
      {content}
    </div>
  ) : (
    content
  )
}
