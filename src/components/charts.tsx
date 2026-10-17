import type { LivelinePoint } from 'liveline'
import { useTheme } from 'next-themes'
import { useLayoutEffect, useState } from 'react'
import {
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceDot,
  ResponsiveContainer,
  Tooltip as ChartTooltip,
  XAxis,
  YAxis,
} from 'recharts'

import type { ActivityItem } from '../lib/activity'
import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import { formatActivityName, formatCompactCurrency, formatCurrency, formatDay } from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { chartPointAtOrAfter, chartPointsFromStartDate, splitAtEstimate } from '../lib/live-chart'
import type { SpendingBar } from '../lib/spending'
import { ActivityMark } from './activity-list'
import { ChartRangeSelect } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const DAY_SECONDS = 24 * 60 * 60
const WEEK_SECONDS = 7 * DAY_SECONDS
const YEAR_SECONDS = 365 * DAY_SECONDS
// Axis labels read in the user's own time zone. Session boundaries are the exception: the
// holding chart marks them in Eastern time and says so.
const timeFormatter = new Intl.DateTimeFormat('en-US', {
  hour: 'numeric',
  minute: '2-digit',
})
const weekdayFormatter = new Intl.DateTimeFormat('en-US', {
  weekday: 'short',
})
const dateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
})
const shortYearFormatter = new Intl.DateTimeFormat('en-US', { year: '2-digit' })
const monthFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  timeZone: 'UTC',
})
const spendingCategoryAmount = (name: string, value: number) =>
  formatCurrency(name === 'Credits' ? Math.abs(value) : value)

// Canvas needs resolved colors; CSS custom-property strings are not canvas colors.
export function useChartColors() {
  const { resolvedTheme } = useTheme()
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
  }, [resolvedTheme])
  return { ...colors, theme: 'dark' as const }
}

export function formatLiveTime(time: number, windowSeconds: number) {
  const date = new Date(time * 1_000)
  if (windowSeconds <= DAY_SECONDS) return timeFormatter.format(date)
  if (windowSeconds <= WEEK_SECONDS) return weekdayFormatter.format(date)
  // A year or more repeats months, so its labels and crosshair name the year, kept short
  // enough that the axis doesn't drop labels to fit them.
  return windowSeconds < YEAR_SECONDS
    ? dateFormatter.format(date)
    : `${dateFormatter.format(date)} ’${shortYearFormatter.format(date)}`
}

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
  const barLabel = ({ from, to }: (typeof data)[number]) => {
    const start = dateFormatter.format(new Date(`${from}T12:00:00Z`))
    const end = dateFormatter.format(new Date(`${to}T12:00:00Z`))
    return from === to ? start : `${start}–${end}`
  }

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
    .map(
      ({ month, value }) =>
        `${monthFormatter.format(new Date(`${month}-01T00:00:00Z`))} ${formatCurrency(value)}`,
    )
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
          <small>{monthFormatter.format(new Date(`${month}-01T00:00:00Z`))}</small>
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

/** One instant on the value chart, with each line's value there. */
type ValueRow = {
  time: number
  value?: number
  estimated?: number
  deposits?: number
  benchmark?: number
}
type ValueLine = Exclude<keyof ValueRow, 'time'>
const valueLines: ValueLine[] = ['value', 'estimated', 'deposits', 'benchmark']
const valueLineNames: Record<ValueLine, string> = {
  value: 'Value',
  estimated: 'Estimated',
  deposits: 'Net deposits',
  benchmark: 'S&P 500',
}

/**
 * The lines as rows by time, from the last point before `start` (so each line enters at the
 * left edge) through the end. Lines that skip a time are joined across it.
 */
export function valueRows(lines: Partial<Record<ValueLine, LivelinePoint[]>>, start: number) {
  const rows = new Map<number, ValueRow>()
  for (const line of valueLines) {
    const points = lines[line] ?? []
    const first = Math.max(
      0,
      points.findLastIndex((point) => point.time <= start),
    )
    for (const point of points.slice(first)) {
      const row = rows.get(point.time) ?? { time: point.time }
      row[line] = point.value
      rows.set(point.time, row)
    }
  }
  return [...rows.values()].toSorted((left, right) => left.time - right.time)
}

/** The value axis over what the window shows, at least `minimum` tall, with room around it. */
export function valueDomain(rows: ValueRow[], start: number, minimum: number): [number, number] {
  const values = rows
    .filter(({ time }) => time >= start)
    .flatMap(({ value, estimated, deposits, benchmark }) => [value, estimated, deposits, benchmark])
    .filter((value): value is number => value !== undefined)
  if (!values.length) return [0, 1]
  const low = Math.min(...values)
  const high = Math.max(...values)
  const middle = (low + high) / 2
  const half = Math.max(high - low, minimum) / 2
  return [middle - half * 1.15, middle + half * 1.15]
}

