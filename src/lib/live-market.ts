import type { LivelinePoint } from 'liveline'

import { startMarketStream, stopMarketStream } from './api'
import { upsertLiveChartPoint } from './live-chart'
import type { FinanceSnapshot, MarketProjection, MarketSnapshots } from './schema'

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
    netWorth: projection.netWorth,
    netWorthIncomplete: projection.netWorthIncomplete,
    accounts: saved.accounts.map((account) => {
      const live = accounts.get(account.id)
      return live
        ? {
            ...account,
            value: live.value,
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

export function liveMarketSeries(
  current: Record<string, LivelinePoint[]>,
  market: MarketSnapshots,
) {
  if (market.chartSeries) return market.chartSeries
  if (!market.chartPoint) return current
  return Object.fromEntries(
    Object.entries(market.chartPoint).map(([id, point]) => {
      const points = upsertLiveChartPoint(current[id] ?? [], point.value, point.time)
      return [id, points.length > 2 ? [points[0], points.at(-1)!] : points]
    }),
  )
}

// One app-level owner. Each restart gets scoped cancellation.
export function subscribeLiveMarket(
  symbols: string[],
  interval: number,
  onUpdate: (market: MarketSnapshots) => void,
  onError: (message: string) => void,
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
        onUpdate(market)
        if (received) return
        received = true
        const transition = Date.parse(market.nextTransitionAt ?? '') - Date.now() + 1_000
        const delay = Number.isFinite(transition) ? transition : Infinity
        if (Number.isFinite(delay)) schedule(delay)
      },
      failed,
    )
      .then(() => {
        if (disposed || active !== id) void stopMarketStream(id).catch(() => undefined)
      })
      .catch((error: unknown) => failed(error instanceof Error ? error.message : String(error)))
  }
  start()
  return () => {
    disposed = true
    clearTimeout(timer)
    if (active) void stopMarketStream(active).catch(() => undefined)
  }
}
