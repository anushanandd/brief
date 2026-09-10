import { useQuery } from '@tanstack/react-query'
import { useCallback, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'

import { applyTransactionAnnotations, loadTransactionAnnotations } from '../lib/annotations'
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
  const query = useQuery({
    queryKey: financeQueryKey,
    queryFn: getFinanceSnapshot,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const annotations = useQuery({
    queryKey: ['transaction-annotations'],
    queryFn: loadTransactionAnnotations,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const snapshot = useMemo(
    () =>
      query.data
        ? applyTransactionAnnotations(query.data, annotations.data?.annotations ?? {})
        : undefined,
    [query.data, annotations.data],
  )
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
    appendRuntimeLog(
      'Local cache',
      'Finance snapshot loaded',
      'positive',
      `Revision ${query.data.revision ?? 0} · saved ${query.data.updatedAt}`,
    )
  }, [appendRuntimeLog, query.data, query.dataUpdatedAt])

  useEffect(() => {
    if (!integration.data) return
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
      const [result] = await Promise.all([query.refetch(), annotations.refetch()])
      return result
    },
    isLoading: query.isLoading,
    isError: query.isError,
    error: query.error,
    annotationWarning: annotations.isError
      ? 'Transaction annotations could not be loaded. Finance data remains available.'
      : annotations.data?.warning,
    data: snapshot,
    annotations: annotations.data?.annotations ?? {},
    integrationStatus: integration.data ?? { plaid: false, snaptrade: false, alpaca: false },
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
