import { useQuery } from '@tanstack/react-query'
import type { LivelinePoint } from 'liveline'
import { useEffect, useMemo, useState } from 'react'

import { getFinanceSnapshot, getMarketSnapshots, isTauri } from '../lib/api'
import { appendLiveChartPoint } from '../lib/live-chart'
import { applyMarketSnapshots } from '../lib/normalize'

export const financeQueryKey = ['finance-snapshot'] as const

export function useFinance({ liveMarket = false }: { liveMarket?: boolean } = {}) {
  const [marketSeries, setMarketSeries] = useState<Record<string, LivelinePoint[]>>({})
  const query = useQuery({
    queryKey: financeQueryKey,
    queryFn: getFinanceSnapshot,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const tickers = useMemo(
    () =>
      [
        ...new Set(
          query.data?.holdings
            .map((holding) => holding.ticker.trim().toUpperCase())
            .filter((ticker) => /^[A-Z0-9.-]{1,24}$/.test(ticker)) ?? [],
        ),
      ].toSorted(),
    [query.data?.holdings],
  )
  const marketQuery = useQuery({
    queryKey: ['market-snapshots', tickers],
    queryFn: () => getMarketSnapshots(tickers),
    enabled: liveMarket && isTauri() && tickers.length > 0,
    refetchInterval: 10_000,
    retry: false,
  })
  const data = useMemo(
    () =>
      query.data && marketQuery.data
        ? applyMarketSnapshots(query.data, marketQuery.data)
        : query.data,
    [marketQuery.data, query.data],
  )

  useEffect(() => {
    if (!liveMarket || !data) return
    const time = Date.now() / 1_000
    setMarketSeries((current) => {
      const next = { ...current }
      next['net-worth'] = appendLiveChartPoint(next['net-worth'] ?? [], data.netWorth, time)
      for (const account of data.brokeragePerformance) {
        next[account.accountId] = appendLiveChartPoint(
          next[account.accountId] ?? [],
          account.currentValue,
          time,
        )
      }
      return next
    })
  }, [data, liveMarket])

  return {
    ...query,
    data,
    marketSeries,
    marketStreamState: marketQuery.isError
      ? ('error' as const)
      : marketQuery.isLoading
        ? ('connecting' as const)
        : marketQuery.isSuccess
          ? ('live' as const)
          : ('idle' as const),
    marketStreamMessage: marketQuery.isError
      ? marketQuery.error instanceof Error
        ? marketQuery.error.message
        : String(marketQuery.error)
      : marketQuery.isSuccess
        ? 'Polling every 10 seconds'
        : undefined,
  }
}
