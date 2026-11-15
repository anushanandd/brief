import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'

import { isTauri, startHoldingChart, stopHoldingChart } from '../lib/api'
import {
  cachedHoldingPreview,
  reduceHoldingChart,
  type HoldingChartState,
  type HoldingPreviewEntry,
} from '../lib/holding-prices'
import { holdingChartEventSchema } from '../lib/schema'

type Shown = { key: string; chart: HoldingChartState; previous?: HoldingChartState }
const lastDrawn = (shown: Shown) => (shown.chart.history ? shown.chart : shown.previous)

export function useHoldingPrices(
  symbol: string,
  range: number,
  enabled: boolean,
  warmSymbols: string[] = [],
) {
  const client = useQueryClient()
  const key = `${symbol}:${range}`
  const cacheKey = ['holding-chart-preview', symbol, range] as const
  const preview = () =>
    enabled
      ? cachedHoldingPreview(client.getQueryData<HoldingPreviewEntry>(cacheKey), symbol, range)
      : { symbol, range, bars: [] }
  const [stored, setStored] = useState<Shown>(() => ({ key, chart: preview() }))
  const state = stored.key === key ? stored.chart : preview()
  const previous = stored.key === key ? stored.previous : lastDrawn(stored)
  const [visible, setVisible] = useState(() => typeof document === 'undefined' || !document.hidden)
  const [now, setNow] = useState(() => Date.now() / 1000)
  const active = enabled && visible && !!symbol && isTauri()
  const activeSession = useRef<object | null>(null)
  const latestRequest = useRef<string | null>(null)
  const startQueue = useRef(Promise.resolve())
  useEffect(() => {
    if (!active) return undefined
    activeSession.current = {}
    return () => {
      activeSession.current = null
      if (latestRequest.current) void stopHoldingChart(latestRequest.current).catch(() => undefined)
      latestRequest.current = null
    }
  }, [active])
  useEffect(() => {
    const update = () => setVisible(!document.hidden)
    document.addEventListener('visibilitychange', update)
    return () => {
      document.removeEventListener('visibilitychange', update)
    }
  }, [])
  useEffect(() => {
    if (!active) return undefined
    setNow(Date.now() / 1000)
    const timer = setInterval(() => setNow(Date.now() / 1000), 1000)
    return () => clearInterval(timer)
  }, [active])
  useEffect(() => {
    if (!active) return undefined
    const requestId = crypto.randomUUID()
    const session = activeSession.current
    let disposed = false
    let stop: (() => void) | undefined
    let previousRequest: string | null = null
    let chart = preview()
    setStored((current) => ({
      key,
      chart,
      previous: current.key === key ? current.previous : lastDrawn(current),
    }))
    void (async () => {
      try {
        stop = await listen<unknown>('holding-chart-update', ({ payload }) => {
          const raw = (payload ?? {}) as Partial<
            Record<'requestId' | 'symbol' | 'kind', unknown>
          > & {
            history?: Partial<Record<'symbol' | 'range', unknown>>
          }
          if (disposed || raw.requestId !== requestId) return
          // A warmed neighbor's history is large, so it's kept as it came until its chart opens.
          if (raw.symbol !== symbol) {
            if (typeof raw.symbol !== 'string') return
            if (raw.kind === 'invalidate')
              client.removeQueries({ queryKey: ['holding-chart-preview', raw.symbol] })
            else if (
              raw.kind === 'history' &&
              raw.history?.symbol === raw.symbol &&
              raw.history.range === range
            )
              client.setQueryData<HoldingPreviewEntry>(
                ['holding-chart-preview', raw.symbol, range],
                (held) => ({
                  event: payload,
                  savedComparison: held?.savedComparison,
                  connection: chart.connection,
                }),
              )
            return
          }
          const parsed = holdingChartEventSchema.safeParse(payload)
          if (!parsed.success) return
          const event = parsed.data
          if (
            event.kind === 'history' &&
            (event.history.symbol !== symbol || event.history.range !== range)
          )
            return
          if (event.kind === 'invalidate')
            client.removeQueries({ queryKey: ['holding-chart-preview', symbol] })
          chart = reduceHoldingChart(chart, event)
          if (chart.history && (event.kind === 'history' || event.kind === 'status'))
            client.setQueryData(cacheKey, {
              ...chart.history,
              savedComparison: chart.savedComparison,
            })
          setStored((current) => ({ key, chart, previous: current.previous }))
        })
        if (disposed) {
          stop()
          return
        }
        // Serialize native selection changes; a slow old start must not replace a newer one.
        const start = startQueue.current.then(async () => {
          if (disposed) return
          previousRequest = latestRequest.current
          latestRequest.current = requestId
          await startHoldingChart(symbol, range, requestId, warmSymbols)
          if (activeSession.current !== session) await stopHoldingChart(requestId)
        })
        startQueue.current = start.catch(() => undefined)
        await start
      } catch {
        if (latestRequest.current === requestId) latestRequest.current = previousRequest
        if (activeSession.current !== session && previousRequest)
          void stopHoldingChart(previousRequest).catch(() => undefined)
        if (!disposed)
          setStored((current) => ({
            key,
            previous: current.previous,
            chart: {
              ...(current.key === key ? current.chart : preview()),
              error: 'Could not start market data.',
            },
          }))
      }
    })()
    return () => {
      disposed = true
      stop?.()
    }
  }, [active, symbol, range, client])
  // Never display a prior ticker during effect cleanup and startup. While the ticker's new
  // range loads, its last drawn range stays up, marked `stale`.
  const shown: HoldingChartState =
    state.symbol === symbol && state.range === range ? state : { bars: [] }
  const fallback =
    !shown.history && !shown.error && !shown.historyError && previous?.symbol === symbol
      ? previous
      : undefined
  return { ...(fallback ?? shown), stale: Boolean(fallback), visible, now }
}
