import { useQuery } from '@tanstack/react-query'
import type { LivelinePoint } from 'liveline'
import { useCallback, useEffect, useMemo, useState, type PropsWithChildren } from 'react'

import { getSavedMarket, isTauri } from '../lib/api'
import {
  applyLiveProjection,
  compatibleMarketProjection,
  liveMarketSeries,
  subscribeLiveMarket,
} from '../lib/live-market'
import { useMarketUpdateInterval } from '../lib/market-preferences'
import type { MarketSnapshots } from '../lib/schema'
import { useFinance } from './use-finance'
import { LiveFinanceContext } from './use-live-finance'

function useLiveMarketState() {
  const base = useFinance()
  const [state, setState] = useState<{
    market?: MarketSnapshots
    symbolKey?: string
    series: Record<string, LivelinePoint[]>
    error?: string
  }>({ series: {} })
  const marketUpdateInterval = useMarketUpdateInterval()
  const [visible, setVisible] = useState(() => typeof document === 'undefined' || !document.hidden)
  const symbolKey = base.marketSymbols.join(',')
  const enabled = visible && base.liveMarketEnabled && isTauri() && !!symbolKey
  const acceptMarket = useCallback(
    (market: MarketSnapshots) => {
      if (!performance.getEntriesByName('brief:first-market-frame').length)
        performance.mark('brief:first-market-frame')
      setState((current) => ({
        market,
        symbolKey,
        series: liveMarketSeries(current.series, market),
      }))
    },
    [symbolKey],
  )
  const rejectMarket = useCallback(
    (error: string) => {
      setState((current) => ({ ...current, error }))
      base.appendRuntimeLog('Alpaca', 'Current market values unavailable', 'negative', error)
    },
    [base.appendRuntimeLog],
  )
  useEffect(() => {
    const update = () => setVisible(!document.hidden)
    document.addEventListener('visibilitychange', update)
    return () => document.removeEventListener('visibilitychange', update)
  }, [])
  useEffect(() => {
    setState({ series: {} })
  }, [symbolKey, base.data?.revision, base.data?.updatedAt])
  useEffect(() => {
    if (!enabled) return undefined
    return subscribeLiveMarket(
      symbolKey.split(','),
      marketUpdateInterval,
      acceptMarket,
      rejectMarket,
    )
  }, [
    enabled,
    symbolKey,
    marketUpdateInterval,
    base.data?.revision,
    base.data?.updatedAt,
    acceptMarket,
    rejectMarket,
  ])
  const saved = useQuery({
    queryKey: ['startup-market', base.data?.revision, base.data?.updatedAt],
    queryFn: getSavedMarket,
    enabled: Boolean(base.data && base.liveMarketEnabled && isTauri()),
    staleTime: Infinity,
    retry: false,
  })
  const [startupFinished, setStartupFinished] = useState(false)
  const startupPending =
    !startupFinished && (base.isLoading || base.integrationStatusLoading || saved.isLoading)
  useEffect(() => {
    if (!startupPending) setStartupFinished(true)
  }, [startupPending])
  const candidate = state.market ?? saved.data ?? undefined
  const market =
    base.liveMarketEnabled &&
    (state.symbolKey === symbolKey || !state.market) &&
    base.data &&
    compatibleMarketProjection(base.data, candidate?.projection)
      ? candidate
      : undefined
  const data = useMemo(
    () => (base.data ? applyLiveProjection(base.data, market?.projection) : undefined),
    [base.data, market?.projection],
  )
  const marketCloseTime = market
    ? Object.values(market.snapshots).reduce<number | undefined>((latest, snapshot) => {
        const time = Date.parse(snapshot.previousCloseAsOf ?? '') / 1_000
        return Number.isFinite(time) && (latest === undefined || time > latest) ? time : latest
      }, undefined)
    : undefined
  const marketFeedLabel =
    market?.feed === 'iex'
      ? 'IEX'
      : market?.feed === 'sip'
        ? 'SIP'
        : market?.feed === 'boats'
          ? 'BOATS'
          : market?.feed === 'delayed_sip'
            ? 'Delayed SIP'
            : market?.feed === 'overnight'
              ? 'Overnight indicative'
              : undefined
  return {
    ...base,
    data,
    valuationAsOf: market?.asOf ?? base.data?.updatedAt,
    startupPending,
    marketIsSaved: Boolean(market?.cached),
    analysisReady:
      !base.integrationStatusLoading &&
      (!base.liveMarketEnabled || !symbolKey || !!state.market || !!state.error),
    marketSeries: market?.cached ? (market.chartSeries ?? {}) : market ? state.series : {},
    marketCloseTime,
    marketSession: market?.session,
    marketPriceState:
      enabled && state.error
        ? ('error' as const)
        : enabled && (!market || market.cached)
          ? ('loading' as const)
          : market?.pollIntervalMs && !market.cached
            ? ('active' as const)
            : ('idle' as const),
    marketPriceMessage:
      enabled && state.error
        ? state.error
        : marketFeedLabel
          ? `${marketFeedLabel}${market?.delayMinutes ? ` · ${market.delayMinutes}-minute delayed` : ''}`
          : undefined,
    marketPriceNote: market
      ? `${state.error ? 'Refresh failed; last available values. ' : ''}${market.cached ? 'Saved market observations · ' : ''}${market.session} · ${marketFeedLabel ?? 'Market data'} · ${market.asOf ? new Date(market.asOf).toLocaleString() : 'Observation time unavailable'}. Values and changes use the shared Rust valuation.`
      : enabled && state.error
        ? 'Market refresh failed; showing saved values.'
        : enabled
          ? 'Loading current values and market changes…'
          : 'Current market values are unavailable; showing saved values.',
  }
}

export type LiveFinanceState = ReturnType<typeof useLiveMarketState>

export function LiveMarketProvider({ children }: PropsWithChildren) {
  const finance = useLiveMarketState()
  return <LiveFinanceContext.Provider value={finance}>{children}</LiveFinanceContext.Provider>
}
