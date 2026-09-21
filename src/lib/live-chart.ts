import type { LivelinePoint } from 'liveline'

import type { MarketSnapshots } from './schema'

const MAX_LIVE_POINTS = 7 * 24 * 60
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/

const minute = (time: number) => Math.floor(time / 60) * 60
const utcDate = (time: number) => new Date(time * 1_000).toISOString().slice(0, 10)

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

export function chartPointAtOrAfter(points: LivelinePoint[], time: number) {
  return points.find((point) => point.time >= time) ?? points.at(-1)
}

export function chartRangeChange(points: LivelinePoint[], windowSeconds: number, now: number) {
  const ordered = points
    .filter(({ time, value }) => Number.isFinite(time) && Number.isFinite(value))
    .toSorted((left, right) => left.time - right.time)
  const first = ordered[0]
  const last = ordered.at(-1)
  if (!first || !last || !Number.isFinite(now)) return { change: 0, percent: 0 }

  const boundary = windowSeconds ? now - windowSeconds : first.time
  const afterIndex = ordered.findIndex(({ time }) => time >= boundary)
  const after = afterIndex < 0 ? last : ordered[afterIndex]
  const before = afterIndex > 0 ? ordered[afterIndex - 1] : after
  const elapsed = after.time - before.time
  const progress = elapsed ? (boundary - before.time) / elapsed : 0
  const baseline =
    boundary <= first.time
      ? first.value
      : boundary >= last.time
        ? last.value
        : before.value + (after.value - before.value) * progress
  const change = last.value - baseline

  return {
    change,
    percent: baseline ? (change / Math.abs(baseline)) * 100 : 0,
  }
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
  const liveDates = new Set(liveWithCurrent.map((point) => utcDate(point.time)))
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

export function marketClosePoint(
  points: LivelinePoint[],
  session: MarketSnapshots['session'] | undefined,
  closeTime: number | undefined,
) {
  if (!session || session === 'Regular market' || closeTime === undefined) return undefined
  return points.find((point) => point.time === closeTime)
}
