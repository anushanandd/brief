import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useRef, useState } from 'react'

import { isTauri, startHoldingChart, stopHoldingChart } from '../lib/api'
import {
  cachedHoldingPreview,
  reduceHoldingChart,
  type HoldingChartState,
  type HoldingPreviewHistory,
} from '../lib/holding-prices'
import { holdingChartEventSchema } from '../lib/schema'
import { useFinance } from './use-finance'

export function useHoldingPrices(
  symbol: string,
  range: number,
  enabled: boolean,
  warmSymbols: string[] = [],
) {
  const client = useQueryClient()
  const finance = useFinance()
  const revision = String(finance.data?.revision ?? finance.data?.updatedAt ?? '')
  const key = `${revision}:${symbol}:${range}`
  const cacheKey = ['holding-chart-preview', revision, symbol, range] as const
  const preview = () =>
    enabled
      ? cachedHoldingPreview(client.getQueryData<HoldingPreviewHistory>(cacheKey), symbol, range)
      : { symbol, range, bars: [] }
  const [stored, setStored] = useState<{ key: string; chart: HoldingChartState }>(() => ({
    key,
    chart: preview(),
  }))
  const state = stored.key === key ? stored.chart : preview()
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
    setStored({ key, chart: preview() })
    void (async () => {
      try {
        stop = await listen<unknown>('holding-chart-update', ({ payload }) => {
          const parsed = holdingChartEventSchema.safeParse(payload)
          if (!disposed && parsed.success && parsed.data.requestId === requestId) {
            const event = parsed.data
            if (
              event.kind === 'history' &&
              event.history.range === range &&
              event.history.symbol === event.symbol
            ) {
              const previous = client.getQueryData<HoldingPreviewHistory>([
                'holding-chart-preview',
                revision,
                event.symbol,
                range,
              ])
              const verified = reduceHoldingChart(
                {
                  bars: [],
                  savedComparison: previous?.savedComparison,
                  connection: chart.connection,
                },
                event,
              )
              client.setQueryData(['holding-chart-preview', revision, event.symbol, range], {
                ...event.history,
                savedComparison: verified.savedComparison,
              })
            }
            if (event.kind === 'invalidate')
              client.removeQueries({ queryKey: ['holding-chart-preview', revision, event.symbol] })
            if (event.symbol !== symbol) return
            if (
              event.kind === 'history' &&
              (event.history.symbol !== symbol || event.history.range !== range)
            )
              return
            if (event.kind === 'invalidate')
              client.removeQueries({ queryKey: cacheKey, exact: true })
            chart = reduceHoldingChart(chart, event)
            if (chart.history && (event.kind === 'history' || event.kind === 'status'))
              client.setQueryData(cacheKey, {
                ...chart.history,
                savedComparison: chart.savedComparison,
              })
            setStored({ key, chart })
          }
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
  }, [active, symbol, range, revision, client])
  // Never display a prior ticker/range during effect cleanup and startup.
  return {
    ...(state.symbol === symbol && state.range === range ? state : { bars: [] }),
    visible,
    now,
  }
}
