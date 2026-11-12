import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { ComponentProps, ReactNode, RefObject } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useLatest } from '../hooks/use-latest'
import {
  activityPlace,
  activityTime,
  groupActivitiesByDate,
  type ActivityFilterColumn,
  type ActivityItem,
  type ActivitySort,
  type ActivitySortColumn,
} from '../lib/activity'
import { displayCase, formatCurrency, valueTone } from '../lib/format'
import { formatActivityDate, formatListDate } from '../lib/spending'
import { AccountMark } from './account-mark'
import { PlaceMark } from './activity-method-mark'
import { BrandMark } from './brand-mark'
import { CategoryMark } from './category-mark'
import { SortHeadings } from './sort-headings'
import { EmptyState } from './ui'

const activityColumns = [
  { id: 'date', label: 'Date' },
  { id: 'description', label: 'Description' },
  { id: 'account', label: 'Account' },
  { id: 'category', label: 'Category' },
  { id: 'owner', label: 'For' },
  { id: 'location', label: 'Location' },
  { id: 'amount', label: 'Amount' },
] as const
// Text widths repeat across rows and renders (account names, categories), so measure each once.
const measuredText = new Map<string, number>()
// Column widths per sizing list. Activity sizes from every activity, which doesn't change while
// filtering, so each keystroke reuses them instead of re-measuring thousands of rows.
const gridWidths = new WeakMap<ActivityItem[], { key: string; widths: number[] }>()
// The description gives way down to this before any column is hidden.
const minimumDescriptionWidth = 180
const descriptionIndex = 1
const locationIndex = 5
// A place fits 95 of 100 rows up to this; a longer one, such as a full airport address, wraps
// onto more lines rather than widening the column for every row.
const widestLocation = 220
// When the ledger is tight, the place narrows to this before the description does.
const minimumLocationWidth = 140
// Columns that give way, in order, when the ledger is too narrow for all of them.
const optionalColumns = [5, 4, 3, 2]
// `strong` is the bold of descriptions and amounts.
type TextWeight = 'body' | 'strong' | 'header' | 'meta'

// Cell geometry, mirroring the CSS: 12px padding each side, a 24px mark plus an 8px gap, and the
// filter button's 6px inset. Kept here so the measured widths match what is drawn.
const cellPadding = 24
const markWidth = 32
const buttonInset = 6
const widestDescription = 280
// Canvas text runs a few pixels narrower than laid-out text (hinting, tabular figures).
const textSlack = 8

/** A ledger row's width: its columns, the 32px mark, and 8px row padding each side. */
function rowWidth(columns: number[], hidden: number[]) {
  return (
    32 + columns.reduce((sum, value, index) => sum + (hidden.includes(index) ? 0 : value), 0) + 16
  )
}

/** The width that fits 95 of 100 rows on one line; the rest wrap. */
function typicalWidth(widths: number[]) {
  const sorted = widths.toSorted((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))] ?? 0
}

/**
 * Column widths for the ledger. Account, category, date, and amount fit their widest value, both
 * lines included, so none wraps. The description and the place fit 95 of 100 rows, up to a cap,
 * so one long title or address wraps instead of widening the column for every row; the
 * description takes the remaining room, and its width here is only how far it may narrow.
 */
