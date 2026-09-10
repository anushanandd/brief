import type { LivelinePoint } from 'liveline'

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

export function chartPointAtOrAfter(points: LivelinePoint[], time: number) {
  return points.find((point) => point.time >= time) ?? points.at(-1)
}

export function buildLiveChartData(
  history: Array<{ date: string; value: number }>,
  live: LivelinePoint[],
  currentValue: number,
  now: number,
): LivelinePoint[] {
  const liveWithCurrent = appendLiveChartPoint(live, currentValue, now)
  const liveDates = new Set(liveWithCurrent.map((point) => utcDate(point.time)))
  const historical = history.flatMap((point): LivelinePoint[] => {
    if (!DATE_PATTERN.test(point.date) || !Number.isFinite(point.value)) return []
    const time = Date.parse(`${point.date}T12:00:00Z`) / 1_000
    return Number.isFinite(time) && !liveDates.has(point.date) ? [{ time, value: point.value }] : []
  })
  const merged = [...historical, ...liveWithCurrent]
    .filter((point) => Number.isFinite(point.time) && Number.isFinite(point.value))
    .toSorted((left, right) => left.time - right.time)

  const deduplicated = [...new Map(merged.map((point) => [point.time, point])).values()]
  if (deduplicated.length !== 1) return deduplicated
  return [{ time: deduplicated[0].time - 60, value: deduplicated[0].value }, ...deduplicated]
}
