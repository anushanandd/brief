import { keepPreviousData, useQuery } from '@tanstack/react-query'
import type { LivelinePoint } from 'liveline'
import { useCallback, useEffect, useMemo, useState, type PropsWithChildren } from 'react'

import { getSavedMarket, isTauri } from '../lib/api'
import { formatUpdatedAt } from '../lib/format'
import {
  applyLiveProjection,
  compatibleMarketProjection,
  forSnapshot,
  subscribeLiveMarket,
  withMarketFrame,
  withMarketWeek,
  type LiveMarketState,
} from '../lib/live-market'
import { useMarketUpdateInterval } from '../lib/market-preferences'
import type { ChartChanges, MarketHistory, MarketSnapshots } from '../lib/schema'
import { useFinance } from './use-finance'
import { LiveFinanceContext } from './use-live-finance'

// Stable empties, so charts reading them don't see new data each render.
const noSeries: Record<string, LivelinePoint[]> = {}
const noChanges: ChartChanges = []

function useLiveMarketState() {
  const base = useFinance()
  const [state, setState] = useState<LiveMarketState>({ series: {} })
  const marketUpdateInterval = useMarketUpdateInterval()
  const [visible, setVisible] = useState(() => typeof document === 'undefined' || !document.hidden)
  const symbolKey = base.marketSymbols.join(',')
  const enabled = visible && base.liveMarketEnabled && isTauri() && !!symbolKey
  const acceptMarket = useCallback(
    (market: MarketSnapshots, first = false) => {
      if (!performance.getEntriesByName('brief:first-market-frame').length)
        performance.mark('brief:first-market-frame')
      setState((current) => withMarketFrame(current, market, symbolKey, first))
    },
    [symbolKey],
  )
  const acceptHistory = useCallback(
    (history: MarketHistory, changes?: ChartChanges) =>
      setState((current) => withMarketWeek(current, history, changes, symbolKey)),
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
    if (!enabled) return undefined
    return subscribeLiveMarket(
      symbolKey.split(','),
      marketUpdateInterval,
      acceptMarket,
      rejectMarket,
      acceptHistory,
    )
  }, [
    enabled,
    symbolKey,
    marketUpdateInterval,
    base.data?.revision,
    base.data?.updatedAt,
    acceptMarket,
    rejectMarket,
    acceptHistory,
  ])
  const saved = useQuery({
    queryKey: ['startup-market', base.data?.revision, base.data?.updatedAt],
    queryFn: getSavedMarket,
    enabled: Boolean(base.data && base.liveMarketEnabled && isTauri()),
    staleTime: Infinity,
    retry: false,
    // Keep showing the last frame while the next loads; the check below ignores its values.
    placeholderData: keepPreviousData,
  })
  const [startupFinished, setStartupFinished] = useState(false)
  const startupPending =
    !startupFinished && (base.isLoading || base.integrationStatusLoading || saved.isLoading)
  useEffect(() => {
    if (!startupPending) setStartupFinished(true)
  }, [startupPending])
  // Values only ever come from a projection of the committed snapshot.
  const compatible = (market: MarketSnapshots | null | undefined) =>
    base.liveMarketEnabled && base.data && compatibleMarketProjection(base.data, market?.projection)
      ? (market ?? undefined)
      : undefined
  const savedMarket = compatible(saved.data)
  // Settled during render, so no frame paints between a snapshot change and its lines.
  const current = forSnapshot(
    state,
    symbolKey,
    `${base.data?.revision ?? 0}:${base.data?.updatedAt ?? ''}`,
    savedMarket,
  )
  if (current !== state) setState(current)
  const market = compatible(current.market) ?? savedMarket
  const data = useMemo(
    () => (base.data ? applyLiveProjection(base.data, market?.projection) : undefined),
    [base.data, market?.projection],
  )
  const analysisReady =
    !base.integrationStatusLoading &&
    (!base.liveMarketEnabled || !symbolKey || !!current.market || !!current.error)
  // Built once per change to the snapshot, the stream, or the saved frame, so a provider
  // render with nothing new keeps every consumer's value (and their memos) as they were.
  return useMemo(() => {
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
      analysisReady,
      // One line per chart from the saved frame, the stream's frames, and its history together,
      // so each message adds detail instead of swapping the line.
      marketSeries: base.liveMarketEnabled ? current.series : noSeries,
      // Live charts' changes from Rust once their lines are whole; until then the saved frame's,
      // else the ones the snapshot carries.
      chartChanges:
        (market?.cached ? market.chartChanges : market ? current.changes : undefined) ??
        (savedMarket ? savedMarket.chartChanges : undefined) ??
        data?.insights?.chartChanges ??
        noChanges,
      marketSession: market?.session,
      marketAsOf: market?.asOf ?? undefined,
      marketFeedLabel,
      marketDelayMinutes: market?.delayMinutes,
      marketNextTransitionAt: market?.nextTransitionAt ?? undefined,
      marketPriceState:
        enabled && current.error
          ? ('error' as const)
          : enabled && (!market || market.cached)
            ? ('loading' as const)
            : market?.pollIntervalMs && !market.cached
              ? ('active' as const)
              : ('idle' as const),
      marketPriceMessage: enabled && current.error ? current.error : undefined,
      marketPriceNote: market
        ? `${current.error ? 'Refresh failed; last available values. ' : ''}${market.cached ? 'Saved market observations · ' : ''}${market.session} · ${marketFeedLabel ?? 'Market data'} · ${market.asOf ? formatUpdatedAt(market.asOf) : 'Observation time unavailable'}. Values and changes use the shared Rust valuation.`
        : enabled && current.error
          ? 'Market refresh failed; showing saved values.'
          : enabled
            ? 'Loading current values and market changes…'
            : 'Current market values are unavailable; showing saved values.',
    }
  }, [base, data, market, savedMarket, current, enabled, startupPending, analysisReady])
}

export type LiveFinanceState = ReturnType<typeof useLiveMarketState>

export function LiveMarketProvider({ children }: PropsWithChildren) {
  const finance = useLiveMarketState()
  return <LiveFinanceContext.Provider value={finance}>{children}</LiveFinanceContext.Provider>
}