export function activityGridWidths(
  activities: ActivityItem[],
  referenceIso: string,
  measure: (text: string, weight: TextWeight) => number,
) {
  const columns: Record<(typeof activityColumns)[number]['id'], number[]> = {
    date: [],
    description: [],
    account: [],
    category: [],
    owner: [],
    location: [],
    amount: [],
  }
  const twoLine = (main: string, sub: string | undefined) =>
    Math.max(measure(main, 'body'), sub ? measure(sub, 'meta') : 0) + textSlack
  // A badge sits 4px after the title, with 6px padding each side.
  const badge = (label: string | undefined) => (label ? measure(label, 'meta') + 16 : 0)
  for (const activity of activities) {
    columns.description.push(
      measure(displayCase(activity.title), 'strong') +
        badge(activity.benefit?.label) +
        badge(activityStatus(activity)?.badge) +
        textSlack +
        cellPadding,
    )
    columns.account.push(
      measure(activity.account ?? '—', 'body') +
        textSlack +
        (activity.account ? markWidth : 0) +
        cellPadding +
        buttonInset,
    )
    columns.category.push(
      twoLine(activity.category, activity.subcategory) + markWidth + cellPadding + buttonInset,
    )
    if (activity.other != null)
      columns.owner.push(
        Math.max(measure('Me', 'body'), measure('Other', 'body')) +
          textSlack +
          cellPadding +
          buttonInset,
      )
    const place = activityPlace(activity)
    if (place.kind !== 'none')
      columns.location.push(
        twoLine(place.primary, place.secondary) + markWidth + cellPadding + buttonInset,
      )
    const date = formatListDate(activity.date, referenceIso)
    columns.date.push(
      Math.max(measure(date, 'body'), measure(activityTime(activity), 'meta')) +
        textSlack +
        cellPadding +
        buttonInset * 2,
    )
    // The amount ends the row, so it pads only its left side.
    columns.amount.push(
      measure(formatCurrency(activity.amount), 'strong') + textSlack + cellPadding / 2,
    )
  }
  return activityColumns.map(({ id, label }) => {
    const values = columns[id]
    // The description heading also spans the mark's column.
    const header = Math.max(
      0,
      measure(label, 'header') + cellPadding + (id === 'description' ? -32 : 0),
    )
    const content =
      id === 'description'
        ? Math.min(typicalWidth(values), widestDescription)
        : id === 'location'
          ? Math.min(typicalWidth(values), widestLocation)
          : Math.max(0, ...values)
    return Math.ceil(Math.max(header, content))
  })
}

