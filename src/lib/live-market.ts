import type { LivelinePoint } from 'liveline'

import { startMarketStream, stopMarketStream } from './api'
import { errorMessage } from './errors'
import { upsertLiveChartPoint } from './live-chart'
import type {
  ChartChanges,
  FinanceSnapshot,
  MarketHistory,
  MarketProjection,
  MarketSnapshots,
} from './schema'

const MARKET_CHART_INTERVAL = 5 * 60
// A week of five-minute points and a month of half-hour ones, as Rust draws the lines.
const MAX_MARKET_CHART_POINTS = 7 * 24 * 12 + 30 * 24 * 2 + 1

function downsampleMarketSeries(points: LivelinePoint[]) {
  const ordered = points.toSorted((left, right) => left.time - right.time)
  const first = ordered[0]
  if (!first) return []
  const buckets = new Map<number, LivelinePoint>()
  for (const point of ordered.slice(1)) {
    buckets.set(Math.floor(point.time / MARKET_CHART_INTERVAL), point)
  }
  const sampled = [first, ...buckets.values()].filter(
    (point, index, values) => point.time !== values[index - 1]?.time,
  )
  return sampled.length <= MAX_MARKET_CHART_POINTS
    ? sampled
    : [first, ...sampled.slice(-(MAX_MARKET_CHART_POINTS - 1))]
}

export function compatibleMarketProjection(
  saved: Pick<FinanceSnapshot, 'revision' | 'updatedAt'>,
  projection?: Pick<MarketProjection, 'revision' | 'updatedAt'>,
) {
  return (
    !!projection &&
    projection.revision === (saved.revision ?? 0) &&
    projection.updatedAt === saved.updatedAt
  )
}

// Rust supplies all values. Unchanged history and transactions retain their identity.
export function applyLiveProjection(
  saved: FinanceSnapshot,
  projection?: MarketProjection,
): FinanceSnapshot {
  if (!compatibleMarketProjection(saved, projection) || !projection) return saved
  const values = new Map(
    projection.brokeragePerformance.map((account) => [account.accountId, account.currentValue]),
  )
  const accounts = new Map(projection.accounts.map((account) => [account.id, account]))
  return {
    ...saved,
    // Account totals at the live values, as Rust summed them.
    insights: saved.insights && { ...saved.insights, rollup: projection.rollup },
    netWorth: projection.netWorth,
    netWorthIncomplete: projection.netWorthIncomplete,
    accounts: saved.accounts.map((account) => {
      const live = accounts.get(account.id)
      return live
        ? {
            ...account,
            value: live.value,
            cashValue: live.cashValue,
            investedValue: live.investedValue,
            knownUnrealizedGain: live.knownUnrealizedGain,
            knownUnrealizedGainPct: live.knownUnrealizedGainPct,
          }
        : account
    }),
    holdings: projection.holdings,
    brokeragePerformance: saved.brokeragePerformance.map((account) => {
      const currentValue = values.get(account.accountId)
      return currentValue === undefined || currentValue === account.currentValue
        ? account
        : { ...account, currentValue }
    }),
  }
}

/**
 * A line from Rust joined to the one held: every point of both, Rust's at a time both have, so
 * a correction replaces what it corrects. A `whole` line (one Rust drew from bars) is the line
 * over its own span, so the points held inside that span go and those before and after stay.
 * Either way the line never covers less time than it did.
 */
function mergeMarketLine(held: LivelinePoint[] = [], incoming: LivelinePoint[], whole: boolean) {
  const points = incoming.filter(
    (point) => Number.isFinite(point.time) && Number.isFinite(point.value),
  )
  if (!points.length) return held
  const times = points.map(({ time }) => time)
  const from = Math.min(...times)
  const to = Math.max(...times)
  const kept = whole ? held.filter(({ time }) => time < from || time > to) : held
  const byTime = new Map(kept.map((point) => [point.time, point]))
  for (const point of points) byTime.set(point.time, point)
  return downsampleMarketSeries([...byTime.values()])
}

function mergeMarketSeries(
  current: Record<string, LivelinePoint[]>,
  incoming: Record<string, LivelinePoint[]>,
  whole: boolean,
) {
  const next = { ...current }
  for (const [id, points] of Object.entries(incoming))
    next[id] = mergeMarketLine(current[id], points, whole)
  return next
}

/**
 * A frame's lines joined to the ones held. A first frame carries two points (the previous
 * close and now) when a holding's saved bars are missing, so it joins the saved frame's detail
 * rather than replacing it; lines the frame leaves out stay as they were.
 */
export function liveMarketSeries(
  current: Record<string, LivelinePoint[]>,
  market: MarketSnapshots,
) {
  if (market.chartSeries) return mergeMarketSeries(current, market.chartSeries, false)
  if (!market.chartPoint) return current
  const next = { ...current }
  for (const [id, point] of Object.entries(market.chartPoint))
    next[id] = downsampleMarketSeries(
      upsertLiveChartPoint(current[id] ?? [], point.value, point.time),
    )
  return next
}

/**
 * The intraday week (and month) Rust built from price bars in place of the held points it
 * spans; earlier points and the ticks the stream added after its last bar stay, so history
 * arriving a moment after the first frame never drops either.
 */
