import { Liveline, type LivelinePoint, type LivelineSeries } from 'liveline'
import { useState, useSyncExternalStore } from 'react'

import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import { formatCompactCurrency, formatCurrency } from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { chartPointAtOrAfter } from '../lib/live-chart'

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
  const [activeSegment, setActiveSegment] = useState<DonutSegment>()
  const description = visibleSegments
    .map(({ name, value }) => `${name} ${formatCurrency(value)}`)
    .join(', ')
  const renderRing = (items: DonutSegment[], radius: number, width: number) => {
    const total = items.reduce((sum, { value }) => sum + value, 0)
    let offset = 0
    return items.map((segment, index) => {
      const percent = total ? (segment.value / total) * 100 : 0
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
    <div className="donut-chart-visual">
      <svg viewBox="0 0 100 100" role="img" aria-label={`${label}: ${description || 'No data'}`}>
        <g transform="rotate(-90 50 50)">{renderRing(visibleSegments, 38, 20)}</g>
      </svg>
      <span className="donut-chart-center" aria-hidden="true">
        <strong>{centerValue}</strong>
        <small>{centerLabel}</small>
      </span>
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

export function PerformanceChart({
  data,
  netDeposits,
  benchmark,
  value,
  events = [],
  referenceIso,
  selectedWindow,
  onWindowChange,
}: {
  data: LivelinePoint[]
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
  value: number
  events?: ChartEventGroup[]
  referenceIso: string
  selectedWindow: number
  onWindowChange: (seconds: number) => void
}) {
  const colors = { value: '#e6e6e6', deposits: '#b0b0b1', benchmark: '#56c2ff' }
  const reduceMotion = useReducedMotion()
  const chartData = data
  const firstTime = chartData[0]?.time ?? 0
  const lastTime = chartData.at(-1)?.time ?? firstTime
  // Keep the all-time window usable even when the series has just one point.
  const allTimeWindow = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.985)
  const visibleSeries = (points: LivelinePoint[]) =>
    selectedWindow === 0 ? points.filter((point) => point.time >= firstTime) : points
  const windows = graphWindows.map((window) => ({
    ...window,
    secs: window.secs || allTimeWindow,
  }))
  const windowSeconds = selectedWindow || allTimeWindow
  const eventColors = {
    income: colors.value,
    expense: colors.deposits,
    transfer: colors.deposits,
    sale: colors.benchmark,
    'market-move': colors.benchmark,
    'stock-move': colors.benchmark,
  }
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
        color: eventColors[group.events[0].kind],
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
      aria-label={`Account value over time${benchmark?.length ? ' compared with VOO' : ''}${events.length ? ` with ${events.length} key event markers` : ''}`}
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
