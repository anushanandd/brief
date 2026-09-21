import type { CandlePoint, LivelinePoint } from 'liveline'

import type { HoldingChartEvent, HoldingPriceHistory, PriceBar } from './schema'

export type SavedRangeChange = { change: number; percent: number; asOf: number }
export type HoldingPreviewHistory = HoldingPriceHistory & { savedComparison?: SavedRangeChange }

export interface HoldingChartState {
  savedComparison?: SavedRangeChange
  symbol?: string
  range?: number
  history?: HoldingPriceHistory
  bars: Array<PriceBar & { receivedAt: number }>
  quote?: Extract<HoldingChartEvent, { kind: 'quote' }>
  connection?: Extract<HoldingChartEvent, { kind: 'status' }>
  error?: string
  historyError?: string
}

export function reduceHoldingChart(
  state: HoldingChartState,
  event: HoldingChartEvent,
): HoldingChartState {
  switch (event.kind) {
    case 'history': {
      const verified = {
        ...state,
        history: event.history,
        bars: [],
        error: undefined,
        historyError: undefined,
      }
      const comparison = event.history.cached
        ? undefined
        : holdingRangeChange(verified, holdingChartPoints(verified), event.history.fetchedAt / 1000)
      return {
        ...state,
        savedComparison: event.history.cached
          ? state.savedComparison
          : comparison
            ? { ...comparison, asOf: event.history.fetchedAt }
            : undefined,
        history: event.history,
        historyError: event.warning ?? undefined,
        // Keep updates received during the download; replace older bars in full
        // so cancellations and split adjustments remove obsolete observations.
        bars: event.history.cached
          ? state.bars
          : state.bars.filter(
              (bar) =>
                (bar.receivedAt > event.startedAt || bar.endTime > event.history.end) &&
                bar.time >= Math.floor(event.history.fetchedAt / 1000 / 86400) * 86400,
            ),
      }
    }
    case 'bar': {
      const bars = state.bars.filter(
        (bar) => !(bar.time === event.bar.time && bar.feed === event.bar.feed),
      )
      bars.push({ ...event.bar, receivedAt: event.receivedAt })
      return { ...state, bars: bars.filter((bar) => bar.time >= event.receivedAt / 1000 - 86400) }
    }
    case 'quote':
      return !state.quote || event.time >= state.quote.time ? { ...state, quote: event } : state
    case 'status': {
      const next = {
        ...state,
        connection: event,
        error: undefined,
        quote: state.quote?.feed === event.feed ? state.quote : undefined,
      }
      if (
        event.status === 'closed' &&
        next.history &&
        !next.history.cached &&
        !next.savedComparison &&
        !next.historyError
      ) {
        const comparison = holdingRangeChange(
          next,
          holdingChartPoints(next),
          next.history.fetchedAt / 1000,
        )
        if (comparison) next.savedComparison = { ...comparison, asOf: next.history.fetchedAt }
      }
      return next
    }
    case 'invalidate':
      return {
        ...state,
        quote: undefined,
        savedComparison: undefined,
        historyError: 'Reconciling a provider correction…',
        bars: state.bars.filter(
          (bar) => bar.feed !== (event.feed === 'delayed_sip' ? 'sip' : event.feed),
        ),
      }
    case 'history-error':
      return { ...state, historyError: event.message }
    case 'error':
      return { ...state, error: event.message }
  }
  return state
}

export const holdingChartRanges = [
  { label: 'D', value: 86400, accessibleLabel: '1 day' },
  { label: 'W', value: 7 * 86400, accessibleLabel: '1 week' },
  { label: 'M', value: 30 * 86400, accessibleLabel: '1 month' },
  { label: 'Y', value: 365 * 86400, accessibleLabel: '1 year' },
  { label: 'A', value: 0, accessibleLabel: 'All available history' },
]

const holdingChartRangeKey = 'brief.holdingChartRange'

export function parseHoldingChartRange(value: unknown) {
  if ((typeof value !== 'number' && typeof value !== 'string') || value === '') return 86400
  const seconds = Number(value)
  return holdingChartRanges.find((range) => range.value === seconds)?.value ?? 86400
}

export function getDefaultHoldingChartRange() {
  return typeof window === 'undefined'
    ? 86400
    : parseHoldingChartRange(window.localStorage.getItem(holdingChartRangeKey))
}

export function saveDefaultHoldingChartRange(value: number) {
  window.localStorage.setItem(holdingChartRangeKey, String(parseHoldingChartRange(value)))
}

export function holdingChartTime(time: number, now: number, seconds = false) {
  const date = new Date(time * 1000)
  const sameDay = date.toDateString() === new Date(now * 1000).toDateString()
  return date.toLocaleString(undefined, {
    ...(!sameDay ? ({ month: 'short', day: 'numeric' } as const) : {}),
    ...(date.getFullYear() !== new Date(now * 1000).getFullYear()
      ? ({ year: 'numeric' } as const)
      : {}),
    hour: 'numeric',
    minute: '2-digit',
    ...(seconds ? ({ second: '2-digit' } as const) : {}),
  })
}