export function withMarketHistory(
  current: Record<string, LivelinePoint[]>,
  history: MarketHistory,
) {
  return mergeMarketSeries(current, history, true)
}

/** The live stream as the renderer holds it, for one set of symbols. */
export type LiveMarketState = {
  market?: MarketSnapshots
  symbolKey?: string
  /** The committed snapshot (revision and time) the stream's values were projected from. */
  snapshot?: string
  /** One line per chart, only ever gaining detail while the symbols stay the same. */
  series: Record<string, LivelinePoint[]>
  /** Each value chart's change at the live lines, as Rust read them. */
  changes?: ChartChanges
  /**
   * Until a stream's week of bars joins its lines, frames read changes from a partial line;
   * they wait here, so the shown change settles once instead of stepping toward its value.
   */
  pendingChanges?: ChartChanges
  settled?: boolean
  error?: string
}

/**
 * The state for the committed snapshot and its symbols, seeded from the saved startup frame
 * (`saved`, already checked against the snapshot) while it holds no lines.
 *
 * Another set of symbols starts over. A new snapshot of the same symbols drops the stream's
 * frame, changes, and error, since they were valued from the old snapshot, but keeps the lines'
 * shape: the restarted stream's first frame and history redraw every timestamp they cover, and
 * Rust's points replace the held ones there, so a line drawn at the old quantities lasts only
 * until those arrive. Only a tick or two the old stream added after its last bar can outlive
 * that, and the next tick in their five minutes replaces them.
 */
export function forSnapshot(
  current: LiveMarketState,
  symbolKey: string,
  snapshot: string,
  saved: MarketSnapshots | undefined,
): LiveMarketState {
  let next = current
  if (current.symbolKey !== symbolKey) next = { symbolKey, snapshot, series: {} }
  else if (current.snapshot !== snapshot) next = { symbolKey, snapshot, series: current.series }
  const savedSeries = saved?.chartSeries
  if (!Object.keys(next.series).length && savedSeries && Object.keys(savedSeries).length)
    next = { ...next, series: liveMarketSeries({}, saved) }
  return next
}

/** A frame joins the lines. A stream's `first` frame waits for its week again. */
export function withMarketFrame(
  current: LiveMarketState,
  market: MarketSnapshots,
  symbolKey: string,
  first: boolean,
): LiveMarketState {
  const same = current.symbolKey === symbolKey
  const settled = !first && same && Boolean(current.settled)
  // A restarted stream keeps showing the changes it had until its own week arrives.
  const shown = same ? current.changes : undefined
  return {
    market,
    symbolKey,
    snapshot: same ? current.snapshot : undefined,
    series: liveMarketSeries(same ? current.series : {}, market),
    settled,
    ...(settled
      ? { changes: market.chartChanges ?? shown }
      : {
          changes: shown,
          pendingChanges: market.chartChanges ?? (first ? undefined : current.pendingChanges),
        }),
  }
}

/** The week joins the lines and settles the changes they show. */
export function withMarketWeek(
  current: LiveMarketState,
  history: MarketHistory,
  changes: ChartChanges | undefined,
  symbolKey: string,
): LiveMarketState {
  if (current.symbolKey !== symbolKey) return current
  return {
    ...current,
    series: withMarketHistory(current.series, history),
    changes: changes ?? current.pendingChanges ?? current.changes,
    pendingChanges: undefined,
    settled: true,
  }
}

// One app-level owner. Each restart gets scoped cancellation.
export function subscribeLiveMarket(
  symbols: string[],
  interval: number,
  /** `first` marks each stream's first frame, which a history message then completes. */
  onUpdate: (market: MarketSnapshots, first: boolean) => void,
  onError: (message: string) => void,
  onHistory: (series: MarketHistory, changes?: ChartChanges) => void = () => undefined,
) {
  let disposed = false
  let active = ''
  let timer: ReturnType<typeof setTimeout> | undefined
  const schedule = (delay: number) => {
    clearTimeout(timer)
    timer = setTimeout(start, Math.max(1_000, delay))
  }
  const start = () => {
    if (disposed) return
    clearTimeout(timer)
    const previous = active
    const id = crypto.randomUUID()
    active = id
    if (previous) void stopMarketStream(previous).catch(() => undefined)
    let received = false
    const failed = (message: string) => {
      if (disposed || active !== id) return
      onError(message)
      schedule(60_000)
    }
    void startMarketStream(
      symbols,
      interval,
      id,
      (market) => {
        if (disposed || active !== id) return
        onUpdate(market, !received)
        if (received) return
        received = true
        const transition = Date.parse(market.nextTransitionAt ?? '') - Date.now() + 1_000
        const delay = Number.isFinite(transition) ? transition : Infinity
        if (Number.isFinite(delay)) schedule(delay)
      },
      failed,
      (series, changes) => {
        if (disposed || active !== id) return
        onHistory(series, changes)
      },
    )
      .then(() => {
        if (disposed || active !== id) void stopMarketStream(id).catch(() => undefined)
      })
      .catch((error: unknown) => failed(errorMessage(error)))
  }
  start()
  return () => {
    disposed = true
    clearTimeout(timer)
    if (active) void stopMarketStream(active).catch(() => undefined)
  }
}
