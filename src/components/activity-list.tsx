import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { ComponentProps } from 'react'

import {
  activityPlace,
  groupActivitiesByDate,
  type ActivityFilterColumn,
  type ActivityItem,
  type ActivitySort,
  type ActivitySortColumn,
} from '../lib/activity'
import { safeExternalUrl } from '../lib/api'
import { displayCase, formatCurrency, valueTone } from '../lib/format'
import { formatActivityDate, formatListDate } from '../lib/spending'
import { AccountMark } from './account-mark'
import { PlaceMark } from './activity-method-mark'
import { BrandMark } from './brand-mark'
import { CategoryMark } from './category-mark'
import { ExternalLink } from './external-link'
import { AlertCircle, ArrowUpRight, Clock, Copy } from './icons'
import { SortHeadings } from './sort-headings'
import { EmptyState } from './ui'

const activityColumns = [
  { id: 'description', label: 'Description' },
  { id: 'account', label: 'Account' },
  { id: 'category', label: 'Category' },
  { id: 'owner', label: 'For' },
  { id: 'location', label: 'Location' },
  { id: 'date', label: 'Date' },
  { id: 'amount', label: 'Amount' },
] as const
// Text widths repeat across rows and renders (account names, categories), so measure each once.
const measuredText = new Map<string, number>()
// Column widths per sizing list. Activity sizes from every activity, which doesn't change while
// filtering, so each keystroke reuses them instead of re-measuring thousands of rows.
const gridWidths = new WeakMap<ActivityItem[], { key: string; widths: number[] }>()
const minimumDescriptionWidth = 180
// Columns that give way, in order, when the ledger is too narrow for all of them.
const optionalColumns = [4, 3, 2, 1]
type TextWeight = 'body' | 'header' | 'meta'

// Cell geometry, mirroring the CSS: 12px padding each side, a 24px mark plus an 8px gap, and the
// filter button's 6px inset. Kept here so the measured widths match what is drawn.
const cellPadding = 24
const markWidth = 32
const buttonInset = 6
const widestTextColumn = 280
// Canvas text runs a few pixels narrower than laid-out text (hinting, tabular figures).
const textSlack = 8

/** The width that fits 95 of 100 rows; the rest ellipsize and keep their full text in a tooltip. */
function typicalWidth(widths: number[]) {
  if (!widths.length) return 0
  const sorted = widths.toSorted((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))]
}

/**
 * Column widths for the ledger. Places size to the 95th percentile so one long address does not
 * widen every row; accounts and categories fit their longest name up to a cap; dates and amounts
 * always fit, since truncating them would hide evidence. Second lines use their smaller size.
 */
export function activityGridWidths(
  activities: ActivityItem[],
  referenceIso: string,
  measure: (text: string, weight: TextWeight) => number,
) {
  const header = activityColumns.map(({ label }, index) =>
    Math.max(0, measure(label, 'header') + cellPadding + (index ? 0 : -32)),
  )
  const columns: number[][] = activityColumns.map(() => [])
  const twoLine = (main: string, sub: string | undefined) =>
    Math.max(measure(main, 'body'), sub ? measure(sub, 'meta') : 0) + textSlack
  for (const activity of activities) {
    columns[1].push(
      measure(activity.account ?? '—', 'body') +
        textSlack +
        (activity.account ? markWidth : 0) +
        cellPadding +
        buttonInset,
    )
    columns[2].push(
      twoLine(activity.category, activity.subcategory) + markWidth + cellPadding + buttonInset,
    )
    if (activity.other != null)
      columns[3].push(
        Math.max(measure('Me', 'body'), measure('Other', 'body')) +
          textSlack +
          cellPadding +
          buttonInset,
      )
    // Rows without a place would drag the percentile down to a dash; they always fit anyway.
    const place = activityPlace(activity)
    if (place.kind !== 'none')
      columns[4].push(
        twoLine(place.primary, place.secondary) + markWidth + cellPadding + buttonInset,
      )
    const date = formatListDate(activity.date, referenceIso)
    // The centered date, with room for its status icon (12px and a 6px gap) on each side so the
    // icon can hang past the date without moving it.
    columns[5].push(
      measure(date, 'body') +
        (activityStatus(activity) ? 36 : 0) +
        textSlack +
        cellPadding +
        buttonInset * 2,
    )
    columns[6].push(measure(formatCurrency(activity.amount), 'body') + textSlack + cellPadding)
  }
  return activityColumns.map(({ id }, index) => {
    if (index === 0) return minimumDescriptionWidth
    const values = columns[index]
    // Accounts and categories are a short, known list, so all of them fit. Places are open-ended,
    // so the rare long one ellipsizes instead of widening every row.
    const content =
      id === 'location'
        ? Math.min(typicalWidth(values), widestTextColumn)
        : id === 'date' || id === 'amount'
          ? Math.max(0, ...values)
          : Math.min(Math.max(0, ...values), widestTextColumn)
    return Math.ceil(Math.max(header[index], content))
  })
}

