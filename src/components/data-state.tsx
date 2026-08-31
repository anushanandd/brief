import { useQueryClient } from '@tanstack/react-query'
import { AlertCircle, RefreshCw } from 'lucide-react'
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

  const onRefresh = async () => {
    const promise = refreshFinanceSnapshot()
    toast.promise(promise, {
      loading: 'Refreshing your snapshot…',
      success: 'Snapshot refreshed',
      error: 'Refresh failed',
    })

    queryClient.setQueryData(financeQueryKey, await promise)
  }

  return (
    <Button variant="secondary" onClick={() => void onRefresh()}>
      <RefreshCw size={15} aria-hidden="true" />
      Refresh
    </Button>
  )
}
