import { useQuery } from '@tanstack/react-query'

import { PageError, PageLoading } from '../components/data-state'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useFinanceRefreshState } from '../hooks/use-refresh-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { getSyncRuns } from '../lib/api'
import type { MarketSnapshots } from '../lib/schema'

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

export function LogsPage() {
  const query = useFinance()
  const refreshing = useFinanceRefreshState()
  const data = query.data
  const quotes = useQuery<MarketSnapshots>({
    queryKey: ['market-snapshots', query.marketSymbols],
    enabled: false,
  })
  const syncRuns = useQuery({ queryKey: ['sync-runs', data?.revision], queryFn: getSyncRuns })

  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const accountDisplayNames = getAccountDisplayNames()
  const configuredCount = Object.values(query.integrationStatus).filter(Boolean).length
  const activeCount = Number(refreshing)
  const connectedAccounts = {
    plaid: data.accounts.filter((account) => account.id.startsWith('plaid:')).length,
    snaptrade: data.accounts.filter((account) => account.id.startsWith('snaptrade:')).length,
  }
  const quoteUpdates = query.marketSymbols.map((ticker) => ({
    ticker,
    updatedAt:
      quotes.data?.snapshots[ticker]?.asOf ??
      data.holdings
        .filter((holding) => holding.ticker.trim().toUpperCase() === ticker)
        .flatMap((holding) => (holding.marketAsOf ? [holding.marketAsOf] : []))
        .toSorted()
        .at(-1),
  }))
  const connections = [
    {
      id: 'plaid',
      name: 'Plaid',
      detail: 'Banking and stock plan data',
      configured: query.integrationStatus.plaid,
      accounts: connectedAccounts.plaid,
      health: data.providerStatus?.plaid,
    },
    {
      id: 'snaptrade',
      name: 'SnapTrade',
      detail: 'Brokerage positions and activity',
      configured: query.integrationStatus.snaptrade,
      accounts: connectedAccounts.snaptrade,
      health: data.providerStatus?.snaptrade,
    },
    {
      id: 'alpaca',
      name: 'Alpaca',
      detail: 'Market history and session-aware snapshots',
      configured: query.integrationStatus.alpaca,
      accounts: 0,
      health: undefined,
    },
  ] as const

  return (
    <div className="page logs-page">
      <header className="page-header logs-header">
        <h1>Logs</h1>
        <span className="freshness">
          <StatusDot tone={activeCount ? 'positive' : 'neutral'} />
          {activeCount ? `${activeCount} active` : 'Quiet'}
        </span>
      </header>

      <div className="logs-grid">
        <Card className="logs-card">
          <SectionHeading title="Background activity" />
          <div className="runtime-list">
            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Market snapshot checks</strong>
                <small>Session-aware on Home · stopped here</small>
              </div>
              <div className="runtime-state">
                <StatusDot tone="neutral" />
                <strong>Stopped</strong>
              </div>
            </div>

            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Account refresh</strong>
                <small>Manual · last saved {formatTime(data.updatedAt)}</small>
              </div>
              <div className="runtime-state">
                <StatusDot tone={refreshing ? 'positive' : 'neutral'} />
                <strong>{refreshing ? 'Running' : 'Idle'}</strong>
              </div>
            </div>

            <div className="runtime-row">
              <div className="runtime-copy">
                <strong>Live chart history</strong>
                <small>Session-local and active only while Home is open</small>
              </div>
              <div className="runtime-state">
                <StatusDot tone="neutral" />
                <strong>Stopped</strong>
              </div>
            </div>
          </div>
        </Card>

        <Card className="logs-card">
          <SectionHeading title={`Connections · ${configuredCount}/3`} />
          <div className="connection-list">
            {connections.map(({ id, name, detail, configured, accounts, health }) => {
              const error = health?.error
              const state = error
                ? 'Error'
                : configured
                  ? accounts
                    ? `${accounts} ${accounts === 1 ? 'account' : 'accounts'}`
                    : id === 'alpaca'
                      ? 'Configured'
                      : 'Ready'
                  : 'Not configured'
              return (
                <div className="connection-row" key={id}>
                  <StatusDot tone={error ? 'negative' : configured ? 'positive' : 'neutral'} />
                  <div>
                    <strong>{name}</strong>
                    <small>
                      {error
                        ? error
                        : health?.updatedAt
                          ? `Last checked ${formatTime(health.updatedAt)}`
                          : id === 'alpaca' && configured
                            ? `${query.marketSymbols.length} symbols · route scoped`
                            : detail}
                    </small>
                  </div>
                  <span>{state}</span>
                </div>
              )
            })}
          </div>
        </Card>
      </div>

      <Card className="logs-card market-data-card">
        <SectionHeading
          title="Source timestamps"
          detail="When providers last obtained the underlying data. A successful check does not imply fresh data."
        />
        {data.accounts
          .filter(({ id }) => id !== 'all')
          .map((account) => (
            <div className="runtime-row" key={account.id}>
              <div className="runtime-copy">
                <strong>{accountDisplayName(account.id, account.name, accountDisplayNames)}</strong>
                <small>
                  Balance: {formatTime(account.balanceAsOf)} · Positions:{' '}
                  {formatTime(account.positionsAsOf)} · Activity: {formatTime(account.activityAsOf)}
                  {' · '}Fetched by Brief: {formatTime(account.balanceFetchedAt)}
                </small>
              </div>
            </div>
          ))}
      </Card>

      <Card className="logs-card market-data-card">
        <SectionHeading
          title={`Market data · ${quoteUpdates.length}`}
          detail="Latest quote time stored for each symbol"
        />
        <div className="market-data-list">
          {quoteUpdates.map(({ ticker, updatedAt }) => (
            <div className="market-data-row" key={ticker}>
              <strong>{ticker}</strong>
              <time dateTime={updatedAt}>{formatTime(updatedAt)}</time>
            </div>
          ))}
          {!quoteUpdates.length ? (
            <p className="activity-log-empty">No market symbols in the current snapshot.</p>
          ) : null}
        </div>
      </Card>

      <Card className="logs-card activity-log-card">
        <SectionHeading
          title="Durable sync history"
          detail="Redacted outcomes stored locally; no credentials or financial values are recorded"
        />
        <div className="activity-log">
          {syncRuns.data?.map((run) => (
            <div className="activity-log-row" key={run.id}>
              <time dateTime={run.finishedAt}>{formatTime(run.finishedAt)}</time>
              <span>Refresh</span>
              <StatusDot tone={run.outcome === 'committed' ? 'positive' : 'negative'} />
              <div>
                <strong>{run.outcome === 'committed' ? 'Committed' : 'Failed safely'}</strong>
                <small>
                  {run.errorCode ??
                    (run.warnings.length ? `${run.warnings.length} warning(s)` : 'No warnings')}
                </small>
              </div>
            </div>
          ))}
          {syncRuns.isError ? (
            <p className="activity-log-empty">Durable sync history is unavailable.</p>
          ) : null}
          {!syncRuns.isPending && !syncRuns.isError && !syncRuns.data?.length ? (
            <p className="activity-log-empty">No durable sync runs recorded yet.</p>
          ) : null}
        </div>
      </Card>

      <Card className="logs-card activity-log-card">
        <SectionHeading title="Recent activity" />
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
            <p className="activity-log-empty">No runtime events recorded yet.</p>
          )}
        </div>
      </Card>
    </div>
  )
}
