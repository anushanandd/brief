import { Liveline, type LivelinePoint, type LivelineSeries } from 'liveline'
import { useState, useSyncExternalStore } from 'react'

import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import { formatCompactCurrency, formatCurrency } from '../lib/format'
import { graphWindowForKey, graphWindows } from '../lib/graph-preferences'
import { chartPointAtOrAfter, chartPointsFromStartDate } from '../lib/live-chart'

const DAY_SECONDS = 24 * 60 * 60
const WEEK_SECONDS = 7 * DAY_SECONDS
const reducedMotionQuery = '(prefers-reduced-motion: reduce)'
const timeFormatter = new Intl.DateTimeFormat('en-US', {
  hour: 'numeric',
  minute: '2-digit',
  timeZone: 'America/Los_Angeles',
})
const weekdayFormatter = new Intl.DateTimeFormat('en-US', {
  weekday: 'short',
  timeZone: 'America/Los_Angeles',
})
const dateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  timeZone: 'America/Los_Angeles',
})
const monthFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  timeZone: 'UTC',
})

function useReducedMotion() {
  return useSyncExternalStore(
    (onChange) => {
      const media = window.matchMedia(reducedMotionQuery)
      media.addEventListener('change', onChange)
      return () => media.removeEventListener('change', onChange)
    },
    () => window.matchMedia(reducedMotionQuery).matches,
    () => false,
  )
}

function formatLiveTime(time: number, windowSeconds: number) {
  const date = new Date(time * 1_000)
  if (windowSeconds <= DAY_SECONDS) return timeFormatter.format(date)
  return windowSeconds <= WEEK_SECONDS ? weekdayFormatter.format(date) : dateFormatter.format(date)
}

export type DonutSegment = {
  name: string
  value: number
  color: string
  details?: DonutSegment[]
}

export function DonutChart({
  segments,
  label,
  centerValue,
  centerLabel,
}: {
  segments: DonutSegment[]
  label: string
  centerValue: string
  centerLabel: string
}) {
  const visibleSegments = segments.filter(({ value }) => value > 0)
  const legendSegments = visibleSegments
    .toSorted((left, right) => right.value - left.value)
    .slice(0, 5)
  const [activeSegment, setActiveSegment] = useState<DonutSegment>()
  const description = visibleSegments
    .map(({ name, value }) => `${name} ${formatCurrency(value)}`)
    .join(', ')
  const renderRing = (items: DonutSegment[], radius: number, width: number) => {
    const ringTotal = items.reduce((sum, { value }) => sum + value, 0)
    let offset = 0
    return items.map((segment, index) => {
      const percent = ringTotal ? (segment.value / ringTotal) * 100 : 0
      const start = offset
      offset += percent
      const interactive = !!segment.details?.length
      return (
        <circle
          className={interactive ? 'donut-segment interactive' : 'donut-segment'}
          cx="50"
          cy="50"
          fill="none"
          key={`${segment.name}:${index}`}
          pathLength="100"
          r={radius}
          stroke={segment.color}
          strokeDasharray={`${Math.max(0, percent - 0.7)} ${100 - Math.max(0, percent - 0.7)}`}
          strokeDashoffset={-start}
          strokeWidth={width}
          tabIndex={interactive ? 0 : undefined}
          aria-label={interactive ? `${segment.name} composition` : undefined}
          onBlur={() => setActiveSegment(undefined)}
          onFocus={() => interactive && setActiveSegment(segment)}
          onPointerEnter={() => interactive && setActiveSegment(segment)}
          onPointerLeave={() => setActiveSegment(undefined)}
        />
      )
    })
  }

  return (
    <div className="donut-chart-layout">
      <div className="donut-chart-visual">
        <svg viewBox="0 0 100 100" role="img" aria-label={`${label}: ${description || 'No data'}`}>
          <g transform="rotate(-90 50 50)">{renderRing(visibleSegments, 38, 20)}</g>
        </svg>
        <span className="donut-chart-center" aria-hidden="true">
          <strong>{centerValue}</strong>
          <small>{centerLabel}</small>
        </span>
      </div>
      <ol className="donut-legend" aria-label={`${label}, largest values`}>
        {legendSegments.map((segment) => (
          <li className="donut-legend-item" key={segment.name}>
            <i style={{ background: segment.color }} />
            <span>{segment.name}</span>
            <strong>{formatCompactCurrency(segment.value)}</strong>
          </li>
        ))}
      </ol>
      {activeSegment?.details?.length ? (
        <div className="donut-composition-tooltip" role="tooltip">
          <strong>{activeSegment.name}</strong>
          {activeSegment.details.map((detail) => (
            <span key={detail.name}>
              <i style={{ background: detail.color }} />
              <small>{detail.name}</small>
              <b>{formatCompactCurrency(detail.value)}</b>
            </span>
          ))}
        </div>
      ) : null}
    </div>
  )
}

