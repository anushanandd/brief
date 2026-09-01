import { useQueryClient } from '@tanstack/react-query'
import { AlertCircle, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey, useFinance } from '../hooks/use-finance'
import { refreshFinanceSnapshot } from '../lib/api'
import { Button } from './ui'

export function PageLoading() {
  return (
    <div className="page loading-page" aria-label="Loading finance data">
      <div className="skeleton skeleton-title" />
      <div className="metric-grid">
        {[0, 1, 2].map((item) => (
          <div key={item} className="surface-card skeleton-card">
            <div className="skeleton skeleton-label" />
            <div className="skeleton skeleton-value" />
          </div>
        ))}
      </div>
      <div className="surface-card skeleton-chart skeleton" />
    </div>
  )
}

export function PageError() {
  const query = useFinance()

  return (
    <div className="page centered-state">
      <AlertCircle size={24} aria-hidden="true" />
      <h1>We couldn’t load your snapshot.</h1>
      <p>{query.error instanceof Error ? query.error.message : 'Unknown error'}</p>
      <Button variant="primary" onClick={() => void query.refetch()}>
        Try again
      </Button>
    </div>
  )
}

export function RefreshButton() {
  const queryClient = useQueryClient()
  const [isRefreshing, setIsRefreshing] = useState(false)

  const onRefresh = async () => {
    if (isRefreshing) return
    setIsRefreshing(true)
    const promise = refreshFinanceSnapshot()
    toast.promise(promise, {
      loading: 'Refreshing your snapshot…',
      success: 'Snapshot refreshed',
      error: 'Refresh failed',
    })

    try {
      queryClient.setQueryData(financeQueryKey, await promise)
    } catch {
      // toast.promise owns the user-facing error state; consume the rejection from this event
      // handler so a failed provider refresh does not become an unhandled browser rejection.
    } finally {
      setIsRefreshing(false)
    }
  }

  return (
    <Button
      variant="secondary"
      size="icon"
      onClick={() => void onRefresh()}
      disabled={isRefreshing}
      aria-label={isRefreshing ? 'Refreshing accounts' : 'Refresh accounts'}
      title={isRefreshing ? 'Refreshing accounts' : 'Refresh accounts'}
    >
      <RefreshCw size={15} aria-hidden="true" />
    </Button>
  )
}