/** What a row's merchant carries beside it: still pending, or a second look Rust suggests. */
export function activityStatus(activity: Pick<ActivityItem, 'pending' | 'flag' | 'wallet'>) {
  if (activity.pending)
    return {
      badge: 'Pending',
      label: activity.wallet ? 'Pending, from Apple Wallet' : 'Pending',
      warning: false,
    }
  if (activity.flag === 'duplicate')
    return { badge: 'Duplicate?', label: 'Possible duplicate', warning: true }
  if (activity.flag === 'unusual')
    return { badge: 'Unusual', label: 'Unusual amount', warning: true }
  if (activity.flag === 'conflict')
    return { badge: 'Check', label: 'Sources disagree', warning: true }
  if (activity.flag === 'new')
    return { badge: 'New', label: 'New recurring series', warning: false }
  if (activity.flag === 'price') return { badge: 'Price', label: 'New price', warning: true }
  if (activity.flag === 'fee')
    return { badge: 'Fee', label: 'Fee or interest charged', warning: true }
  if (activity.flag === 'pay')
    return { badge: 'Pay', label: 'Paycheck outside its usual range', warning: true }
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

/** Which pages of a long list `ActivityList` draws; every other page is a gap of its height. */
export type ActivityPages = {
  size: number
  shown: ReadonlySet<number>
  heights: ReadonlyMap<number, number>
}

/**
 * The list's rows, or with `pages`, each page behind a marker that also stands in for the page
 * at its measured height while it isn't drawn.
 */
function drawRows(
  activities: ActivityItem[],
  pages: ActivityPages | undefined,
  row: (activity: ActivityItem) => ReactNode,
) {
  if (!pages) return activities.map(row)
  const drawn: ReactNode[] = []
  for (let start = 0; start < activities.length; start += pages.size) {
    const page = start / pages.size
    const shown = pages.shown.has(page)
    drawn.push(
      <div
        key={`page-${page}`}
        aria-hidden
        data-activity-page={page}
        style={shown ? undefined : { height: pages.heights.get(page) ?? 0 }}
      />,
    )
    if (shown) drawn.push(...activities.slice(start, start + pages.size).map(row))
  }
  return drawn
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
  checkedIds,
  highlightedIds,
  onHighlight,
  pages,
}: {
  activities: ActivityItem[]
  referenceIso: string
  /** Draws only these pages of a long list; see `useActivityWindow`. */
  pages?: ActivityPages
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
  /** Rows chosen for a bulk edit. */
  checkedIds?: ReadonlySet<string>
  /** Rows to mark, such as the day under a linked chart's pointer. */
  highlightedIds?: ReadonlySet<string>
  /** The row under the pointer or keyboard focus; null when it leaves. */
  onHighlight?: (activity: ActivityItem | null) => void
}) {
  // Memoized, so it subscribes to the privacy mask itself to re-format amounts when it changes.
  useAmountsHidden()
  return (
    <div
      className={`financial-activity-list${showColumns ? ' financial-activity-list-columns' : ''}`}
      data-keyboard-region
      role="group"
      aria-label="Activity entries"
      tabIndex={0}
    >
      {drawRows(activities, pages, (activity) => {
        const date = formatActivityDate(activity.date)
        const shortDate = formatListDate(activity.date, referenceIso)
        const place = activityPlace(activity)
        const amount = formatCurrency(activity.amount)
        const time = activityTime(activity)
        const status = activityStatus(activity)
        const linked = onHighlight
          ? {
              onPointerEnter: () => onHighlight(activity),
              onPointerLeave: () => onHighlight(null),
              onFocus: () => onHighlight(activity),
              onBlur: () => onHighlight(null),
            }
          : {}
        const checked = checkedIds?.has(activity.id) || undefined
        const content = (
          <>
            {showColumns ? (
              <span className="financial-activity-column financial-activity-date">
                <button
                  type="button"
                  className="financial-activity-cell-button"
                  aria-label={`Filter by date: ${date}`}
                  disabled={!onFilter}
                  onClick={() => onFilter?.('date', activity)}
                >
                  {time ? (
                    <span className="financial-activity-cell-text">
                      <time dateTime={activity.date}>{shortDate}</time>
                      <small>{time}</small>
                    </span>
                  ) : (
                    <time dateTime={activity.date}>{shortDate}</time>
                  )}
                </button>
              </span>
            ) : null}
            <ActivityMark activity={activity} />
            {checked ? <span className="sr-only">Selected. </span> : null}
            <span className="financial-activity-copy">
              <span className="financial-activity-title">
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
                {activity.benefit ? (
                  <span
                    className={`activity-badge financial-activity-benefit${activity.benefit.tone ? ` is-${activity.benefit.tone}` : ''}`}
                    title={activity.benefit.text}
                  >
                    {activity.benefit.label}
                    <span className="sr-only">: {activity.benefit.text}</span>
                  </span>
                ) : null}
                {status ? (
                  <span
                    className={`activity-badge${status.warning ? ' is-warning' : ''}`}
                    title={status.label}
                  >
                    {status.badge}
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
                    aria-label={
                      place.kind === 'place'
                        ? `Filter by location: ${place.primary}`
                        : place.kind === 'transfer'
                          ? `Filter by account: ${activity.transfer?.account}`
                          : place.kind === 'none'
                            ? 'Location unavailable'
                            : `Filter by method: ${place.primary}`
                    }
                    disabled={!onFilter || place.kind === 'none'}
                    onClick={() =>
                      onFilter?.(
                        place.kind === 'place' || place.kind === 'transfer' ? 'location' : 'method',
                        activity,
                      )
                    }
                  >
                    {place.kind === 'none' ? null : <PlaceMark kind={place.kind} />}
                    <span className="financial-activity-cell-text">
                      <span>{place.primary}</span>
                      {place.secondary ? <small>{place.secondary}</small> : null}
                    </span>
                  </button>
                </span>
                <span
                  className={`financial-activity-amount ${valueTone(activity.amount)}`}
                  aria-label={`Amount: ${amount}`}
                >
                  <span data-copy-amount>{amount}</span>
                </span>
              </>
            ) : (
              <span className="financial-activity-meta">
                <small>{shortDate}</small>
                <strong className={valueTone(activity.amount)} data-copy-amount>
                  {amount}
                </strong>
              </span>
            )}
          </>
        )
        return (
          <div
            className={`financial-activity-row${showColumns ? ' financial-activity-row-columns' : ''}${onOpen ? ' is-openable' : ''}`}
            key={activity.id}
            data-activity-id={activity.id}
            data-keyboard-row
            data-keyboard-open={onOpen ? true : undefined}
            data-selected={selectedId === activity.id || undefined}
            data-checked={checked}
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
  /** Rows chosen for a bulk edit. */
  checkedIds?: ReadonlySet<string>
  /**
   * A row pinned under the columned ledger, laid out on its columns: a child spanning to the
   * last column ends where the amounts do, whichever columns are showing.
   */
  footer?: ReactNode
}

export function GroupedActivityList(props: GroupedActivityListProps) {
  return <ProgressiveActivityList {...props} />
}

const activityPageSize = 60

/** The nearest ancestor that scrolls, which the list's windowing and paging measure against. */
function scrollContainer(element: HTMLElement) {
  let root = element.parentElement
  while (root && !/auto|scroll/.test(getComputedStyle(root).overflowY)) root = root.parentElement
  return root
}

/**
 * Renders a long list a page at a time: the next page mounts once the end of the rendered rows comes
 * within a viewport and a half of the scroll container, so scrolling and row motions stay ahead of
 * it while rows nobody reaches are never built. `minimum` keeps rows that must be shown, such as
 * highlighted ones, within the rendered pages. A new `rows` (what identifies the list's content)
 * starts again from the first page.
 */
function useActivityPages(total: number, minimum = 0, rows?: string) {
  const [pages, setPages] = useState(1)
  const [pagedRows, setPagedRows] = useState(rows)
  if (pagedRows !== rows) {
    setPagedRows(rows)
    setPages(1)
  }
  const sentinel = useRef<HTMLButtonElement>(null)
  const count = Math.max(pages * activityPageSize, minimum)
  const hasMore = count < total
  useEffect(() => {
    const target = sentinel.current
    if (!target || !hasMore || typeof IntersectionObserver === 'undefined') return undefined
    // The margin applies only to the root, so observe within the list's own scroll container.
    const root = scrollContainer(target)
    // A new observer per page reports at once, so pages keep coming while the end stays in range.
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) setPages(Math.ceil(count / activityPageSize) + 1)
      },
      { root, rootMargin: '0px 0px 150% 0px' },
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
  /** Loads through the page after `index`'s, so a row motion always has a next row. */
  const reach = (index: number) => {
    const page = Math.floor(index / activityPageSize) + 1
    if (hasMore && page >= Math.ceil(count / activityPageSize)) setPages(page + 1)
  }
  return { count, more, reach }
}

/**
 * Keeps a long ledger's DOM bounded: of the loaded pages it draws those within a viewport and a
 * half of the scroll position, the first and last (so G G, ⇧G, and wrapping keep their rows),
 * the focused row's page and its neighbours (so J and K always have a next row), and the
 * selected row's. Every other page is one gap of the height it had when last drawn, so the
 * scroll position never moves. Rows wrap to varying heights, so nothing is estimated: a page is
 * drawn until it has been measured.
 */
function useActivityWindow(
  list: RefObject<HTMLElement | null>,
  activities: ActivityItem[],
  count: number,
  focusedPage: number | undefined,
  selectedId: string | undefined,
): ActivityPages {
  const size = activityPageSize
  const loaded = Math.ceil(Math.min(count, activities.length) / size)
  // Heights belong to one list of rows; new rows are measured again.
  const heights = useMemo(() => new Map<number, number>(), [activities])
  const [near, setNear] = useState<{ rows: ActivityItem[]; pages: ReadonlySet<number> }>()
  const selectedIndex = useMemo(
    () => (selectedId ? activities.findIndex(({ id }) => id === selectedId) : -1),
    [activities, selectedId],
  )
  const selectedPage = selectedIndex < 0 ? undefined : Math.floor(selectedIndex / size)
  const nearPages = near?.rows === activities ? near.pages : undefined
  const shown = useMemo(() => {
    const pinned = [0, loaded - 1, selectedPage]
    if (focusedPage != null) pinned.push(focusedPage - 1, focusedPage, focusedPage + 1)
    const pages = new Set<number>()
    for (let page = 0; page < loaded; page += 1)
      if (!heights.has(page) || nearPages?.has(page) || pinned.includes(page)) pages.add(page)
    return pages
  }, [loaded, heights, nearPages, focusedPage, selectedPage])

  // Measures the drawn pages and finds the ones near the scroll position, after every render
  // and on each scroll; a render with the same pages near changes nothing.
  const update = useLatest(() => {
    const element = list.current
    const markers = element?.querySelectorAll<HTMLElement>('[data-activity-page]')
    const end = markers?.[0]?.parentElement?.getBoundingClientRect().bottom
    if (!element || !markers || end == null) return
    const tops = [...markers].map((marker) => marker.getBoundingClientRect().top)
    const bottom = (page: number) => tops[page + 1] ?? end
    tops.forEach((top, page) => {
      if (shown.has(page)) heights.set(page, bottom(page) - top)
    })
    const root = scrollContainer(element)
    const view = root?.getBoundingClientRect()
    const margin = (view?.height ?? 0) * 1.5
    const pages = new Set<number>()
    if (view)
      tops.forEach((top, page) => {
        if (bottom(page) > view.top - margin && top < view.bottom + margin) pages.add(page)
      })
    setNear((current) =>
      current?.rows === activities &&
      current.pages.size === pages.size &&
      [...pages].every((page) => current.pages.has(page))
        ? current
        : { rows: activities, pages },
    )
  })
  useLayoutEffect(() => update.current())
  const drawn = loaded > 0
  useEffect(() => {
    const element = list.current
    const root = element ? scrollContainer(element) : null
    if (!root) return undefined
    let frame = 0
    const schedule = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => update.current())
    }
    root.addEventListener('scroll', schedule, { passive: true })
    const observer =
      typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(schedule)
    observer?.observe(root)
    return () => {
      cancelAnimationFrame(frame)
      root.removeEventListener('scroll', schedule)
      observer?.disconnect()
    }
  }, [list, drawn, update])
  return useMemo(() => ({ size, shown, heights }), [size, shown, heights])
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
  checkedIds,
  footer,
}: GroupedActivityListProps) {
  // Paging starts again when filtering changes which rows there are; an edit to a row doesn't.
  // The count and ends stand in for the full list of ids, which would cost a join per render.
  const rows = `${activities.length}\0${activities[0]?.id}\0${activities.at(-1)?.id}`
  const { count, more, reach } = useActivityPages(activities.length, 0, rows)
  const table = useRef<HTMLDivElement>(null)
  const [focusedPage, setFocusedPage] = useState<number>()
  const pages = useActivityWindow(table, activities, count, focusedPage, selectedId)
  const loadedRows = useMemo(() => activities.slice(0, count), [activities, count])
  const focusRow = (target: EventTarget) => {
    const id =
      target instanceof Element
        ? target.closest<HTMLElement>('[data-activity-id]')?.dataset.activityId
        : undefined
    const index = id == null ? -1 : activities.findIndex((activity) => activity.id === id)
    if (index < 0) return
    // By page, so stepping within one renders nothing.
    setFocusedPage(Math.floor(index / activityPageSize))
    reach(index)
  }

  useLayoutEffect(() => {
    const element = table.current
    if (!showColumns || !element || !activities.length) return undefined
    const canvas = document.createElement('canvas')
    const context = canvas.getContext('2d')
    const header = element.querySelector<HTMLElement>('.financial-activity-header')
    const row = element.querySelector<HTMLElement>('.financial-activity-row-columns')
    const amount = row?.querySelector<HTMLElement>('.financial-activity-amount')
    if (!context || !header || !row || !amount) return undefined
    // Built from parts: WebKit and Chromium leave the computed `font` shorthand empty, which
    // would silently measure everything in the canvas default of 10px.
    const fontOf = (style: CSSStyleDeclaration, size = style.fontSize) =>
      `${style.fontStyle} ${style.fontWeight} ${size} ${style.fontFamily}`
    const bodyStyle = getComputedStyle(row)
    const fonts: Record<TextWeight, string> = {
      header: fontOf(getComputedStyle(header)),
      body: fontOf(bodyStyle),
      strong: fontOf(getComputedStyle(amount)),
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
    const widthsKey = Object.values(fonts).join('\0') + `\0${referenceIso}`
    const cachedWidths = gridWidths.get(sizing)
    const widths =
      cachedWidths?.key === widthsKey
        ? cachedWidths.widths
        : activityGridWidths(sizing, referenceIso, measure)
    gridWidths.set(sizing, { key: widthsKey, widths })
    // Amounts and dates never clip: when the ledger is narrower than its columns, the place
    // narrows, then the description, then places go, then who it's for, categories, and accounts;
    // each stays in the transaction inspector. Room to spare spaces the columns evenly.
    const fit = () => {
      // Short of room, the place gives way first, down to its minimum, so the description keeps
      // its usual width as long as it can.
      const short = rowWidth(widths, []) - element.clientWidth
      const sized = widths.map((value, index) =>
        index === locationIndex && short > 0
          ? Math.max(Math.min(value, minimumLocationWidth), value - short)
          : value,
      )
      const floors = sized.map((value, index) =>
        index === descriptionIndex ? Math.min(value, minimumDescriptionWidth) : value,
      )
      const hidden = optionalColumns.reduce<number[]>(
        (dropped, index) =>
          rowWidth(floors, dropped) > element.clientWidth ? [...dropped, index] : dropped,
        [],
      )
      // The description takes back what room there is, up to its usual width; anything more is
      // shared evenly, so the columns space out rather than the description stretching. Cells
      // align to their start, so each column's share reads as the gap after it.
      let spare = element.clientWidth - rowWidth(floors, hidden)
      const usual = sized[descriptionIndex]
      const floor = floors[descriptionIndex]
      const regained =
        usual != null && floor != null ? Math.max(0, Math.min(spare, usual - floor)) : 0
      const columns = floors.map((value, index) =>
        index === descriptionIndex ? value + regained : value,
      )
      spare -= regained
      const visible = columns.flatMap((value, index) => (hidden.includes(index) ? [] : [value]))
      const share = spare > 0 ? Math.floor(spare / visible.length) : 0
      // The date is never hidden, so the first visible column is always there; the mark's
      // 32px column follows it.
      const [first = 0, ...rest] = visible.map((value) => value + share)
      element.dataset.hiddenColumns = hidden
        .flatMap((index) => activityColumns[index]?.id ?? [])
        .join(' ')
      element.style.setProperty(
        '--activity-grid-columns',
        `${first}px 32px ${rest.map((value) => `${value}px`).join(' ')}`,
      )
      element.style.setProperty('--activity-grid-width', `${rowWidth(floors, hidden)}px`)
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
          activities={loadedRows}
          pages={pages}
          referenceIso={referenceIso}
          showColumns
          accountTypes={accountTypes}
          accountInstitutions={accountInstitutions}
          onFilter={onFilter}
          onToggleOther={onToggleOther}
          onOpen={onOpen}
          selectedId={selectedId}
          checkedIds={checkedIds}
        />
      ) : (
        groupActivitiesByDate(loadedRows, referenceIso).map((group, index) => {
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
      {showColumns && footer ? <div className="financial-activity-footer">{footer}</div> : null}
    </>
  )
  return showColumns ? (
    <div
      className="financial-activity-table"
      ref={table}
      onFocus={(event) => focusRow(event.target)}
    >
      {content}
    </div>
  ) : (
    content
  )
}