export function SpendingLineChart({
  data,
  activityMarkers,
  startingValue,
  label,
}: {
  data: Array<{ date: string; value: number }>
  activityMarkers: Array<{
    id: string
    date: string
    sequence: number
    count: number
    value: number
    direction: 'expense' | 'credit'
    merchant: string
    amount: number
  }>
  startingValue: number
  label: string
}) {
  const reduceMotion = useReducedMotion()
  const markersByDate = new Map<string, typeof activityMarkers>()
  for (const marker of activityMarkers) {
    const markers = markersByDate.get(marker.date) ?? []
    markers.push(marker)
    markersByDate.set(marker.date, markers)
  }
  const markers: Array<{
    id: string
    time: number
    value: number
    color: string
    label: string
  }> = []
  let previousValue = startingValue
  const points = data.flatMap(({ date, value }) => {
    const dayStart = Date.parse(`${date}T00:00:00Z`) / 1_000
    if (!Number.isFinite(dayStart)) return []
    const dailyMarkers = markersByDate.get(date) ?? []
    if (!dailyMarkers.length) {
      previousValue = value
      return [{ time: dayStart + DAY_SECONDS / 2, value }]
    }
    const dayPoints: LivelinePoint[] = [{ time: dayStart + DAY_SECONDS / 3, value: previousValue }]
    for (const marker of dailyMarkers) {
      const time =
        dayStart + DAY_SECONDS * (1 / 3 + ((marker.sequence + 1) / (marker.count + 1)) * (1 / 3))
      dayPoints.push({ time, value: marker.value })
      markers.push({
        id: marker.id,
        time,
        value: marker.value,
        color: marker.direction === 'expense' ? 'var(--negative)' : 'var(--positive)',
        label: `${dateFormatter.format(new Date(time * 1_000))} · ${marker.merchant} · ${formatCurrency(marker.amount)}`,
      })
    }
    dayPoints.push({ time: dayStart + (DAY_SECONDS * 2) / 3, value })
    previousValue = value
    return dayPoints
  })
  const firstTime = points[0]?.time ?? 0
  const lastTime = points.at(-1)?.time ?? firstTime
  const windowSeconds = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.985)
  const value = points.at(-1)?.value ?? 0
  const lineColor = '#e6e6e6'
  const series: LivelineSeries[] = [
    {
      id: 'Statement balance',
      data: points,
      value,
      color: lineColor,
    },
  ]

  return (
    <div
      className="chart-container spending-line-chart"
      role="group"
      aria-label={`${label}, cumulative card activity over time with ${markers.length} transaction markers`}
    >
      <Liveline
        data={points}
        value={value}
        series={series}
        theme="dark"
        color={lineColor}
        window={windowSeconds}
        badgeVariant="minimal"
        currentLine={false}
        markers={markers}
        emptyText="No spending in this period"
        tooltipY={-10}
        tooltipOutline={false}
        formatValue={formatCurrency}
        formatTime={(time) => dateFormatter.format(new Date(time * 1_000))}
        pulse={false}
        momentum={false}
        lerpSpeed={reduceMotion ? 1 : 0.4}
        lineWidth={2.25}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
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

export function PerformanceChart({
  data,
  netDeposits,
  benchmark,
  value,
  events = [],
  referenceIso,
  startDate,
  selectedWindow,
  onWindowChange,
}: {
  data: LivelinePoint[]
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
  value: number
  events?: ChartEventGroup[]
  referenceIso: string
  startDate?: string
  selectedWindow: number
  onWindowChange: (seconds: number) => void
}) {
  const colors = { value: '#e6e6e6', deposits: '#b0b0b1', benchmark: '#56c2ff' }
  const reduceMotion = useReducedMotion()
  const chartData = chartPointsFromStartDate(data, startDate)
  const firstTime = chartData[0]?.time ?? 0
  const lastTime = chartData.at(-1)?.time ?? firstTime
  // Keep the all-time window usable even when the series has just one point.
  const allTimeWindow = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.985)
  const visibleSeries = (points: LivelinePoint[]) => {
    const startedPoints = chartPointsFromStartDate(points, startDate)
    return selectedWindow === 0
      ? startedPoints.filter((point) => point.time >= firstTime)
      : startedPoints
  }
  const windows = graphWindows.map((window) => ({
    ...window,
    secs: window.secs || allTimeWindow,
  }))
  const windowSeconds = selectedWindow || allTimeWindow
  const eventColors = { in: 'var(--positive)', out: 'var(--negative)' }
  const referenceTime = Date.parse(referenceIso) / 1_000
  const markers = events.flatMap((group) => {
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
        label: chartEventGroupLabel(group, referenceIso),
      },
    ]
  })
  const series: LivelineSeries[] = [
    {
      id: 'Value',
      data: chartData,
      value,
      color: colors.value,
    },
    ...(netDeposits?.length
      ? [
          {
            id: 'Starting value + net flows',
            data: visibleSeries(netDeposits),
            value: netDeposits.at(-1)?.value ?? 0,
            color: colors.deposits,
            dash: [1, 5],
          },
        ]
      : []),
    ...(benchmark?.length
      ? [
          {
            id: 'S&P 500 (VOO)',
            data: visibleSeries(benchmark),
            value: benchmark.at(-1)?.value ?? 0,
            color: colors.benchmark,
            dash: [1, 5],
          },
        ]
      : []),
  ]

  return (
    <div
      className="chart-container performance-chart live-performance-chart"
      role="group"
      tabIndex={0}
      aria-keyshortcuts="W M Q A"
      aria-label={`Account value over time${benchmark?.length ? ' compared with VOO' : ''}${events.length ? ` with ${events.length} key event markers` : ''}`}
      onKeyDown={(event) => {
        if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
        const graphWindow = graphWindowForKey(event.key)
        if (graphWindow == null) return
        event.preventDefault()
        onWindowChange(graphWindow)
      }}
    >
      <Liveline
        data={chartData}
        value={value}
        series={series}
        theme="dark"
        color={colors.value}
        window={windowSeconds}
        windows={windows}
        onWindowChange={(seconds) => onWindowChange(seconds === allTimeWindow ? 0 : seconds)}
        windowStyle="text"
        seriesToggleCompact
        badgeVariant="minimal"
        currentLine={false}
        markers={markers}
        tooltipY={-10}
        tooltipOutline={false}
        formatValue={formatCurrency}
        formatTime={(time) => formatLiveTime(time, windowSeconds)}
        pulse={false}
        continuous
        momentum={!reduceMotion}
        lerpSpeed={reduceMotion ? 1 : 0.4}
        lineWidth={2.25}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
    </div>
  )
}
