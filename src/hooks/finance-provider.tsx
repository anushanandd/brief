import { useQuery } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'

import { hasLegacyAnnotations, loadTransactionAnnotations } from '../lib/annotations'
import { getFinanceSnapshot, getIntegrationStatus, isTauri } from '../lib/api'
import { localDateKey } from '../lib/dates'
import { hasLegacySettings, importLegacyFinanceSettings } from '../lib/finance-settings'
import { formatUpdatedAt } from '../lib/format'
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
  // Settings and annotations kept in renderer storage by older versions are imported into Rust.
  // Only then does the native view include them, so the first read waits for the import, but
  // only when there is something to import; otherwise both start at once.
  const [legacyImport] = useState(() => hasLegacySettings() || hasLegacyAnnotations())
  const annotations = useQuery({
    queryKey: ['transaction-annotations'],
    queryFn: async () => {
      const settingsWarning = await importLegacyFinanceSettings()
      const result = await loadTransactionAnnotations()
      const warning = [result.warning, settingsWarning].filter(Boolean).join(' ')
      return { ...result, warning: warning || undefined }
    },
    staleTime: Number.POSITIVE_INFINITY,
  })
  const query = useQuery({
    queryKey: financeQueryKey,
    queryFn: getFinanceSnapshot,
    enabled: !legacyImport || annotations.isSuccess || annotations.isError,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const insightsDay = query.data?.insights?.today
  const refetchSnapshot = query.refetch
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
      `Revision ${query.data.revision ?? 0} · saved ${formatUpdatedAt(query.data.updatedAt)}`,
    )
  }, [appendRuntimeLog, query.data, query.dataUpdatedAt])

  // A background refresh commits while the window is hidden; reread when it reports one.
  useEffect(() => {
    if (!isTauri()) return undefined
    let stop: (() => void) | undefined
    let disposed = false
    void listen('finance-snapshot-updated', () => void refetchSnapshot()).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [refetchSnapshot])

  // Native insights describe "today", so a new calendar day rereads the saved snapshot.

  useEffect(() => {
    // The synthetic preview is pinned to its seed date.
    if (!insightsDay || !isTauri()) return undefined
    const check = () => {
      if (localDateKey(Date.now() / 1000) !== insightsDay) void refetchSnapshot()
    }
    const timer = window.setInterval(check, 60_000)
    window.addEventListener('focus', check)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', check)
    }
  }, [insightsDay, refetchSnapshot])

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
    isLoading: (legacyImport && annotations.isLoading) || query.isLoading,
    isError: query.isError,
    error: query.error,
    annotationWarning: annotations.isError
      ? 'Couldn’t load annotations'
      : annotations.data?.warning,
    data: query.data,
    integrationStatus: integration.data ?? {
      plaid: false,
      snaptrade: false,
      alpaca: false,
      alphaVantage: false,
      sec: false,
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
