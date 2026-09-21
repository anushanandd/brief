import { Liveline, type LivelinePoint, type LivelineSeries } from 'liveline'
import { useLayoutEffect, useState, useSyncExternalStore } from 'react'

import { type ChartEventGroup, chartEventGroupLabel } from '../lib/chart-events'
import { formatCompactCurrency, formatCurrency, formatActivityName } from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { chartPointAtOrAfter, chartPointsFromStartDate } from '../lib/live-chart'
import { RangeSelector } from './ui'

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

// Canvas needs resolved colors; CSS custom-property strings are not canvas colors.
export function useChartColors() {
  const [colors, setColors] = useState({
    value: 'transparent',
    deposits: 'transparent',
    benchmark: 'transparent',
    positive: 'transparent',
    negative: 'transparent',
  })
  useLayoutEffect(() => {
    const style = getComputedStyle(document.documentElement)
    setColors({
      value: style.getPropertyValue('--chart-primary').trim(),
      deposits: style.getPropertyValue('--chart-secondary').trim(),
      benchmark: style.getPropertyValue('--chart-benchmark').trim(),
      positive: style.getPropertyValue('--positive').trim(),
      negative: style.getPropertyValue('--negative').trim(),
    })
  }, [])
  return colors
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

function renderRing(items: DonutSegment[], radius: number, width: number) {
  const ringTotal = items.reduce((sum, { value }) => sum + value, 0)
  let offset = 0
  return items.map((segment, index) => {
    const percent = ringTotal ? (segment.value / ringTotal) * 100 : 0
    const start = offset
    offset += percent
    const detail = `${segment.name}: ${formatCurrency(segment.value)} (${percent.toFixed(1)}%)`
    return (
      <circle
        key={`${segment.name}:${index}`}
        aria-label={detail}
        className="donut-segment"
        cx="50"
        cy="50"
        fill="none"
        pathLength="100"
        r={radius}
        stroke={segment.color}
        strokeDasharray={`${Math.max(0, percent - 0.7)} ${100 - Math.max(0, percent - 0.7)}`}
        strokeDashoffset={-start}
        strokeWidth={width}
        tabIndex={0}
      />
    )
  })
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
  const legendSegments = visibleSegments.toSorted((left, right) => right.value - left.value)
  const description = visibleSegments
    .map(({ name, value }) => `${name} ${formatCurrency(value)}`)
    .join(', ')

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
      <ol className="donut-legend" aria-label={`${label}, all values`}>
        {legendSegments.map((segment) => {
          const content = (
            <>
              <i style={{ background: segment.color }} />
              <span>{segment.name}</span>
              <strong aria-label={formatCurrency(segment.value)}>
                {formatCompactCurrency(segment.value)}
              </strong>
            </>
          )
          return (
            <li className="donut-legend-item" key={segment.name}>
              {segment.details?.length ? (
                <details className="donut-details">
                  <summary className="donut-legend-row">{content}</summary>
                  <ul>
                    {segment.details.map((detail) => (
                      <li className="donut-legend-row" key={detail.name}>
                        <i style={{ background: detail.color }} />
                        <span>{detail.name}</span>
                        <strong aria-label={formatCurrency(detail.value)}>
                          {formatCompactCurrency(detail.value)}
                        </strong>
                      </li>
                    ))}
                  </ul>
                </details>
              ) : (
                <div className="donut-legend-row">{content}</div>
              )}
            </li>
          )
        })}
      </ol>
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
  const colors = useChartColors()
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
        color: marker.direction === 'expense' ? colors.negative : colors.positive,
        label: `${dateFormatter.format(new Date(time * 1_000))} · ${formatActivityName(marker.merchant)} · ${formatCurrency(marker.amount)}`,
      })
    }
    dayPoints.push({ time: dayStart + (DAY_SECONDS * 2) / 3, value })
    previousValue = value
    return dayPoints
  })
  const firstTime = points[0]?.time ?? 0
  const lastTime = points.at(-1)?.time ?? firstTime
  const windowSeconds = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.935)
  const value = points.at(-1)?.value ?? 0
  const lineColor = colors.value

  return (
    <div
      className="chart-container spending-line-chart"
      role="group"
      aria-label={`${label}, cumulative card activity over time with ${markers.length} transaction markers`}
    >
      <Liveline
        data={points}
        value={value}
        valueTime={lastTime}
        endTime={lastTime}
        continuous={false}
        fill={false}
        grid
        theme="dark"
        color={lineColor}
        window={windowSeconds}
        badge
        badgeVariant="minimal"
        currentLine={false}
        markers={markers}
        emptyText="No spending in this period"
        scrub={false}
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
    <RangeSelector
      label="Chart range"
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

export function PerformanceChartControls({
  value,
  onValueChange,
  netDeposits,
  benchmark,
}: {
  value: number
  onValueChange: (seconds: number) => void
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
}) {
  return (
    <div className="performance-chart-controls">
      <ul className="performance-chart-key" aria-label="Chart key">
        <li>
          <span className="performance-chart-key-line" data-series="value" aria-hidden="true" />
          <span>Value</span>
        </li>
        {netDeposits?.length ? (
          <li>
            <span
              className="performance-chart-key-line"
              data-series="deposits"
              aria-hidden="true"
            />
            <span>Net deposits</span>
          </li>
        ) : null}
        {benchmark?.length ? (
          <li>
            <span
              className="performance-chart-key-line"
              data-series="benchmark"
              aria-hidden="true"
            />
            <span>VOO</span>
          </li>
        ) : null}
      </ul>
      <ChartRangeSelector value={value} onValueChange={onValueChange} />
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
  sessionBoundary,
}: {
  data: LivelinePoint[]
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
  value: number
  events?: ChartEventGroup[]
  referenceIso: string
  startDate?: string
  selectedWindow: number
  sessionBoundary?: LivelinePoint
}) {
  const reduceMotion = useReducedMotion()
  const colors = useChartColors()
  const chartData = chartPointsFromStartDate(data, startDate)
  const plottedValue = chartData.at(-1)?.value ?? value
  const minValueRange = Math.max(1, Math.abs(value) * 0.05)
  const firstTime = chartData[0]?.time ?? 0
  const lastTime = chartData.at(-1)?.time ?? firstTime
  const historyWindow = Math.max(DAY_SECONDS + 1, (lastTime - firstTime) / 0.935)
  const visibleSeries = (points: LivelinePoint[]) => {
    const startedPoints = chartPointsFromStartDate(points, startDate)
    return selectedWindow === 0
      ? startedPoints.filter((point) => point.time >= firstTime)
      : startedPoints
  }
  const windowSeconds = selectedWindow || historyWindow
  const eventColors = { in: colors.positive, out: colors.negative }
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
        label: chartEventGroupLabel(group, referenceIso),
      },
    ]
  })
  const markers = [
    ...(sessionBoundary
      ? [
          {
            id: 'market-close',
            time: sessionBoundary.time,
            value: sessionBoundary.value,
            color: colors.deposits,
            label: 'Regular market close',
          },
        ]
      : []),
    ...eventMarkers,
  ]
  const series: LivelineSeries[] = [
    {
      id: 'Value',
      data: chartData,
      value: plottedValue,
      color: colors.value,
    },
    ...(netDeposits?.length
      ? [
          {
            id: 'Net deposits',
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
            id: 'S&P 500',
            data: visibleSeries(benchmark),
            value: benchmark.at(-1)?.value ?? 0,
            color: colors.benchmark,
            dash: [1, 5],
          },
        ]
      : []),
  ]
  const comparisonSeries = series.length > 1 ? series : undefined

  return (
    <div
      className="chart-container performance-chart live-performance-chart"
      role="group"
      tabIndex={0}
      aria-keyshortcuts="W M Q A"
      aria-label={`Account value over time${benchmark?.length ? ' compared with VOO' : ''}${events.length ? ` with ${events.length} key event markers` : ''}${sessionBoundary ? ', with the regular-market close marked' : ''}`}
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
        theme="dark"
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
        formatTime={(time) => formatLiveTime(time, windowSeconds)}
        pulse={false}
        continuous
        momentum={false}
        lerpSpeed={reduceMotion ? 1 : 0.25}
        lineWidth={1.75}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
    </div>
  )
}
