import type { LivelinePoint } from 'liveline'

import { dayKey } from './dates'
import { chartPeriod } from './graph-preferences'
import type { ChartChanges } from './schema'

const MAX_LIVE_POINTS = 7 * 24 * 60
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/

const minute = (time: number) => Math.floor(time / 60) * 60

export function appendLiveChartPoint(
  points: LivelinePoint[],
  value: number,
  time: number,
): LivelinePoint[] {
  if (!Number.isFinite(value) || !Number.isFinite(time)) return points
  const last = points.at(-1)
  const next = { time: minute(time), value }
  if (last && (next.time < last.time || (next.time === last.time && next.value === last.value)))
    return points
  const updated =
    last && minute(last.time) === next.time ? [...points.slice(0, -1), next] : [...points, next]
  return updated.slice(-MAX_LIVE_POINTS)
}

export function upsertLiveChartPoint(
  points: LivelinePoint[],
  value: number,
  time: number,
): LivelinePoint[] {
  if (!Number.isFinite(value) || !Number.isFinite(time)) return points
  const next = { time: minute(time), value }
  const existing = points.findIndex((point) => minute(point.time) === next.time)
  if (existing >= 0) {
    if (points[existing].value === next.value) return points
    return [...points.slice(0, existing), next, ...points.slice(existing + 1)]
  }
  return [...points, next].toSorted((left, right) => left.time - right.time).slice(-MAX_LIVE_POINTS)
}

/**
 * A line split at its last observed day: what was observed, and the estimate after it, which
 * starts at that last observation so the two lines meet. Without `estimatedAfter`, all of it
 * is observed.
 */
export function splitAtEstimate(points: LivelinePoint[], estimatedAfter: string | undefined) {
  const last = estimatedAfter
    ? points.findLastIndex(
        (point) => point.time <= Date.parse(`${estimatedAfter}T23:59:59Z`) / 1_000,
      )
    : -1
  if (last < 0 || last === points.length - 1) return { observed: points, estimated: [] }
  return { observed: points.slice(0, last + 1), estimated: points.slice(last) }
}

export function chartPointAtOrAfter(points: LivelinePoint[], time: number) {
  return points.find((point) => point.time >= time) ?? points.at(-1)
}

/**
 * A value chart's change over a window, as Rust read it from the same line: saved, or at the
 * live points when the market stream is on. Nothing changed when Rust has no reading.
 */
export function chartRangeChange(changes: ChartChanges, series: string, windowSeconds: number) {
  return (
    changes.find((chart) => chart.series === series)?.windows[chartPeriod(windowSeconds)] ?? {
      change: 0,
      percent: 0,
    }
  )
}

export function chartPointsFromStartDate(points: LivelinePoint[], startDate?: string) {
  if (!startDate || !DATE_PATTERN.test(startDate)) return points
  const startTime = Date.parse(`${startDate}T00:00:00Z`) / 1_000
  return Number.isFinite(startTime) ? points.filter(({ time }) => time >= startTime) : points
}

export function buildLiveChartData(
  history: Array<{ date: string; value: number }>,
  live: LivelinePoint[],
  currentValue: number,
  now: number,
): LivelinePoint[] {
  const liveWithCurrent = live.length ? live : appendLiveChartPoint([], currentValue, now)
  const liveDates = new Set(liveWithCurrent.map((point) => dayKey(point.time * 1_000)))
  const historical = history.flatMap((point): LivelinePoint[] => {
    if (!DATE_PATTERN.test(point.date) || !Number.isFinite(point.value)) return []
    const time = Date.parse(`${point.date}T12:00:00Z`) / 1_000
    return Number.isFinite(time) && !liveDates.has(point.date) ? [{ time, value: point.value }] : []
  })
  const merged = [...historical, ...liveWithCurrent]
    .filter((point) => Number.isFinite(point.time) && Number.isFinite(point.value))
    .toSorted((left, right) => left.time - right.time)

  return [...new Map(merged.map((point) => [point.time, point])).values()]
}