/** What a row's date carries beside it: still pending, or a second look Rust suggests. */
export function activityStatus(activity: Pick<ActivityItem, 'pending' | 'flag'>) {
  if (activity.pending) return { icon: Clock, label: 'Pending', warning: false }
  if (activity.flag === 'duplicate')
    return { icon: Copy, label: 'Possible duplicate', warning: true }
  if (activity.flag === 'unusual')
    return { icon: AlertCircle, label: 'Unusual amount', warning: true }
  return undefined
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
  showDescriptions = false,
  showColumns = false,
  conciseTradeGain = false,
  accountTypes,
  accountInstitutions,
  onFilter,
  onToggleOther,
  onOpen,
  selectedId,
  highlightedIds,
  onHighlight,
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  showDescriptions?: boolean
  showColumns?: boolean
  conciseTradeGain?: boolean
  accountTypes?: Readonly<Record<string, string>>
  /** Institutions and domains by account ID, for the account column's logos. */
  accountInstitutions?: Readonly<Record<string, { institution: string; domain?: string }>>
  onFilter?: (column: ActivityFilterColumn, activity: ActivityItem) => void
  /** Switches a transaction between yours and someone else's. */
  onToggleOther?: (activity: ActivityItem) => void
  /** Opens the activity's details; the description becomes the way in. */
  onOpen?: (activity: ActivityItem) => void
  selectedId?: string
  /** Rows to mark, such as the day under a linked chart's pointer. */
  highlightedIds?: ReadonlySet<string>
  /** The row under the pointer or keyboard focus; null when it leaves. */
  onHighlight?: (activity: ActivityItem | null) => void
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
        const shortDate = formatListDate(activity.date, referenceIso)
        const place = activityPlace(activity)
        const amount = formatCurrency(activity.amount)
        const status = activityStatus(activity)
        const website = activity.website ? safeExternalUrl(activity.website) : undefined
        const linked = onHighlight
          ? {
              onPointerEnter: () => onHighlight(activity),
              onPointerLeave: () => onHighlight(null),
              onFocus: () => onHighlight(activity),
              onBlur: () => onHighlight(null),
            }
          : {}
        const content = (
          <>
            <ActivityMark activity={activity} />
            <span className="financial-activity-copy">
              <span className="financial-activity-title" title={activity.title}>
                {showColumns ? (
                  <>
                    <button
                      type="button"
                      className="financial-activity-filter-button financial-activity-description-button"
                      aria-label={`Filter by description: ${activity.title}`}
                      onClick={() => onFilter?.('description', activity)}
                      disabled={!onFilter}
                    >
                      <strong>{displayCase(activity.title)}</strong>
                    </button>
                    {onOpen ? (
                      <button
                        type="button"
                        className="financial-activity-open"
                        aria-haspopup="dialog"
                        onClick={() => onOpen(activity)}
                      >
                        Open details: {activity.title}
                      </button>
                    ) : null}
                  </>
                ) : (
                  <strong>{displayCase(activity.title)}</strong>
                )}
                {website && showColumns ? (
                  <ExternalLink
                    className="financial-activity-external"
                    href={website}
                    target="_blank"
                    rel="noopener noreferrer"
                    aria-label={`Open ${activity.title} website`}
                  >
                    <ArrowUpRight aria-hidden="true" size={12} />
                  </ExternalLink>
                ) : website ? (
                  <span className="financial-activity-external">
                    <ArrowUpRight aria-hidden="true" size={12} />
                  </span>
                ) : null}
              </span>
              {!showColumns ? (
                <small
                  title={
                    conciseTradeGain &&
                    activity.kind === 'trade' &&
                    activity.estimatedRealizedGain != null
                      ? undefined
                      : activity.detail
                  }
                >
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
              {activity.benefit ? (
                <small
                  className={`financial-activity-benefit${activity.benefit.tone ? ` is-${activity.benefit.tone}` : ''}`}
                >
                  {activity.benefit.text}
                </small>
              ) : null}
            </span>
            {showColumns ? (
              <>
                <span
                  className="financial-activity-column financial-activity-identity"
                  data-column="account"
                >
                  <button
                    type="button"
                    className="financial-activity-cell-button"
                    title={activity.account}
                    aria-label={`Filter by account: ${activity.account ?? 'Unavailable'}`}
                    disabled={!onFilter || !activity.accountId}
                    onClick={() => onFilter?.('account', activity)}
                  >
                    {activity.account ? (
                      <AccountMark
                        type={accountTypes?.[activity.accountId ?? ''] ?? 'unknown'}
                        {...accountInstitutions?.[activity.accountId ?? '']}
                      />
                    ) : null}
                    <span className="financial-activity-cell-text">
                      <span>{activity.account ?? '—'}</span>
                    </span>
                  </button>
                </span>
                <span
                  className="financial-activity-column financial-activity-identity"
                  data-column="category"
                >
                  <button
                    type="button"
                    className="financial-activity-cell-button"
                    title={
                      activity.subcategory
                        ? `${activity.category} · ${activity.subcategory}`
                        : activity.category
                    }
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
                    <span className="financial-activity-cell-text">
                      <span>{activity.category}</span>
                      {activity.subcategory ? <small>{activity.subcategory}</small> : null}
                    </span>
                  </button>
                </span>
                <span className="financial-activity-column" data-column="owner">
                  {activity.other != null ? (
                    <button
                      type="button"
                      className={`financial-activity-cell-button financial-activity-owner${activity.other ? ' is-other' : ''}`}
                      aria-label={`Someone else's: ${activity.title}`}
                      aria-pressed={activity.other}
                      disabled={!onToggleOther}
                      onClick={() => onToggleOther?.(activity)}
                    >
                      <span className="financial-activity-cell-text">
                        <span>{activity.other ? 'Other' : 'Me'}</span>
                      </span>
                    </button>
                  ) : null}
                </span>
                <span
                  className={`financial-activity-column financial-activity-location place-${place.kind}`}
                  data-column="location"
                >
                  <button
                    type="button"
                    className="financial-activity-cell-button"
                    title={
                      place.secondary ? `${place.primary} · ${place.secondary}` : place.primary
                    }
                    aria-label={
                      place.kind === 'place'
                        ? `Filter by location: ${place.primary}`
                        : place.kind === 'none'
                          ? 'Location unavailable'
                          : `Filter by method: ${place.primary}`
                    }
                    disabled={!onFilter || place.kind === 'none'}
                    onClick={() =>
                      onFilter?.(place.kind === 'place' ? 'location' : 'method', activity)
                    }
                  >
                    {place.kind === 'none' ? null : <PlaceMark kind={place.kind} />}
                    <span className="financial-activity-cell-text">
                      <span>{place.primary}</span>
                      {place.secondary ? <small>{place.secondary}</small> : null}
                    </span>
                  </button>
                </span>
                <span className="financial-activity-column financial-activity-date">
                  <button
                    type="button"
                    className="financial-activity-cell-button"
                    aria-label={`Filter by date: ${date}${status ? `, ${status.label.toLocaleLowerCase()}` : ''}`}
                    disabled={!onFilter}
                    onClick={() => onFilter?.('date', activity)}
                  >
                    <span className="financial-activity-date-line">
                      <time dateTime={activity.date}>{shortDate}</time>
                      {status ? (
                        <span
                          className={`financial-activity-status${status.warning ? ' activity-flag' : ''}`}
                          title={status.label}
                        >
                          <status.icon size={12} aria-hidden="true" />
                          <span className="sr-only">{status.label}</span>
                        </span>
                      ) : null}
                    </span>
                  </button>
                </span>
                <span
                  className={`financial-activity-amount ${valueTone(activity.amount)}`}
                  aria-label={`Amount: ${amount}`}
                  data-copy-amount
                >
                  {amount}
                </span>
              </>
            ) : (
              <span className="financial-activity-meta">
                <small>
                  {shortDate}
                  {activity.pending ? ' · Pending' : ''}
                  {!activity.pending && activity.flag ? (
                    <span className="activity-flag">
                      {activity.flag === 'duplicate' ? ' · Duplicate?' : ' · Unusual'}
                    </span>
                  ) : null}
                </small>
                <strong className={valueTone(activity.amount)} data-copy-amount>
                  {amount}
                </strong>
              </span>
            )}
          </>
        )
        return website && !showColumns ? (
          <ExternalLink
            className="financial-activity-row financial-activity-link"
            data-keyboard-row
            data-keyboard-open
            data-highlighted={highlightedIds?.has(activity.id) || undefined}
            {...linked}
            href={website}
            target="_blank"
            rel="noopener noreferrer"
            key={activity.id}
          >
            {content}
          </ExternalLink>
        ) : (
          <div
            className={`financial-activity-row${showColumns ? ' financial-activity-row-columns' : ''}${onOpen ? ' is-openable' : ''}`}
            key={activity.id}
            data-activity-id={activity.id}
            data-keyboard-row
            data-keyboard-open={onOpen ? true : undefined}
            data-selected={selectedId === activity.id || undefined}
            data-highlighted={highlightedIds?.has(activity.id) || undefined}
            {...linked}
            tabIndex={-1}
            onClick={
              onOpen
                ? (event) => {
                    // Text is a filter; the space around it opens the details.
                    if (event.target instanceof Element && event.target.closest('button, a')) {
                      return
                    }
                    if (window.getSelection()?.toString()) return
                    onOpen(activity)
                  }
                : undefined
            }
          >
            {content}
          </div>
        )
      })}
      {!activities.length ? <EmptyState>{emptyMessage}</EmptyState> : null}
    </div>
  )
})

