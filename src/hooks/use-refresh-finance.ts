import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import { useCallback, useEffect, useRef } from 'react'
import { toast } from 'sonner'

import { refreshFinanceSnapshot } from '../lib/api'
import { financeQueryKey } from './use-finance'

const refreshQueryKey = ['refresh-finance'] as const
const AUTO_REFRESH_INTERVAL_MS = 60 * 60 * 1_000

export function isFinanceSyncStale(updatedAt: string, now = Date.now()) {
  const syncedAt = Date.parse(updatedAt)
  return !Number.isFinite(syncedAt) || now - syncedAt >= AUTO_REFRESH_INTERVAL_MS
}

export function useFinanceRefreshState() {
  return useIsFetching({ queryKey: refreshQueryKey, exact: true }) > 0
}

export function useRefreshFinance() {
  const queryClient = useQueryClient()

  return useCallback(
    async ({ silent = false }: { silent?: boolean } = {}) => {
      const toastId = silent ? undefined : toast.loading('Refreshing connected accounts…')
      try {
        const { snapshot, warnings } = await queryClient.fetchQuery({
          queryKey: refreshQueryKey,
          queryFn: refreshFinanceSnapshot,
          staleTime: 0,
        })
        queryClient.setQueryData(financeQueryKey, snapshot)
        if (!silent && warnings.length) {
          toast.warning(`Refresh completed with notices · ${warnings.join(' · ')}`, { id: toastId })
        } else if (!silent) {
          toast.success('Latest available provider data saved', { id: toastId })
        }
        return snapshot
      } catch (error) {
        if (!silent) {
          toast.error(error instanceof Error ? error.message : 'Refresh failed', { id: toastId })
        }
        throw error
      }
    },
    [queryClient],
  )
}

export function useAutoRefreshFinance(updatedAt: string | undefined, enabled: boolean) {
  const refresh = useRefreshFinance()
  const lastAttemptAt = useRef(0)

  useEffect(() => {
    if (!enabled || !updatedAt) return undefined

    const refreshIfStale = () => {
      const now = Date.now()
      if (
        document.visibilityState !== 'visible' ||
        now - lastAttemptAt.current < AUTO_REFRESH_INTERVAL_MS ||
        !isFinanceSyncStale(updatedAt, now)
      ) {
        return
      }
      lastAttemptAt.current = now
      void refresh({ silent: true }).catch(() => undefined)
    }

    refreshIfStale()
    window.addEventListener('focus', refreshIfStale)
    document.addEventListener('visibilitychange', refreshIfStale)
    return () => {
      window.removeEventListener('focus', refreshIfStale)
      document.removeEventListener('visibilitychange', refreshIfStale)
    }
  }, [enabled, refresh, updatedAt])
}
