import { useQuery } from '@tanstack/react-query'

import { PageError, PageLoading } from '../components/data-state'
import { Card, EmptyState, ScrollCueCard, SectionHeading, StatusDot } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { useFinanceRefreshState } from '../hooks/use-refresh-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { getSyncRuns } from '../lib/api'
import type { SyncRun } from '../lib/schema'

const logTimeFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
  second: '2-digit',
})

const formatTime = (value?: string | number | null) => {
  if (!value || !Number.isFinite(new Date(value).getTime())) return 'Unknown'
  return logTimeFormatter.format(new Date(value))
}

const syncRunSummary = (run: SyncRun) => {
  const failure = run.details.providers.at(0)
  if (failure) {
    const reason = failure.errorCode ?? failure.errorType ?? failure.httpStatus ?? failure.kind
    const attempts = failure.attempts > 1 ? ` · ${failure.attempts} attempts` : ''
    const retry = failure.retryAt ? ` · retry after ${formatTime(failure.retryAt)}` : ''
    return `${failure.provider} · ${reason}${attempts}${retry}`
  }
  if (run.details.warningCount) return `${run.details.warningCount} warning(s)`
  return run.errorCode ? run.details.phase.replaceAll('_', ' ') : 'No warnings'
}

export function LogsPage() {
  const query = useFinance()
  const refreshing = useFinanceRefreshState()
  const data = query.data
  const live = useLiveFinance()
  const syncRuns = useQuery({ queryKey: ['sync-runs', data?.revision], queryFn: getSyncRuns })

  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const accountDisplayNames = getAccountDisplayNames()
  const connectedAccounts = {
    plaid: data.accounts.filter((account) => account.id.startsWith('plaid:')).length,
    snaptrade: data.accounts.filter((account) => account.id.startsWith('snaptrade:')).length,
  }
  const quoteUpdates = query.marketSymbols.map((ticker) => ({
    ticker,
    updatedAt: (live.data ?? data).holdings
      .filter((holding) => holding.ticker.trim().toUpperCase() === ticker)
      .flatMap((holding) => (holding.marketAsOf ? [holding.marketAsOf] : []))
      .toSorted()
      .at(-1),
  }))
  const connections = [
    {
      id: 'plaid',
      name: 'Plaid',
      configured: query.integrationStatus.plaid,
      accounts: connectedAccounts.plaid,
      health: data.providerStatus?.plaid,
    },
    {
      id: 'snaptrade',
      name: 'SnapTrade',
      configured: query.integrationStatus.snaptrade,
      accounts: connectedAccounts.snaptrade,
      health: data.providerStatus?.snaptrade,
    },
    {
      id: 'alpaca',
      name: 'Alpaca',
      configured: query.integrationStatus.alpaca,
      accounts: 0,
      health: undefined,
    },
    {
      id: 'alphavantage',
      name: 'Alpha Vantage',
      configured: query.integrationStatus.alphaVantage,
      accounts: 0,
      health: undefined,
    },
  ] as const
  const configuredCount = connections.filter(({ configured }) => configured).length
  const marketState = {
    active: 'Live',
    loading: 'Checking',
    paused: 'REST polling',
    error: 'Unavailable',
    idle: 'Idle',
  }[live.marketPriceState]

  return (
    <div className="page logs-page">
      <WorkspaceHeader
        title="Diagnostics"
        parent={{ label: 'Settings', to: '/settings' }}
        showSnapshot={false}
        showRefresh={false}
      />
      <div className="logs-grid">
        <Card className="logs-card">
          <SectionHeading
            title="Connections"
            action={
              <span className="logs-count">
                {configuredCount}/{connections.length} configured
              </span>
            }
          />
          <div className="connection-list">
            {connections.map(({ id, name, configured, accounts, health }) => {
              const error = health?.error
              const state = error
                ? 'Error'
                : configured
                  ? accounts
                    ? `${accounts} ${accounts === 1 ? 'account' : 'accounts'}`
                    : id === 'alpaca' || id === 'alphavantage'
                      ? 'Configured'
                      : 'Ready'
                  : 'Not configured'
              return (
                <div className="connection-row" key={id}>
                  <StatusDot tone={error ? 'negative' : configured ? 'positive' : 'neutral'} />
                  <div>
                    <strong>{name}</strong>
                    {error ? (
                      <small className="negative">{error}</small>
                    ) : health?.updatedAt ? (
                      <small>Checked {formatTime(health.updatedAt)}</small>
                    ) : null}
                  </div>
                  <span>{state}</span>
                </div>
              )
            })}
          </div>
        </Card>
        <Card className="logs-card">
          <SectionHeading title="Background activity" />
          <div className="runtime-list">
            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Account refresh</strong>
              </div>
              <div className="runtime-state">
                <StatusDot tone={refreshing ? 'positive' : 'neutral'} />
                <strong>{refreshing ? 'Running' : 'Idle'}</strong>
              </div>
            </div>
            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Market prices</strong>
                {live.marketPriceState === 'error' && live.marketPriceMessage ? (
                  <small>{live.marketPriceMessage}</small>
                ) : null}
              </div>
              <div className="runtime-state">
                <StatusDot
                  tone={
                    live.marketPriceState === 'error'
                      ? 'negative'
                      : live.marketPriceState === 'active'
                        ? 'positive'
                        : 'neutral'
                  }
                />
                <strong>
                  {live.marketIsSaved ? `Saved · ${marketState.toLowerCase()}` : marketState}
                </strong>
              </div>
            </div>
            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Snapshot saved</strong>
              </div>
              <time dateTime={data.updatedAt}>{formatTime(data.updatedAt)}</time>
            </div>
          </div>
        </Card>
      </div>
      <ScrollCueCard className="logs-card activity-log-card" scrollSelector=".activity-log">
        <SectionHeading title="Sync history" />
        <div className="activity-log">
          {syncRuns.data?.map((run) => (
            <div className="activity-log-row" key={run.id}>
              <time dateTime={run.finishedAt}>{formatTime(run.finishedAt)}</time>
              <span>Refresh</span>
              <StatusDot tone={run.outcome === 'committed' ? 'positive' : 'negative'} />
              <div>
                <strong>{run.outcome === 'committed' ? 'Committed' : 'Failed safely'}</strong>
                <small>{syncRunSummary(run)}</small>
              </div>
            </div>
          ))}
          {syncRuns.isPending ? <EmptyState>Loading sync history…</EmptyState> : null}
          {syncRuns.isError ? <EmptyState>Sync history is unavailable.</EmptyState> : null}
          {!syncRuns.isPending && !syncRuns.isError && !syncRuns.data?.length ? (
            <EmptyState>No refreshes recorded yet.</EmptyState>
          ) : null}
        </div>
      </ScrollCueCard>
      <ScrollCueCard className="logs-card activity-log-card" scrollSelector=".activity-log">
        <SectionHeading title="Runtime activity" />
        <div className="activity-log" aria-live="polite">
          {query.runtimeLog.length ? (
            query.runtimeLog.map((entry) => (
              <div className="activity-log-row" key={entry.id}>
                <time dateTime={entry.at}>{formatTime(entry.at)}</time>
                <span>{entry.source}</span>
                <StatusDot tone={entry.tone} />
                <div>
                  <strong>{entry.message}</strong>
                  {entry.detail ? <small>{entry.detail}</small> : null}
                </div>
              </div>
            ))
          ) : (
            <EmptyState>No runtime events recorded yet.</EmptyState>
          )}
        </div>
      </ScrollCueCard>
      <Card className="logs-card market-data-card">
        <SectionHeading title="Source timestamps" />
        <div className="settings-table-scroll">
          <table className="source-time-table">
            <thead>
              <tr>
                {['Account', 'Balance', 'Positions', 'Activity', 'Fetched'].map((label) => (
                  <th scope="col" key={label}>
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.accounts
                .filter(({ id }) => id !== 'all')
                .map((account) => (
                  <tr key={account.id}>
                    <th scope="row">
                      {accountDisplayName(account.id, account.name, accountDisplayNames)}
                    </th>
                    {[
                      account.balanceAsOf,
                      account.positionsAsOf,
                      account.activityAsOf,
                      account.balanceFetchedAt,
                    ].map((time, index) => (
                      <td key={index}>
                        <time dateTime={time ?? undefined}>{formatTime(time)}</time>
                      </td>
                    ))}
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {!data.accounts.some(({ id }) => id !== 'all') ? (
          <EmptyState>No account timestamps available.</EmptyState>
        ) : null}
      </Card>
      <Card className="logs-card market-data-card">
        <SectionHeading
          title="Quote timestamps"
          action={<span className="logs-count">{quoteUpdates.length} symbols</span>}
        />
        <div className="market-data-list">
          {quoteUpdates.map(({ ticker, updatedAt }) => (
            <div className="market-data-row" key={ticker}>
              <strong>{ticker}</strong>
              <time dateTime={updatedAt}>{formatTime(updatedAt)}</time>
            </div>
          ))}
          {!quoteUpdates.length ? (
            <EmptyState>No market symbols in the current snapshot.</EmptyState>
          ) : null}
        </div>
      </Card>
    </div>
  )
}
