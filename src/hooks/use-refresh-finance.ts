import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { createElement, useCallback, useEffect, useRef } from 'react'
import { toast } from 'sonner'

import { RefreshProgress, type RefreshProgressTask } from '../components/refresh-progress'
import { isTauri, refreshFinanceSnapshot } from '../lib/api'
import { financeQueryKey } from './use-finance'

const refreshQueryKey = ['refresh-finance'] as const
const AUTO_REFRESH_INTERVAL_MS = 60 * 60 * 1_000

type FinanceRefreshProgress = {
  title: string
  detail: string
  task?: RefreshProgressTask
}

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
      const startedAt = Date.now()
      const tasks = new Map<string, RefreshProgressTask>()
      const description = (detail?: string, finished = false) =>
        createElement(RefreshProgress, {
          detail,
          startedAt,
          tasks: [...tasks.values()],
          finished,
        })
      const toastId = silent
        ? undefined
        : toast.loading('Preparing refresh…', {
            description: description(
              'Unlocking credentials and reading the last committed snapshot.',
            ),
          })
      let stopListening: (() => void) | undefined

      if (!silent && isTauri()) {
        stopListening = await listen<FinanceRefreshProgress>(
          'finance-refresh-progress',
          ({ payload }) => {
            if (typeof payload.title !== 'string' || typeof payload.detail !== 'string') return
            if (
              payload.task &&
              typeof payload.task.id === 'string' &&
              typeof payload.task.label === 'string' &&
              typeof payload.task.status === 'string'
            ) {
              tasks.set(payload.task.id, payload.task)
            }
            toast.loading(payload.title, { id: toastId, description: description() })
          },
        ).catch(() => undefined)
      }

      try {
        const { snapshot, warnings } = await queryClient.fetchQuery({
          queryKey: refreshQueryKey,
          queryFn: refreshFinanceSnapshot,
          staleTime: 0,
        })
        queryClient.setQueryData(financeQueryKey, snapshot)
        if (!silent && warnings.length) {
          toast.warning('Refresh completed with notices', {
            id: toastId,
            description: description(warnings.join(' · '), true),
          })
        } else if (!silent) {
          toast.success('Latest available provider data saved', {
            id: toastId,
            description: description('Validated and saved locally.', true),
          })
        }
        return snapshot
      } catch (error) {
        if (!silent) {
          toast.error(error instanceof Error ? error.message : 'Refresh failed', {
            id: toastId,
            description: description('Your previously saved data is unchanged.', true),
          })
        }
        throw error
      } finally {
        stopListening?.()
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
    const timer = window.setInterval(refreshIfStale, 60_000)
    window.addEventListener('focus', refreshIfStale)
    document.addEventListener('visibilitychange', refreshIfStale)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', refreshIfStale)
      document.removeEventListener('visibilitychange', refreshIfStale)
    }
  }, [enabled, refresh, updatedAt])
}
