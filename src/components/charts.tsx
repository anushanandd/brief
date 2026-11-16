import { Liveline, type LivelineHandle, type LivelinePoint, type LivelineSeries } from 'liveline'
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type RefObject,
  type SetStateAction,
} from 'react'

import { graphRangeActions, useScrollBounds } from '../hooks/use-graph-window-shortcuts'
import { useLatest } from '../hooks/use-latest'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ActivityItem } from '../lib/activity'
import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import { addDays } from '../lib/dates'
import {
  formatActivityName,
  formatCalendarDate,
  formatCompactCurrency,
  formatCurrency,
  formatDay,
  formatInstant,
  formatShortDay,
} from '../lib/format'
import {
  graphWindows,
  scrollPeriodDays,
  sessionGraphEnd,
  setGraphEnd,
  shiftedGraphEnd,
  useGraphEnd,
  type GraphScroll,
} from '../lib/graph-preferences'
import {
  chartDayTime,
  chartPointAtOrAfter,
  chartPointsFromStartDate,
  splitAtEstimate,
} from '../lib/live-chart'
import type { SpendingBar } from '../lib/spending'
import { ActivityMark } from './activity-list'
import { X } from './icons'
import { Button, ChartRangeSelect } from './ui'
import { Dialog, dialogAutofocus } from './ui/dialog'
import { usePresence } from './ui/presence'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const DAY_SECONDS = 24 * 60 * 60
const WEEK_SECONDS = 7 * DAY_SECONDS
const YEAR_SECONDS = 365 * DAY_SECONDS
const monthOf = (month: string) => formatCalendarDate(`${month}-01`, { month: 'short' })
const spendingCategoryAmount = (name: string, value: number) =>
  formatCurrency(name === 'Credits' ? Math.abs(value) : value)

// Canvas needs resolved colors; CSS custom-property strings are not canvas colors.
export function useChartColors() {
  const [colors, setColors] = useState({
    value: 'transparent',
    deposits: 'transparent',
    benchmark: 'transparent',
    positive: 'transparent',
    negative: 'transparent',
    grid: 'transparent',
    label: 'transparent',
  })
  useLayoutEffect(() => {
    const style = getComputedStyle(document.documentElement)
    setColors({
      value: style.getPropertyValue('--chart-primary').trim(),
      deposits: style.getPropertyValue('--chart-secondary').trim(),
      benchmark: style.getPropertyValue('--chart-benchmark').trim(),
      positive: style.getPropertyValue('--positive').trim(),
      negative: style.getPropertyValue('--negative').trim(),
      grid: style.getPropertyValue('--border-subtle').trim(),
      label: style.getPropertyValue('--text-tertiary').trim(),
    })
  }, [])
  return { ...colors, theme: 'dark' as const }
}

/**
 * Axis labels read in the user's own time zone. Session boundaries are the exception: the
 * holding chart marks them in Eastern time and says so.
 */
export function formatLiveTime(time: number, windowSeconds: number) {
  const date = new Date(time * 1_000)
  if (windowSeconds <= DAY_SECONDS)
    return formatInstant(date, { hour: 'numeric', minute: '2-digit' })
  if (windowSeconds <= WEEK_SECONDS) return formatInstant(date, { weekday: 'short' })
  // A year or more repeats months, so its labels and crosshair name the year, kept short
  // enough that the axis doesn't drop labels to fit them.
  const day = formatInstant(date, { month: 'short', day: 'numeric' })
  if (windowSeconds < YEAR_SECONDS) return day
  // The digits alone: newer locale data writes a two-digit year with its own apostrophe.
  const year = formatInstant(date, { year: '2-digit' }).replace(/\D/g, '')
  return `${day} ’${year}`
}

const barLabel = ({ from, to }: SpendingBar) =>
  from === to ? formatShortDay(from) : `${formatShortDay(from)}–${formatShortDay(to)}`