type GroupedActivityListProps = {
  activities: ActivityItem[]
  sizingActivities?: ActivityItem[]
  referenceIso: string
  emptyMessage?: string
  showColumns?: boolean
  accountTypes?: Readonly<Record<string, string>>
  /** Institutions and domains by account ID, for the account column's logos. */
  accountInstitutions?: Readonly<Record<string, { institution: string; domain?: string }>>
  sort?: ActivitySort
  onSort?: (column: ActivitySortColumn) => void
  onFilter?: (column: ActivityFilterColumn, activity: ActivityItem) => void
  /** Switches a transaction between yours and someone else's. */
  onToggleOther?: (activity: ActivityItem) => void
  onOpen?: (activity: ActivityItem) => void
  selectedId?: string
}

export function GroupedActivityList(props: GroupedActivityListProps) {
  // Reset the batch when filtering changes the actual rows, not their array identity.
  return (
    <ProgressiveActivityList key={props.activities.map(({ id }) => id).join('\0')} {...props} />
  )
}

const activityPageSize = 60

/**
 * Renders a long list a page at a time, adding a page as the end nears its scroll container.
 * `minimum` keeps rows that must be shown, such as highlighted ones, within the rendered pages.
 */
function useActivityPages(total: number, minimum = 0) {
  const [pages, setPages] = useState(1)
  const sentinel = useRef<HTMLButtonElement>(null)
  const count = Math.max(pages * activityPageSize, minimum)
  const hasMore = count < total
  useEffect(() => {
    const target = sentinel.current
    if (!target || !hasMore || typeof IntersectionObserver === 'undefined') return undefined
    let root = target.parentElement
    while (root && !/(auto|scroll)/.test(getComputedStyle(root).overflowY))
      root = root.parentElement
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) setPages(Math.ceil(count / activityPageSize) + 1)
      },
      { root, rootMargin: '900px 0px' },
    )
    observer.observe(target)
    return () => observer.disconnect()
  }, [count, hasMore])
  const more = hasMore ? (
    <button
      ref={sentinel}
      type="button"
      className="activity-load-more"
      aria-label="Load more activities"
      onClick={() => setPages(Math.ceil(count / activityPageSize) + 1)}
    >
      ↓
    </button>
  ) : null
  return { count, more }
}

