import { AlertCircle, RefreshCw } from 'lucide-react'

import { useFinance } from '../hooks/use-finance'
import { useFinanceRefreshState, useRefreshFinance } from '../hooks/use-refresh-finance'
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
  const recovery = useFinance().data?.recovery
  const refresh = useRefreshFinance()
  const isRefreshing = useFinanceRefreshState()

  const onRefresh = async () => {
    if (isRefreshing) return
    try {
      await refresh()
    } catch {
      // The shared refresh hook owns the user-facing error state.
    }
  }

  return (
    <Button
      variant="ghost"
      size="icon"
      className={`icon-only-subtle${isRefreshing ? ' refresh-button-active' : ''}`}
      onClick={() => void onRefresh()}
      disabled={isRefreshing || Boolean(recovery)}
      aria-busy={isRefreshing}
      aria-label={isRefreshing ? 'Refreshing accounts' : 'Refresh accounts'}
      title={isRefreshing ? 'Refreshing accounts' : 'Refresh accounts'}
    >
      <RefreshCw
        className={isRefreshing ? 'refresh-icon-active' : undefined}
        size={15}
        aria-hidden="true"
      />
    </Button>
  )
}
