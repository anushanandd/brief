import { useFinance } from '../hooks/use-finance'
import { useFinanceRefreshState, useRefreshFinance } from '../hooks/use-refresh-finance'
import { AlertCircle, RefreshCw } from './icons'
import { Button, EmptyState } from './ui'

export function ValueHistoryEmptyState({
  incomplete,
  valueAvailable,
}: {
  incomplete: boolean
  valueAvailable: boolean
}) {
  // Chart placeholders are one muted line, like every other empty state.
  return (
    <EmptyState>
      {incomplete
        ? 'Complete balance unavailable: history needs every account’s value.'
        : valueAvailable
          ? 'No history yet.'
          : 'No usable USD balance or history from the provider.'}
    </EmptyState>
  )
}

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

export function HomeLoading() {
  return (
    <div
      className="page loading-page home-loading"
      aria-busy="true"
      aria-label="Loading finance data"
    >
      <span className="sr-only" role="status">
        Opening saved finances
      </span>
      <div className="page-header">
        <div className="skeleton skeleton-label" />
      </div>
      <div className="home-primary-grid" aria-hidden="true">
        <div className="surface-card home-loading-chart">
          <div className="skeleton skeleton-label" />
          <div className="skeleton skeleton-value" />
        </div>
        <div className="surface-card home-loading-overview">
          {[0, 1, 2].map((key) => (
            <div className="skeleton skeleton-value" key={key} />
          ))}
        </div>
      </div>
      <div className="home-secondary-grid home-holdings-row" aria-hidden="true">
        {[0, 1, 2].map((key) => (
          <div className="surface-card skeleton-card" key={key} />
        ))}
      </div>
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
      <Button icon={RefreshCw} variant="primary" onClick={() => void query.refetch()}>
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
      data-command="finance.refresh"
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