export function SpendingBarChart({
  data,
  label,
  categories,
  activities,
}: {
  data: SpendingBar[]
  label: string
  categories: Array<{ name: string; color: string }>
  activities: Map<string, ActivityItem>
}) {
  const extents = data.map(({ categories: items }) => ({
    positive: items.reduce((sum, { value }) => sum + Math.max(0, value), 0),
    negative: items.reduce((sum, { value }) => sum + Math.min(0, value), 0),
  }))
  const maximum = Math.max(0, ...extents.map(({ positive }) => positive))
  const minimum = Math.min(0, ...extents.map(({ negative }) => negative))
  const span = maximum - minimum || 1
  const zero = (maximum / span) * 100
  const dense = data.length > 12
  const labelEvery = Math.max(1, Math.ceil(data.length / 8))
  const colors = new Map(categories.map(({ name, color }) => [name, color]))
  const axis = Array.from({ length: 5 }, (_, index) => maximum - (span * index) / 4)

  return (
    <div className="analytics-plot spending-bar-chart">
      <div className="analytics-bars" role="group" aria-label={`${label} spending by category`}>
        <div className="analytics-bar-axis" aria-hidden="true">
          {axis.map((value, index) => (
            <span key={index}>{formatCompactCurrency(value)}</span>
          ))}
        </div>
        <div className="analytics-bars-scroll">
          <div
            className={`analytics-bars-grid${dense ? ' is-dense' : ''}`}
            style={{ gridTemplateColumns: `repeat(${data.length}, minmax(32px, 1fr))` }}
          >
            {data.map((bar, index) => {
              let positiveTotal = 0
              let negativeTotal = 0
              const description = bar.categories
                .map(({ name, value }) => `${name} ${spendingCategoryAmount(name, value)}`)
                .join(', ')
              return (
                <div
                  className="monthly-bar-column analytics-bar-column"
                  key={bar.from}
                  role="group"
                  aria-label={`${barLabel(bar)}: ${formatCurrency(bar.value)}${description ? `. ${description}` : ''}`}
                >
                  <div className="analytics-bar-track">
                    <span className="analytics-bar-zero" style={{ top: `${zero}%` }} />
                    {bar.categories.map((category) => {
                      const positive = category.value >= 0
                      const top = positive
                        ? ((maximum - (positiveTotal += category.value)) / span) * 100
                        : ((maximum + Math.abs(negativeTotal)) / span) * 100
                      if (!positive) negativeTotal += category.value
                      const date = barLabel(bar)
                      return (
                        <Tooltip key={category.name}>
                          <TooltipTrigger
                            render={
                              <span
                                role="img"
                                tabIndex={0}
                                className={`analytics-bar-fill spending-bar-fill${positive ? '' : ' is-negative'}`}
                                aria-label={`${category.name}, ${date}: ${spendingCategoryAmount(category.name, category.value)}`}
                                style={{
                                  top: `${top}%`,
                                  height: `${(Math.abs(category.value) / span) * 100}%`,
                                  background: colors.get(category.name),
                                }}
                              />
                            }
                          />
                          <TooltipContent
                            className="treemap-tooltip sankey-tooltip spending-bar-tooltip"
                            sideOffset={12}
                          >
                            <div className="treemap-tooltip-heading">
                              <strong>
                                {category.name} · {date}
                              </strong>
                              <span>{spendingCategoryAmount(category.name, category.value)}</span>
                            </div>
                            <div className="treemap-tooltip-composition">
                              <ul>
                                {category.activities.slice(0, 8).flatMap((item) => {
                                  const activity = activities.get(item.id)
                                  if (!activity) return []
                                  const title = formatActivityName(activity.title)
                                  return [
                                    <li key={item.id}>
                                      <ActivityMark activity={activity} />
                                      <span>
                                        {title}
                                        {activity.subcategory ? (
                                          <small className="spending-bar-tooltip-label">
                                            {' · '}
                                            {activity.subcategory}
                                          </small>
                                        ) : null}
                                      </span>
                                      <strong>{formatCurrency(item.value)}</strong>
                                    </li>,
                                  ]
                                })}
                              </ul>
                            </div>
                          </TooltipContent>
                        </Tooltip>
                      )
                    })}
                  </div>
                  <small>
                    {!dense || index % labelEvery === 0 || index === data.length - 1
                      ? barLabel(bar)
                      : null}
                  </small>
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}

export function MonthlyBarChart({
  data,
  label,
}: {
  data: Array<{ month: string; value: number }>
  label: string
}) {
  const maximum = Math.max(0, ...data.map(({ value }) => value))
  const description = data
    .map(({ month, value }) => `${monthOf(month)} ${formatCurrency(value)}`)
    .join(', ')

  return (
    <div className="monthly-bar-chart" role="img" aria-label={`${label}: ${description}`}>
      <div className="monthly-bar-axis" aria-hidden="true">
        {[maximum, maximum / 2, 0].map((value, index) => (
          <span key={index}>{formatCompactCurrency(value)}</span>
        ))}
      </div>
      {data.map(({ month, value }, index) => (
        <div className="monthly-bar-column" key={month} aria-hidden="true">
          <strong>{formatCompactCurrency(value)}</strong>
          <div className="monthly-bar-track">
            <span
              className={index === data.length - 1 ? 'current' : undefined}
              style={{ height: maximum && value ? `${Math.max(3, (value / maximum) * 100)}%` : 0 }}
            />
          </div>
          <small>{monthOf(month)}</small>
        </div>
      ))}
    </div>
  )
}

export function ChartRangeSelector({
  value,
  onValueChange,
}: {
  value: number
  onValueChange: (seconds: number) => void
}) {
  return (
    <ChartRangeSelect
      label="Chart range"
      command="graph.range."
      options={graphWindows.map(({ label, settingsLabel, secs }) => ({
        label,
        accessibleLabel: settingsLabel,
        value: secs,
      }))}
      value={value}
      onValueChange={onValueChange}
    />
  )
}

/** A calendar day at noon UTC, the time every saved day is drawn at. */
const noonOf = (date: string) => Date.parse(`${date}T12:00:00Z`) / 1_000

/**
 * Rust's cash days as steps: each day holds its balance until the next change, and the last
 * holds through the horizon, so the line never slopes between two bills.
 */
export function cashSteps(
  days: ReadonlyArray<{ date: string; value: number }>,
  through: string,
): LivelinePoint[] {
  return days.flatMap(({ date, value }, index) => {
    const next = days[index + 1]?.date ?? through
    const point = { time: noonOf(date), value }
    return next > date ? [point, { time: noonOf(next) - 1, value }] : [point]
  })
}

/**
 * Cash ahead: the expected balance over the forecast's days, between the balance if every bill
 * is its largest and every deposit its smallest and the reverse, with the low point marked.
 */
export function CashAheadChart({
  days,
  through,
  lowest,
  onExpand,
}: {
  days: ReadonlyArray<{ date: string; expected: number; low: number; high: number }>
  /** The horizon's last day. */
  through: string
  lowest?: { date: string; expected: number }
  /** Opens the chart larger, from a click on it or ↵ on it. */
  onExpand?: () => void
}) {
  const colors = useChartColors()
  const expected = useMemo(
    () =>
      cashSteps(
        days.map(({ date, expected: value }) => ({ date, value })),
        through,
      ),
    [days, through],
  )
  const series = useMemo(
    (): LivelineSeries[] => [
      {
        id: 'Expected',
        data: expected,
        value: expected.at(-1)?.value ?? 0,
        color: colors.value,
      },
      ...(['low', 'high'] as const).map((bound) => {
        const data = cashSteps(
          days.map(({ date, [bound]: value }) => ({ date, value })),
          through,
        )
        return {
          id: bound === 'low' ? 'Low' : 'High',
          data,
          value: data.at(-1)?.value ?? 0,
          color: colors.deposits,
          dash: [1, 5],
        }
      }),
    ],
    [days, expected, through, colors.value, colors.deposits],
  )
  const first = expected[0]?.time ?? 0
  const end = noonOf(through)
  const last = expected.at(-1)?.value ?? 0
  const windowSeconds = Math.max(DAY_SECONDS + 1, end - first)
  return (
    <div
      className={`chart-container performance-chart live-performance-chart${onExpand ? ' is-expandable' : ''}`}
      role="group"
      tabIndex={0}
      aria-keyshortcuts={onExpand ? 'Enter' : undefined}
      aria-label={`Expected cash through ${formatDay(through)}, between its low and high range${lowest ? `, lowest ${formatDay(lowest.date)}` : ''}`}
      onClick={expandOnClick(onExpand)}
      onKeyDown={expandOnEnter(onExpand)}
    >
      <Liveline
        className="liveline-chart-canvas"
        data={expected}
        value={last}
        endTime={end}
        series={series}
        theme={colors.theme}
        color={colors.value}
        window={windowSeconds}
        badge
        badgeVariant="minimal"
        currentLine={false}
        grid
        minValueRange={Math.max(1, Math.abs(last) * 0.05)}
        markers={
          lowest
            ? [
                {
                  id: `lowest:${lowest.date}`,
                  time: noonOf(lowest.date),
                  value: lowest.expected,
                  color: colors.benchmark,
                  label: `Lowest · ${formatDay(lowest.date)}`,
                },
              ]
            : []
        }
        scrub
        formatValue={formatCurrency}
        formatTime={(time) => formatLiveTime(time, windowSeconds)}
        pulse={false}
        continuous={false}
        momentum={false}
        lerpSpeed={0.25}
        lineWidth={1.75}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
    </div>
  )
}

/**
 * A value's history beside its deposits and the S&P 500 on Liveline's canvas: a range change
 * zooms the window there, without re-rendering React each frame. Hovering reads every line at
 * that day. With `scroll`, the window ends on the page's end day, and scrolling over the chart
 * moves it through time; clicking it, or ↵ on it, opens the page's expanded chart.
 */
export function PerformanceChart({
  data,
  netDeposits,
  benchmark,
  value,
  events = [],
  referenceIso,
  startDate,
  selectedWindow,
  observedOn = [],
  estimatedAfter,
  scroll,
}: {
  data: LivelinePoint[]
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
  value: number
  events?: ChartEventGroup[]
  referenceIso: string
  startDate?: string
  selectedWindow: number
  /** Days the value was observed (a statement price or an entry), each marked with a dot. */
  observedOn?: string[]
  /** The last observed day of a value estimated since; the line after it is dashed. */
  estimatedAfter?: string
  scroll?: GraphScroll
}) {
  const colors = useChartColors()
  const expand = useChartExpansion()
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const [graphEnd] = useGraphEnd()
  const pastEnd = scroll ? graphEnd : undefined
  const chart = useRef<LivelineHandle>(null)
  useChartPan(container, selectedWindow, scroll, { chart })
  // Saved days are drawn at noon, so a chart scrolled back ends on its end day's point. Liveline
  // draws every line to that end, so the lines and markers are built once from all the history.
  const endTime = pastEnd ? chartDayTime(pastEnd) : undefined
  const allData = useMemo(() => chartPointsFromStartDate(data, startDate), [data, startDate])
  const plottedValue = allData.at(-1)?.value ?? value
  const minValueRange = Math.max(1, Math.abs(value) * 0.05)
  const firstTime = allData[0]?.time ?? 0
  const lastTime = allData.at(-1)?.time ?? firstTime
  const historyWindow = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.935)
  const windowSeconds = selectedWindow || historyWindow
  const eventColors = { in: colors.positive, out: colors.negative, none: colors.benchmark }
  const referenceTime = Date.parse(referenceIso) / 1_000
  const eventMarkers = events.flatMap((group) => {
    const eventTime =
      group.date === referenceIso.slice(0, 10) && Number.isFinite(referenceTime)
        ? referenceTime
        : Date.parse(`${group.date}T12:00:00Z`) / 1_000
    if (!Number.isFinite(eventTime) || eventTime < firstTime - DAY_SECONDS) return []
    const point = chartPointAtOrAfter(allData, eventTime)
    // The group's color is its leading event's.
    const [lead] = group.events
    if (!point || !lead) return []
    return [
      {
        id: group.id,
        time: point.time,
        value: point.value,
        color: eventColors[lead.direction],
        label: chartEventGroupLabel(group),
      },
    ]
  })
  // Observed values are dots on the line; between them the line may be an estimate.
  const observationMarkers = observedOn.flatMap((date) => {
    const target = Date.parse(`${date}T12:00:00Z`) / 1_000
    const point = chartPointAtOrAfter(allData, target - DAY_SECONDS / 2)
    return point && Math.abs(point.time - target) < DAY_SECONDS
      ? [
          {
            id: `observed:${date}`,
            time: point.time,
            value: point.value,
            color: colors.value,
            label: `Observed · ${formatDay(date)}`,
          },
        ]
      : []
  })
  const markers = [...observationMarkers, ...eventMarkers]
  const series = useMemo((): LivelineSeries[] => {
    // After the last observation the value is an estimate, drawn as its own dashed line.
    const { observed, estimated } = splitAtEstimate(allData, estimatedAfter)
    const visibleSeries = (points: LivelinePoint[]) => {
      const startedPoints = chartPointsFromStartDate(points, startDate)
      return selectedWindow === 0
        ? startedPoints.filter((point) => point.time >= firstTime)
        : startedPoints
    }
    const layer = (
      id: string,
      points: LivelinePoint[] | undefined,
      color: string,
      dash: number[],
    ): LivelineSeries[] => {
      const shown = points?.length ? visibleSeries(points) : []
      return shown.length ? [{ id, data: shown, value: shown.at(-1)!.value, color, dash }] : []
    }
    return [
      {
        id: 'Value',
        data: observed,
        value: observed.at(-1)?.value ?? plottedValue,
        color: colors.value,
      },
      ...(estimated.length >= 2
        ? [
            {
              id: 'Estimated',
              label: 'Estimated · S&P 500',
              data: estimated,
              value: plottedValue,
              color: colors.value,
              dash: [4, 4],
            },
          ]
        : []),
      ...layer('Net deposits', netDeposits, colors.deposits, [1, 5]),
      ...layer('S&P 500', benchmark, colors.benchmark, [1, 5]),
    ]
  }, [
    allData,
    estimatedAfter,
    startDate,
    selectedWindow,
    firstTime,
    plottedValue,
    netDeposits,
    benchmark,
    colors.value,
    colors.deposits,
    colors.benchmark,
  ])
  const comparisonSeries = series.length > 1 ? series : undefined
  const lineNames = series
    .filter(({ id }) => id !== 'Value' && id !== 'Estimated')
    .map(({ id }) => id)
  const formatTime = (time: number) => formatLiveTime(time, windowSeconds)

  return (
    <div
      ref={setContainer}
      className={`chart-container performance-chart live-performance-chart${expand ? ' is-expandable' : ''}`}
      data-range-focus-region
      role="group"
      tabIndex={0}
      aria-keyshortcuts={`D W M Q Y A${scroll ? ' [ ] }' : ''}${expand ? ' Enter' : ''}`}
      aria-label={`Account value over time${lineNames.length ? ` beside ${lineNames.join(', ')}` : ''}${events.length ? `, with ${events.length} key event markers` : ''}`}
      onClick={(event) => {
        if (event.target instanceof HTMLCanvasElement) expand?.()
      }}
      onKeyDown={(event) => {
        if (event.key !== 'Enter' || event.target !== event.currentTarget || !expand) return
        event.preventDefault()
        expand()
      }}
    >
      <span className="sr-only" role="status">
        Chart range: {graphWindows.find(({ secs }) => secs === selectedWindow)?.settingsLabel}
      </span>
      {netDeposits?.length ? (
        <span className="sr-only">Net deposits shows the starting value plus net cash flows.</span>
      ) : null}
      <Liveline
        ref={chart}
        className="liveline-chart-canvas"
        data={allData}
        value={plottedValue}
        endTime={endTime}
        series={comparisonSeries}
        theme={colors.theme}
        color={colors.value}
        window={windowSeconds}
        badge
        badgeVariant="minimal"
        currentLine={false}
        grid
        minValueRange={minValueRange}
        markers={markers}
        scrub
        formatValue={formatCurrency}
        formatTime={formatTime}
        pulse={false}
        // Settled charts stop drawing; new values, range changes and scrubbing still redraw.
        // The shortest range is a week, where the clock moves the axis about a pixel in 13 min.
        // A range change zooms the window on the canvas; Liveline honors reduced motion itself.
        continuous={false}
        momentum={false}
        lerpSpeed={0.25}
        lineWidth={1.75}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
    </div>
  )
}

// A wheel's line is about this many pixels.
const LINE_PIXELS = 16
// A pause this long ends a scroll, and the chart eases onto the nearest day.
const SETTLE_MS = 160
const EASE_MS = 200

/**
 * A chart's own time, for one that doesn't draw calendar time, like a holding's sessions with
 * closures removed: it moves `span` across its window, and `end` and `day` place an end day on it.
 */
export type ChartTimeline = {
  span: number
  toDisplay: (time: number) => number
  toActual: (time: number) => number
  /** When a window ending on `day` ends; today's end when undefined. */
  end: (day: string | undefined) => number
  /** The end day nearest a time; undefined for today. */
  day: (time: number) => string | undefined
}

const calendarTimeline = (span: number, today: string): ChartTimeline => ({
  span,
  toDisplay: (time) => time,
  toActual: (time) => time,
  end: (day) => (day ? chartDayTime(day) : Date.now() / 1_000),
  day: (time) => addDays(today, Math.round((time - chartDayTime(today)) / DAY_SECONDS)),
})

/**
 * Scrolling over a chart as time moved: down, or a swipe right, goes back. Each wheel event moves
 * `chart` to where the scroll is, drawn outside React; once the scroll pauses it eases onto the
 * nearest day, the page's end day moves there, and the chart returns to its own end once the page
 * has caught up. The page hears nothing while the scroll moves: drawing it costs more than a frame.
 * A chart with its own `timeline` moves along it, and settles where that places the nearest day.
 * The scroll survives the page drawing again mid-gesture, as it does when new figures or prices
 * arrive. Whether it's moving is returned.
 */
export function useChartPan(
  element: HTMLElement | null,
  seconds: number,
  scroll: GraphScroll | undefined,
  { chart, timeline }: { chart?: RefObject<LivelineHandle | null>; timeline?: ChartTimeline } = {},
) {
  const [panning, setPanning] = useState(false)
  // After the commit that carries the scroll's last end day, so the chart never draws an older one.
  useLayoutEffect(() => {
    if (!panning) chart?.current?.view(undefined, false)
  }, [panning, chart])
  const current = useLatest({ seconds, scroll, timeline, chart })
  useEffect(() => {
    if (!element) return undefined
    let width = 0
    const resize =
      typeof ResizeObserver === 'function'
        ? new ResizeObserver(([entry]) => (width = entry?.contentRect.width ?? 0))
        : undefined
    resize?.observe(element)
    // The drawn end in actual time, so it holds while the chart's timeline changes under it; the
    // page's end day moves once it settles.
    let active = false
    let time = 0
    let easing = 0
    let settle: ReturnType<typeof setTimeout> | undefined
    const axis = () =>
      current.current.timeline ??
      calendarTimeline(current.current.seconds, current.current.scroll?.today ?? '')
    const nearest = () => {
      const { seconds: span, scroll: bounds } = current.current
      const day = bounds && axis().day(time)
      return day && shiftedGraphEnd(day, 0, span, bounds.today, bounds.first)
    }
    const move = (pixels: number) => {
      const { seconds: span, scroll: bounds } = current.current
      const days = scrollPeriodDays(span)
      const fraction = pixels / Math.max(1, (width ||= element.clientWidth))
      if (!bounds?.first || !days) return
      const { span: across, toDisplay, toActual, end } = axis()
      const earliest = toDisplay(end(addDays(bounds.first, days)))
      const at = Math.min(
        toDisplay(end(undefined)),
        Math.max(earliest, toDisplay(time) + fraction * across),
      )
      time = toActual(at)
      current.current.chart?.current?.view(at)
    }
    const finish = () => {
      const target = nearest()
      const { toDisplay, toActual, end } = axis()
      const from = toDisplay(time)
      const to = toDisplay(end(target))
      const startedAt = Date.now()
      const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
      const ease = () => {
        const progress = reduced ? 1 : Math.min(1, (Date.now() - startedAt) / EASE_MS)
        const at = from + (to - from) * (1 - (1 - progress) ** 3)
        time = toActual(at)
        current.current.chart?.current?.view(at)
        if (progress < 1) {
          easing = requestAnimationFrame(ease)
          return
        }
        easing = 0
        active = false
        setGraphEnd(target)
        setPanning(false)
      }
      ease()
    }
    const wheel = (event: WheelEvent) => {
      const { seconds: span, scroll: bounds } = current.current
      const days = scrollPeriodDays(span)
      // Momentum from a page scroll can't be cancelled, and isn't the chart's to take.
      if (!event.cancelable || !bounds?.first || !days) return
      if (chartDayTime(addDays(bounds.first, days)) >= Date.now() / 1_000) return
      const across = Math.abs(event.deltaX) > Math.abs(event.deltaY)
      const delta =
        (across ? event.deltaX : -event.deltaY) * (event.deltaMode === 1 ? LINE_PIXELS : 1)
      if (!delta) return
      event.preventDefault()
      if (!active) {
        active = true
        time = axis().end(sessionGraphEnd())
        setPanning(true)
      }
      cancelAnimationFrame(easing)
      easing = 0
      move(delta)
      clearTimeout(settle)
      settle = setTimeout(finish, SETTLE_MS)
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => {
      element.removeEventListener('wheel', wheel)
      resize?.disconnect()
      cancelAnimationFrame(easing)
      clearTimeout(settle)
      if (active) setGraphEnd(nearest())
      setPanning(false)
    }
  }, [element, current])
  return panning
}

/**
 * Scrolling over a chart of periods as whole periods: down, or a swipe right, goes back one each
 * time the scroll passes half the chart's width. `onStep` hears -1 or 1.
 */
export function useWheelSteps(element: HTMLElement | null, onStep: (direction: -1 | 1) => void) {
  const step = useLatest(onStep)
  useEffect(() => {
    if (!element) return undefined
    let moved = 0
    const wheel = (event: WheelEvent) => {
      const across = Math.abs(event.deltaX) > Math.abs(event.deltaY)
      const pixels =
        (across ? event.deltaX : -event.deltaY) * (event.deltaMode === 1 ? LINE_PIXELS : 1)
      if (!pixels) return
      event.preventDefault()
      moved += pixels
      const page = Math.max(1, element.clientWidth / 2)
      if (Math.abs(moved) < page) return
      step.current(moved < 0 ? -1 : 1)
      moved = 0
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => element.removeEventListener('wheel', wheel)
  }, [element, step])
}

/** A click on a chart that opens it larger, unless it landed on a link or control inside it. */
export const expandOnClick = (expand: (() => void) | undefined) =>
  expand
    ? (event: MouseEvent) => {
        if (
          event.target instanceof Element &&
          !event.target.closest('a, button, input, select, [role="combobox"]')
        )
          expand()
      }
    : undefined

/** ↵ on a focused chart opens it larger. */
export const expandOnEnter = (expand: (() => void) | undefined) =>
  expand
    ? (event: KeyboardEvent) => {
        if (event.key !== 'Enter' || event.target !== event.currentTarget) return
        event.preventDefault()
        expand()
      }
    : undefined

/** The days a chart scrolled back in time shows, and the way back; nothing while it ends today. */
export function ChartSpan({ seconds }: { seconds: number }) {
  const [end] = useGraphEnd()
  const days = scrollPeriodDays(seconds)
  if (!end || !days) return null
  return (
    <span className="chart-span">
      <span>
        {formatShortDay(addDays(end, 1 - days))} – {formatDay(end)}
      </span>
      <Button
        size="compact"
        variant="ghost"
        aria-keyshortcuts="}"
        onClick={() => setGraphEnd(undefined)}
      >
        Today
      </Button>
    </span>
  )
}

const ChartExpansionContext = createContext<(() => void) | undefined>(undefined)

/** Opens the expanded chart this one sits in; undefined inside the expanded window itself. */
export const useChartExpansion = () => useContext(ChartExpansionContext)

/**
 * A chart card's content, and the same content in a large window that E, a click on the chart,
 * or ↵ on it opens. Both follow the page's range and end day, so scrolling one moves the other,
 * and the range and scroll keys work in the window too. A chart with its own period keys passes
 * them as `actions` instead. A page's second chart is `named`, so E stays with the first and the
 * palette opens it by name.
 */
export function ExpandableChart({
  title,
  seconds,
  setSeconds,
  scroll,
  actions: own,
  named,
  children,
}: {
  title: string
  seconds?: number
  setSeconds?: Dispatch<SetStateAction<number>>
  scroll?: GraphScroll
  actions?: readonly AppAction[]
  named?: string
  /** The content, inline or in the window; inline, `expand` opens the window. */
  children: (expanded: boolean, expand?: () => void) => ReactNode
}) {
  const [open, setOpen] = useState(false)
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const present = usePresence(open, dialog)
  const titleId = useId()
  const scrolled = Boolean(useGraphEnd()[0])
  const bounds = useScrollBounds(scroll)
  const expand = useCallback(() => setOpen(true), [])
  const close = useCallback(() => setOpen(false), [])
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: named ? `graph.expand.${named}` : 'graph.expand',
          title: named ? `Expand ${named} chart` : 'Expand chart',
          search: 'chart graph expand enlarge larger window',
          group: 'View',
          scope: 'page',
          bindings: named ? undefined : [{ sequence: ['e'], label: 'E' }],
          run: expand,
        },
      ],
      [expand, named],
    ),
  )
  // The window's keys are the page's own under ids of their own: the shell keeps one action per
  // id, and the page's, registered again as they change, would otherwise replace these.
  const actions = useMemo(
    () =>
      [
        ...(setSeconds && seconds !== undefined
          ? graphRangeActions(setSeconds, seconds, scrolled, bounds)
          : []),
        ...(own ?? []),
      ].map((action) => ({ ...action, id: `${action.id}.expanded` })),
    [setSeconds, seconds, scrolled, bounds, own],
  )
  return (
    <>
      <ChartExpansionContext.Provider value={expand}>
        {children(false, expand)}
      </ChartExpansionContext.Provider>
      <Dialog
        ref={setDialog}
        open={open}
        onClose={close}
        popups
        actions={actions}
        className="chart-dialog"
        aria-labelledby={titleId}
      >
        {present ? (
          <div className="chart-dialog-panel">
            <header className="chart-dialog-header">
              {/* The content already shows its name, so the title is for assistive tech; it
                  takes focus on open, so no control starts out ringed. */}
              <h2 id={titleId} className="sr-only" tabIndex={-1} {...dialogAutofocus}>
                {title}
              </h2>
              <Button
                icon={X}
                size="icon-compact"
                variant="ghost"
                aria-label="Close chart"
                onClick={close}
              />
            </header>
            <div className="chart-dialog-body">{children(true)}</div>
          </div>
        ) : null}
      </Dialog>
    </>
  )
}
