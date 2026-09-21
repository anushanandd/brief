import { useQuery } from '@tanstack/react-query'
import { useCallback, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'

import { loadTransactionAnnotations } from '../lib/annotations'
import { getFinanceSnapshot, getIntegrationStatus } from '../lib/api'
import { FinanceContext, financeQueryKey } from './use-finance'

export type RuntimeLogEntry = {
  id: number
  at: string
  source: string
  message: string
  detail?: string
  tone: 'positive' | 'negative' | 'neutral'
}

function useBaseFinanceState() {
  const integration = useQuery({
    queryKey: ['integration-status'],
    queryFn: getIntegrationStatus,
    staleTime: 60_000,
  })
  const [runtimeLog, setRuntimeLog] = useState<RuntimeLogEntry[]>([])
  const logId = useRef(0)
  const appendRuntimeLog = useCallback(
    (
      source: string,
      message: string,
      tone: RuntimeLogEntry['tone'] = 'neutral',
      detail?: string,
    ) => {
      const at = new Date().toISOString()
      setRuntimeLog((current) => {
        const latest = current[0]
        if (
          latest?.source === source &&
          latest.message === message &&
          latest.detail === detail &&
          Date.parse(at) - Date.parse(latest.at) < 1_500
        ) {
          return current
        }
        logId.current += 1
        return [{ id: logId.current, at, source, message, detail, tone }, ...current].slice(0, 50)
      })
    },
    [],
  )
  // Import legacy annotations before reading the native, annotation-aware view.
  const annotations = useQuery({
    queryKey: ['transaction-annotations'],
    queryFn: loadTransactionAnnotations,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const query = useQuery({
    queryKey: financeQueryKey,
    queryFn: getFinanceSnapshot,
    enabled: annotations.isSuccess || annotations.isError,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const tickers = useMemo(
    () =>
      [
        ...new Set(
          query.data?.holdings
            .filter((holding) => holding.quoteEligible === true)
            .map((holding) => holding.ticker.trim().toUpperCase())
            .filter((ticker) => /^[A-Z0-9.-]{1,24}$/.test(ticker)) ?? [],
        ),
      ].toSorted(),
    [query.data?.holdings],
  )

  useEffect(() => {
    if (!query.dataUpdatedAt || !query.data) return
    if (!performance.getEntriesByName('brief:local-snapshot-ready').length)
      performance.mark('brief:local-snapshot-ready')
    appendRuntimeLog(
      'Local cache',
      'Finance snapshot loaded',
      'positive',
      `Revision ${query.data.revision ?? 0} · saved ${query.data.updatedAt}`,
    )
  }, [appendRuntimeLog, query.data, query.dataUpdatedAt])

  useEffect(() => {
    if (!integration.data) return
    if (!performance.getEntriesByName('brief:integrations-ready').length)
      performance.mark('brief:integrations-ready')
    const configured = Object.entries(integration.data)
      .filter(([, enabled]) => enabled)
      .map(([provider]) => provider)
      .join(', ')
    appendRuntimeLog(
      'Connections',
      'Provider configuration checked',
      integration.isError ? 'negative' : 'positive',
      configured ? `Configured: ${configured}` : 'No provider credentials configured',
    )
  }, [appendRuntimeLog, integration.data, integration.isError])

  return {
    ...query,
    refetch: async () => {
      await annotations.refetch()
      const result = await query.refetch()
      return result
    },
    isLoading: annotations.isLoading || query.isLoading,
    isError: query.isError,
    error: query.error,
    annotationWarning: annotations.isError
      ? 'Transaction annotations could not be loaded. Finance data remains available.'
      : annotations.data?.warning,
    data: query.data,
    annotations: annotations.data?.annotations ?? {},
    integrationStatus: integration.data ?? {
      plaid: false,
      snaptrade: false,
      alpaca: false,
      alphaVantage: false,
    },
    integrationStatusLoading: integration.isLoading,
    liveMarketEnabled: integration.data?.alpaca === true && !query.data?.recovery,
    marketSymbols: tickers,
    runtimeLog,
    appendRuntimeLog,
  }
}

export type FinanceState = ReturnType<typeof useBaseFinanceState>

export function FinanceProvider({ children }: PropsWithChildren) {
  // Keep the shared context in its own module so provider hot reloads preserve its identity.
  const finance = useBaseFinanceState()
  return <FinanceContext.Provider value={finance}>{children}</FinanceContext.Provider>
}
