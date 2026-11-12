import { Liveline, type LivelinePoint, type LivelineSeries } from 'liveline'
import { useLayoutEffect, useMemo, useState } from 'react'

import type { ActivityItem } from '../lib/activity'
import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import {
  formatActivityName,
  formatCalendarDate,
  formatCompactCurrency,
  formatCurrency,
  formatDay,
  formatInstant,
  formatShortDay,
} from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { chartPointAtOrAfter, chartPointsFromStartDate, splitAtEstimate } from '../lib/live-chart'
import type { SpendingBar } from '../lib/spending'
import { ActivityMark } from './activity-list'
import { ChartRangeSelect } from './ui'
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
}: {
  days: ReadonlyArray<{ date: string; expected: number; low: number; high: number }>
  /** The horizon's last day. */
  through: string
  lowest?: { date: string; expected: number }
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
      className="chart-container performance-chart live-performance-chart"
      role="group"
      tabIndex={0}
      aria-label={`Expected cash through ${formatDay(through)}, between its low and high range${lowest ? `, lowest ${formatDay(lowest.date)}` : ''}`}
    >
      <Liveline
        className="liveline-chart-canvas"
        data={expected}
        value={last}
        valueTime={end}
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
 * that day.
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
}) {
  const colors = useChartColors()
  // Liveline animates toward each new data set, so the lines keep their identity until a point
  // changes.
  const chartData = useMemo(() => chartPointsFromStartDate(data, startDate), [data, startDate])
  const plottedValue = chartData.at(-1)?.value ?? value
  const minValueRange = Math.max(1, Math.abs(value) * 0.05)
  const firstTime = chartData[0]?.time ?? 0
  const lastTime = chartData.at(-1)?.time ?? firstTime
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
    const point = chartPointAtOrAfter(chartData, eventTime)
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
    const point = chartPointAtOrAfter(chartData, target - DAY_SECONDS / 2)
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
    const { observed, estimated } = splitAtEstimate(chartData, estimatedAfter)
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
    ): LivelineSeries[] =>
      points?.length
        ? [{ id, data: visibleSeries(points), value: points.at(-1)!.value, color, dash }]
        : []
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
    chartData,
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
      className="chart-container performance-chart live-performance-chart"
      data-range-focus-region
      role="group"
      tabIndex={0}
      aria-keyshortcuts="D W M Q Y A"
      aria-label={`Account value over time${lineNames.length ? ` beside ${lineNames.join(', ')}` : ''}${events.length ? `, with ${events.length} key event markers` : ''}`}
    >
      <span className="sr-only" role="status">
        Chart range: {graphWindows.find(({ secs }) => secs === selectedWindow)?.settingsLabel}
      </span>
      {netDeposits?.length ? (
        <span className="sr-only">Net deposits shows the starting value plus net cash flows.</span>
      ) : null}
      <Liveline
        className="liveline-chart-canvas"
        data={chartData}
        value={plottedValue}
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