/**
 * A value's history beside its deposits and the S&P 500, drawn settled: the line appears
 * whole, and new values, range changes, and scrubbing redraw it without animation.
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
  const chartData = chartPointsFromStartDate(data, startDate)
  const firstTime = chartData[0]?.time ?? 0
  const lastTime = chartData.at(-1)?.time ?? firstTime
  // The whole history, or the chosen window ending at the latest value.
  const start = selectedWindow ? Math.max(firstTime, lastTime - selectedWindow) : firstTime
  const windowSeconds = selectedWindow || Math.max(DAY_SECONDS + 1, lastTime - firstTime)
  const eventColors = { in: colors.positive, out: colors.negative, none: colors.benchmark }
  const referenceTime = Date.parse(referenceIso) / 1_000
  const eventMarkers = events.flatMap((group) => {
    const eventTime =
      group.date === referenceIso.slice(0, 10) && Number.isFinite(referenceTime)
        ? referenceTime
        : Date.parse(`${group.date}T12:00:00Z`) / 1_000
    if (!Number.isFinite(eventTime) || eventTime < firstTime - DAY_SECONDS) return []
    const point = chartPointAtOrAfter(chartData, eventTime)
    if (!point) return []
    return [
      {
        id: group.id,
        time: point.time,
        value: point.value,
        color: eventColors[group.events[0].direction],
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
  // After the last observation the value is an estimate, drawn as its own dashed line.
  const { observed: observedData, estimated: estimatedData } = splitAtEstimate(
    chartData,
    estimatedAfter,
  )
  const markers = [...observationMarkers, ...eventMarkers].filter(
    ({ time }) => time >= start && time <= lastTime,
  )
  const startedSeries = (points?: LivelinePoint[]) =>
    points?.length ? chartPointsFromStartDate(points, startDate) : undefined
  const rows = valueRows(
    {
      value: observedData,
      ...(estimatedData.length >= 2 ? { estimated: estimatedData } : {}),
      ...(netDeposits?.length ? { deposits: startedSeries(netDeposits) } : {}),
      ...(benchmark?.length ? { benchmark: startedSeries(benchmark) } : {}),
    },
    start,
  )
  const domain = valueDomain(rows, start, Math.max(1, Math.abs(value) * 0.05))
  const lines: Array<{ line: ValueLine; color: string; dash?: string }> = [
    { line: 'value', color: colors.value },
    { line: 'estimated', color: colors.value, dash: '4 4' },
    { line: 'deposits', color: colors.deposits, dash: '1 5' },
    { line: 'benchmark', color: colors.benchmark, dash: '1 5' },
  ]
  const ticks = Array.from(
    { length: 4 },
    (_, index) => start + ((lastTime - start) * (index + 0.5)) / 4,
  )

  return (
    <div
      className="chart-container performance-chart value-chart"
      data-range-focus-region
      role="group"
      tabIndex={0}
      aria-keyshortcuts="D W M Q Y A"
      aria-label={`Account value over time${benchmark?.length ? ' compared with VOO' : ''}${events.length ? ` with ${events.length} key event markers` : ''}`}
    >
      <span className="sr-only" role="status">
        Chart range: {graphWindows.find(({ secs }) => secs === selectedWindow)?.settingsLabel}
      </span>
      {netDeposits?.length ? (
        <span className="sr-only">Net deposits shows the starting value plus net cash flows.</span>
      ) : null}
      <ResponsiveContainer width="100%" height="100%" minWidth={0}>
        <ComposedChart data={rows} margin={{ top: 8, right: 0, bottom: 0, left: 0 }}>
          <CartesianGrid vertical={false} stroke={colors.grid} />
          <XAxis
            dataKey="time"
            type="number"
            domain={[start, lastTime]}
            allowDataOverflow
            ticks={ticks}
            tickFormatter={(time: number) => formatLiveTime(time, windowSeconds)}
            tick={{ fill: colors.label, fontSize: 11 }}
            axisLine={false}
            tickLine={false}
          />
          <YAxis
            orientation="right"
            domain={domain}
            allowDataOverflow
            tickCount={4}
            tickFormatter={(amount: number) => formatCompactCurrency(amount)}
            tick={{ fill: colors.label, fontSize: 11 }}
            axisLine={false}
            tickLine={false}
            width={56}
          />
          <ChartTooltip
            isAnimationActive={false}
            cursor={{ stroke: colors.label, strokeDasharray: '2 3' }}
            content={({ active, payload, label }) => {
              const time = Number(label)
              if (!active || !payload.length || !Number.isFinite(time)) return null
              const marked = markers.filter((marker) => marker.time === time)
              return (
                <div className="tooltip-content treemap-tooltip value-chart-tooltip">
                  <strong>{formatLiveTime(time, windowSeconds)}</strong>
                  {payload
                    .filter((item) => item.value != null)
                    .map((item) => (
                      <div className="treemap-tooltip-heading" key={String(item.dataKey)}>
                        <span>{item.name}</span>
                        <strong>{formatCurrency(Number(item.value))}</strong>
                      </div>
                    ))}
                  {marked.map((marker) => (
                    <p key={marker.id}>{marker.label}</p>
                  ))}
                </div>
              )
            }}
          />
          {lines.map(({ line, color, dash }) =>
            rows.some((row) => row[line] !== undefined) ? (
              <Line
                key={line}
                dataKey={line}
                name={valueLineNames[line]}
                type="linear"
                stroke={color}
                strokeWidth={line === 'value' || line === 'estimated' ? 1.75 : 1.5}
                strokeDasharray={dash}
                strokeLinecap="round"
                dot={false}
                activeDot={line === 'value' || line === 'estimated' ? { r: 3 } : false}
                connectNulls
                isAnimationActive={false}
              />
            ) : null,
          )}
          {markers.map((marker) => (
            <ReferenceDot
              key={marker.id}
              x={marker.time}
              y={marker.value}
              r={3.5}
              fill={marker.color}
              stroke="none"
              aria-label={marker.label}
            />
          ))}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  )
}