/** `ActivityList` rendered a page at a time inside its scroll container. */
export function PagedActivityList({
  activities,
  minimumCount,
  ...props
}: ComponentProps<typeof ActivityList> & { minimumCount?: number }) {
  const { count, more } = useActivityPages(activities.length, minimumCount)
  const visible = useMemo(() => activities.slice(0, count), [activities, count])
  return (
    <>
      <ActivityList activities={visible} {...props} />
      {more}
    </>
  )
}

function ProgressiveActivityList({
  activities,
  sizingActivities,
  referenceIso,
  emptyMessage = 'No activity matches these filters.',
  showColumns = false,
  accountTypes,
  accountInstitutions,
  sort,
  onSort,
  onFilter,
  onToggleOther,
  onOpen,
  selectedId,
}: GroupedActivityListProps) {
  const { count, more } = useActivityPages(activities.length)
  const table = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const element = table.current
    if (!showColumns || !element || !activities.length) return undefined
    const canvas = document.createElement('canvas')
    const context = canvas.getContext('2d')
    const header = element.querySelector<HTMLElement>('.financial-activity-header')
    const row = element.querySelector<HTMLElement>('.financial-activity-row-columns')
    if (!context || !header || !row) return undefined
    // Built from parts: WebKit and Chromium leave the computed `font` shorthand empty, which
    // would silently measure everything in the canvas default of 10px.
    const fontOf = (style: CSSStyleDeclaration, size = style.fontSize) =>
      `${style.fontStyle} ${style.fontWeight} ${size} ${style.fontFamily}`
    const bodyStyle = getComputedStyle(row)
    const fonts: Record<TextWeight, string> = {
      header: fontOf(getComputedStyle(header)),
      body: fontOf(bodyStyle),
      meta: fontOf(bodyStyle, bodyStyle.getPropertyValue('--type-meta').trim() || '12px'),
    }
    const measure = (value: string, weight: TextWeight) => {
      const key = `${fonts[weight]}\0${value}`
      const cached = measuredText.get(key)
      if (cached != null) return cached
      context.font = fonts[weight]
      const width = context.measureText(value).width
      if (measuredText.size > 5_000) measuredText.clear()
      measuredText.set(key, width)
      return width
    }
    const sizing = sizingActivities ?? activities
    const widthsKey = `${referenceIso}\0${fonts.header}\0${fonts.body}\0${fonts.meta}`
    const cachedWidths = gridWidths.get(sizing)
    const widths =
      cachedWidths?.key === widthsKey
        ? cachedWidths.widths
        : activityGridWidths(sizing, referenceIso, measure)
    gridWidths.set(sizing, { key: widthsKey, widths })
    // Amounts and dates never clip: when the ledger is narrower than its columns, places go
    // first, then who it's for, categories, and accounts; each stays in the transaction inspector.
    const fit = () => {
      const width = (hidden: number[]) =>
        32 +
        widths.reduce((sum, value, index) => sum + (hidden.includes(index) ? 0 : value), 0) +
        16
      const hidden = optionalColumns.reduce<number[]>(
        (dropped, index) => (width(dropped) > element.clientWidth ? [...dropped, index] : dropped),
        [],
      )
      const shown = widths.flatMap((value, index) => (hidden.includes(index) ? [] : [value]))
      element.dataset.hiddenColumns = hidden.map((index) => activityColumns[index].id).join(' ')
      element.style.setProperty(
        '--activity-grid-columns',
        `32px minmax(${shown[0]}px, 1fr) ${shown
          .slice(1)
          .map((value) => `${value}px`)
          .join(' ')}`,
      )
      element.style.setProperty('--activity-grid-width', `${width(hidden)}px`)
    }
    fit()
    if (typeof ResizeObserver === 'undefined') return undefined
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => observer.disconnect()
  }, [activities, sizingActivities, referenceIso, showColumns])

  if (!activities.length) {
    return (
      <ActivityList
        activities={[]}
        referenceIso={referenceIso}
        emptyMessage={emptyMessage}
        showColumns={showColumns}
        accountTypes={accountTypes}
        accountInstitutions={accountInstitutions}
        onFilter={onFilter}
      />
    )
  }

  const content = (
    <>
      {showColumns ? (
        <div className="financial-activity-header">
          <SortHeadings
            columns={activityColumns}
            sort={sort}
            command="activity.sort."
            onSort={onSort}
          />
        </div>
      ) : null}
      {showColumns ? (
        <ActivityList
          activities={activities.slice(0, count)}
          referenceIso={referenceIso}
          showColumns
          accountTypes={accountTypes}
          accountInstitutions={accountInstitutions}
          onFilter={onFilter}
          onToggleOther={onToggleOther}
          onOpen={onOpen}
          selectedId={selectedId}
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
      {more}
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