export function holdingChartBars(state: HoldingChartState): PriceBar[] {
  const history = state.history
  if (!history) return []
  const merged = new Map(history.bars.map((bar) => [`${bar.feed}:${bar.time}`, bar]))
  // Coarser views stay canonical; the live observation is a separate marker.
  if (history.resolution === 60 && !history.cached)
    for (const bar of state.bars) merged.set(`${bar.feed}:${bar.time}`, bar)
  return [...merged.values()].toSorted((a, b) => a.time - b.time || a.feed.localeCompare(b.feed))
}

export function holdingChartPoints(state: HoldingChartState): LivelinePoint[] {
  const history = state.history
  if (!history) return []
  const bars = holdingChartBars(state)
  const daily = history.resolution === 86400
  return bars.map((bar, index) => ({
    time: bar.endTime,
    value: bar.close,
    // Fully paginated history connects reported closes; streaming gaps and
    // failed verification remain breaks. No bars or executions are fabricated.
    breakBefore:
      index > 0 &&
      (bar.feed !== bars[index - 1].feed ||
        (bar.time - bars[index - 1].time > history.resolution * (daily ? 4 : 1.5) &&
          (!history.calendar?.length ||
            Boolean(state.historyError || state.error) ||
            bars[index - 1].time < history.start ||
            bar.endTime > history.end))),
  }))
}

export function visibleHoldingChartPoints(points: LivelinePoint[], range: number, now: number) {
  return points.filter((point) => point.time <= now && (!range || point.time >= now - range))
}

export function holdingChartEnd(state: HoldingChartState, points: LivelinePoint[], now: number) {
  const last = points.findLast((point) => point.time <= now)
  return state.history?.range === 86400 &&
    state.connection?.status === 'closed' &&
    last &&
    now - last.time <= 7 * 86400
    ? last.time
    : now
}

export function holdingRangeChange(state: HoldingChartState, points: LivelinePoint[], now: number) {
  const history = state.history
  if (!history) return undefined
  if (history.cached || state.historyError || state.error) {
    const saved = state.savedComparison
    return saved && saved.asOf === history.fetchedAt
      ? { change: saved.change, percent: saved.percent }
      : undefined
  }
  if (points.length < 2) return undefined
  const end = holdingChartEnd(state, points, now)
  const last = points.findLast((point) => point.time <= end)
  const boundary = end - history.range
  const first = history.range ? points.findLast((point) => point.time <= boundary) : points[0]
  if (
    !first ||
    !last ||
    first.time >= last.time ||
    first.value <= 0 ||
    (history.range && boundary - first.time > 7 * 86400) ||
    now - last.time > 7 * 86400
  )
    return undefined
  const change = last.value - first.value
  return { change, percent: (change / first.value) * 100 }
}

// Preview only canonical bars, never an old quote, connection, or live marker.
export function cachedHoldingPreview(
  history: HoldingPreviewHistory | undefined,
  symbol: string,
  range: number,
  now = Date.now(),
): HoldingChartState {
  const usable =
    history &&
    history.symbol === symbol &&
    history.range === range &&
    history.fetchedAt <= now &&
    Math.floor(history.fetchedAt / 86400000) === Math.floor(now / 86400000)
  return {
    symbol,
    range,
    bars: [],
    ...(usable
      ? { history: { ...history, cached: true }, savedComparison: history.savedComparison }
      : {}),
  }
}

// Calendar dates are NY trading dates. Intl supplies the historical DST offset;
// using noon avoids the midnight representation and DST transition ambiguity.
const easternHour = new Intl.DateTimeFormat('en-US', {
  timeZone: 'America/New_York',
  hour: 'numeric',
  hourCycle: 'h23',
})
export function holdingSessions(history: HoldingPriceHistory | undefined) {
  return (history?.calendar ?? []).flatMap((day) => {
    const noon = Date.parse(`${day.date}T12:00:00Z`) / 1000
    const midnight = noon - Number(easternHour.format(noon * 1000)) * 3600
    return [
      { start: midnight - 14400, end: midnight + 14400, label: 'Overnight', kind: 'overnight' },
      { start: midnight + 14400, end: midnight + day.open, label: 'Pre-market', kind: 'extended' },
      { start: midnight + day.open, end: midnight + day.close, label: 'Open', kind: 'regular' },
      { start: midnight + day.close, end: midnight + 72000, label: 'Close', kind: 'extended' },
    ]
  })
}

