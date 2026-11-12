import { useIsFetching, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { createElement, useCallback, useEffect, useRef } from 'react'
import { toast } from 'sonner'

import { RefreshProgress, type RefreshProgressTask } from '../components/refresh-progress'
import { isTauri, refreshFinanceSnapshot, syncWallet } from '../lib/api'
import { setFinanceSnapshot } from './use-finance'

const refreshQueryKey = ['refresh-finance'] as const
const AUTO_REFRESH_INTERVAL_MS = 60 * 60 * 1_000
const WALLET_CHECK_INTERVAL_MS = 60_000

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
  return useIsFetching({ queryKey: refreshQueryKey }) > 0
}

const refreshKey = (live: boolean) => [...refreshQueryKey, live ? 'live' : 'saved'] as const
const startRefresh = (client: QueryClient, live: boolean) =>
  client.fetchQuery({
    queryKey: refreshKey(live),
    queryFn: () => refreshFinanceSnapshot({ liveBalances: live }),
    staleTime: 0,
  })

/**
 * Runs one refresh at a time. Asking while one runs joins it, except a live-balance refresh
 * asked for during an ordinary one: that waits for it to finish, then runs live, so what was
 * asked for happens. An ordinary request joins a live one already running, which is fresher.
 * Live balances are billed, so only an explicit `live` request ever fetches them.
 */
export async function refreshOnce(client: QueryClient, live: boolean) {
  const running = (mode: boolean) => client.isFetching({ queryKey: refreshKey(mode), exact: true })
  if (running(true)) return startRefresh(client, true)
  if (!live || !running(false)) return startRefresh(client, live)
  await startRefresh(client, false).catch(() => undefined)
  return refreshOnce(client, true)
}

export function useRefreshFinance() {
  const queryClient = useQueryClient()

  return useCallback(
    async ({
      silent = false,
      liveBalances = false,
    }: { silent?: boolean; liveBalances?: boolean } = {}) => {
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
        : toast.loading('Preparing refresh…', { description: description() })
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
        const { snapshot, warnings } = await refreshOnce(queryClient, liveBalances)
        setFinanceSnapshot(queryClient, snapshot)
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

/**
 * While Apple Wallet is on and the window is visible, picks up charges as Wallet records them.
 * Each check costs one file stat in Rust unless Wallet has written, and a failure waits for
 * the next refresh to report it.
 */
export function useWalletWatch(enabled: boolean) {
  const client = useQueryClient()

  useEffect(() => {
    if (!enabled) return undefined

    const check = () => {
      if (document.visibilityState !== 'visible') return
      void syncWallet(true)
        .then((view) => {
          if (view) setFinanceSnapshot(client, view)
        })
        .catch(() => undefined)
    }

    check()
    const timer = window.setInterval(check, WALLET_CHECK_INTERVAL_MS)
    document.addEventListener('visibilitychange', check)
    return () => {
      window.clearInterval(timer)
      document.removeEventListener('visibilitychange', check)
    }
  }, [client, enabled])
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
