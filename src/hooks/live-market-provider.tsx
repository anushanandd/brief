import { useQuery } from '@tanstack/react-query'
import type { LivelinePoint } from 'liveline'
import { useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'

import { getMarketSnapshots, isTauri } from '../lib/api'
import { appendLiveChartPoint } from '../lib/live-chart'
import { useFinance } from './use-finance'
import { LiveFinanceContext } from './use-live-finance'

function useLiveMarketState(poll: boolean) {
  const base = useFinance()
  const [marketSeries, setMarketSeries] = useState<Record<string, LivelinePoint[]>>({})
  const marketQuery = useQuery({
    queryKey: ['market-snapshots', base.marketSymbols],
    queryFn: () => getMarketSnapshots(base.marketSymbols),
    enabled: base.liveMarketEnabled && isTauri() && base.marketSymbols.length > 0,
    refetchInterval: poll
      ? (market) => (market.state.error ? 60_000 : (market.state.data?.pollIntervalMs ?? false))
      : false,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: poll,
    retry: false,
  })
  const marketSnapshots = marketQuery.data?.snapshots ?? {}
  const data =
    marketQuery.data?.financeSnapshot && base.liveMarketEnabled
      ? marketQuery.data.financeSnapshot
      : base.data
  const latestUpdateTime = useMemo(() => {
    const times = Object.values(marketSnapshots)
      .map(({ asOf }) => Date.parse(asOf) / 1_000)
      .filter(Number.isFinite)
    return times.length ? Math.max(...times) : undefined
  }, [marketSnapshots])

  useEffect(() => {
    const market = marketQuery.data
    if (!market) return
    base.appendRuntimeLog(
      'Alpaca REST',
      poll && market.pollIntervalMs ? 'Market polling scheduled' : 'Market snapshot loaded',
      'positive',
      `${market.session} · ${market.feed}${market.delayMinutes ? ` · ${market.delayMinutes}-minute delayed` : ''}`,
    )
  }, [
    base.appendRuntimeLog,
    marketQuery.data?.delayMinutes,
    marketQuery.data?.feed,
    marketQuery.data?.pollIntervalMs,
    marketQuery.data?.session,
    poll,
  ])

  const marketHadError = useRef(false)
  useEffect(() => {
    if (marketQuery.isError && !marketHadError.current) {
      marketHadError.current = true
      base.appendRuntimeLog(
        'Alpaca REST',
        'Market snapshot failed',
        'negative',
        marketQuery.error instanceof Error ? marketQuery.error.message : String(marketQuery.error),
      )
    } else if (!marketQuery.isError && marketHadError.current) {
      marketHadError.current = false
      base.appendRuntimeLog('Alpaca REST', 'Market snapshots recovered', 'positive')
    }
  }, [base.appendRuntimeLog, marketQuery.error, marketQuery.isError])

  useEffect(() => {
    if (!poll) return undefined
    const transition = Date.parse(marketQuery.data?.nextTransitionAt ?? '')
    const delay = transition - Date.now() + 1_000
    if (!Number.isFinite(delay) || delay <= 0) return undefined
    const timer = window.setTimeout(() => {
      if (document.visibilityState === 'visible') void marketQuery.refetch()
    }, delay)
    return () => window.clearTimeout(timer)
  }, [marketQuery.data?.nextTransitionAt, marketQuery.refetch, poll])

  useEffect(() => {
    if (!poll || !base.liveMarketEnabled || !data) return
    const time = Math.max(latestUpdateTime ?? 0, Date.parse(data.updatedAt) / 1_000)
    setMarketSeries((current) => {
      const ids = new Set([
        'net-worth',
        ...data.brokeragePerformance.map((account) => account.accountId),
      ])
      const next = Object.fromEntries(Object.entries(current).filter(([id]) => ids.has(id)))
      let changed = Object.keys(next).length !== Object.keys(current).length
      const update = (id: string, value: number) => {
        const points = next[id] ?? []
        const updated = appendLiveChartPoint(points, value, time)
        if (updated !== points) changed = true
        next[id] = updated
      }
      if (!data.netWorthIncomplete) update('net-worth', data.netWorth)
      const investmentsIncomplete = data.accounts.some(
        ({ type, value }) => (type === 'brokerage' || type === 'retirement') && value == null,
      )
      for (const account of data.brokeragePerformance) {
        if (account.accountId !== 'total' || !investmentsIncomplete)
          update(account.accountId, account.currentValue)
      }
      return changed ? next : current
    })
  }, [base.liveMarketEnabled, data, latestUpdateTime, poll])

  const marketPriceState = marketQuery.isError
    ? ('error' as const)
    : marketQuery.isLoading
      ? ('loading' as const)
      : poll && marketQuery.data?.pollIntervalMs
        ? ('active' as const)
        : ('idle' as const)
  const marketFeedLabel =
    marketQuery.data?.feed === 'iex'
      ? 'IEX'
      : marketQuery.data?.feed === 'delayed_sip'
        ? 'Delayed SIP'
        : marketQuery.data?.feed === 'overnight'
          ? 'Overnight indicative'
          : undefined
  const marketPriceMessage = marketQuery.isError
    ? marketQuery.error instanceof Error
      ? marketQuery.error.message
      : String(marketQuery.error)
    : marketFeedLabel
      ? `${marketFeedLabel}${marketQuery.data?.delayMinutes ? ` · ${marketQuery.data.delayMinutes}-minute delayed` : ''}`
      : undefined

  return {
    ...base,
    data,
    marketSeries,
    marketSession: marketQuery.data?.session,
    marketPriceState,
    marketPriceMessage,
  }
}

export type LiveFinanceState = ReturnType<typeof useLiveMarketState>

export function LiveMarketProvider({
  children,
  poll = true,
}: PropsWithChildren<{ poll?: boolean }>) {
  const finance = useLiveMarketState(poll)
  return <LiveFinanceContext.Provider value={finance}>{children}</LiveFinanceContext.Provider>
}