// Only display coordinates change. Range returns, stored bars and quote times
// keep their original timestamps. Unknown intervals never become closures.
export function holdingChartTimeline(
  history: HoldingPriceHistory | undefined,
  points: LivelinePoint[],
  end: number,
  range: number,
) {
  const sessions = holdingSessions(history)
  const daily = history?.resolution === 86400
  const first = points[0]?.time ?? end
  const start = Math.max(first, range ? end - range : first)
  const days = sessions.filter((session) => session.kind === 'regular')
  const gaps: Array<{ start: number; end: number; removed: number }> = []
  for (let i = 1; i < days.length; i++) {
    const previous = days[i - 1]
    const next = days[i]
    const gapStart = daily
      ? previous.end
      : previous.end + (72000 - (history?.calendar?.[i - 1]?.close ?? 57600))
    const gapEnd = daily ? next.start : sessions[i * 4].start
    if (gapEnd <= gapStart || gapEnd < start || gapStart > end) continue
    // Unexpected executions override the nominal calendar; never hide evidence.
    if (points.some((point) => point.time > gapStart && point.time < gapEnd)) continue
    const a = Math.max(gapStart, start)
    const b = Math.min(gapEnd, end)
    const kept = Math.min(b - a, daily ? 3600 : (history?.resolution ?? 60))
    gaps.push({ start: a, end: b, removed: b - a - kept })
  }
  const toDisplay = (time: number) => {
    let removed = 0
    for (const gap of gaps) {
      if (time >= gap.end) removed += gap.removed
      else if (time > gap.start)
        removed += (gap.removed * (time - gap.start)) / (gap.end - gap.start)
    }
    return time - removed
  }
  const toActual = (time: number) => {
    let removed = 0
    for (const gap of gaps) {
      const a = gap.start - removed
      const b = gap.end - removed - gap.removed
      if (time >= b) removed += gap.removed
      else if (time > a) return gap.start + ((time - a) * (gap.end - gap.start)) / (b - a)
    }
    return time + removed
  }
  const displayEnd = toDisplay(end)
  const window = Math.max(60, (displayEnd - toDisplay(start)) / 0.95)
  const left = displayEnd - window * 0.95
  const position = (time: number) => ((toDisplay(time) - left) / window) * 100
  return {
    points: points.map((point, i) => ({
      ...point,
      time: toDisplay(point.time),
      breakBefore:
        point.breakBefore ||
        (!daily &&
          i > 0 &&
          gaps.some((gap) => points[i - 1].time <= gap.start && point.time >= gap.end)),
    })),
    end: displayEnd,
    window,
    toDisplay,
    toActual,
    position,
    sessions: daily ? [] : sessions.filter((session) => session.end > start && session.start < end),
    gaps: gaps.filter((gap) => gap.removed > 0 && (!daily || gap.end - gap.start > 86400)),
  }
}

export function holdingChartCandles(
  state: HoldingChartState,
  timeline: ReturnType<typeof holdingChartTimeline>,
  end: number,
  range: number,
) {
  const resolution =
    range === 86400
      ? 900
      : range === 604800
        ? 3600
        : range === 2592000
          ? 14400
          : range === 31536000
            ? 7 * 86400
            : 30 * 86400
  const sessions = holdingSessions(state.history)
  const grouped: Array<PriceBar & { bucket: number; session: number }> = []
  for (const bar of holdingChartBars(state)) {
    if (bar.endTime > end || (range && bar.endTime < end - range)) continue
    // Never combine feeds or intraday sessions. Aggregate only supplied OHLCV;
    // missing bars do not create candles and quotes never enter this path.
    const session =
      resolution < 86400 ? sessions.findIndex((s) => bar.time >= s.start && bar.time < s.end) : -1
    const bucket = Math.floor(bar.time / resolution)
    const last = grouped.at(-1)
    if (last && last.bucket === bucket && last.feed === bar.feed && last.session === session) {
      last.high = Math.max(last.high, bar.high)
      last.low = Math.min(last.low, bar.low)
      last.close = bar.close
      last.endTime = bar.endTime
      last.volume += bar.volume
      last.trades += bar.trades
    } else grouped.push({ ...bar, bucket, session })
  }
  const centers = grouped.map((bar) => timeline.toDisplay(bar.endTime))
  const spacing = centers
    .slice(1)
    .map((time, index) => time - centers[index])
    .filter((value) => value > 0)
  const sortedSpacing = spacing.toSorted((a, b) => a - b)
  const width = Math.min(
    resolution,
    sortedSpacing[Math.floor(sortedSpacing.length / 2)] ?? resolution,
  )
  // A partial candle or compressed closure must not shrink every other body.
  const widths = centers.map((center, index) =>
    Math.min(
      width,
      index > 0 ? center - centers[index - 1] : width,
      index + 1 < centers.length ? centers[index + 1] - center : width,
    ),
  )
  const candles: CandlePoint[] = grouped.map((bar, index) => ({
    time: centers[index] - widths[index] / 2,
    width: widths[index],
    open: bar.open,
    high: bar.high,
    low: bar.low,
    close: bar.close,
  }))
  return { candles, width, resolution }
}
