import { useQuery } from '@tanstack/react-query'

import { PageError, PageLoading } from '../components/data-state'
import { Database, Link2, RefreshCw, TrendingUp } from '../components/icons'
import { Card, EmptyState, ScrollCueCard, SectionHeading, StatusDot } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { useFinanceRefreshState } from '../hooks/use-refresh-finance'
import { accountDisplayName } from '../lib/account-names'
import { getCalibration, getChangeLog, getSummaryRuns, getSyncRuns } from '../lib/api'
import { financeSettings } from '../lib/finance-settings'
import { countOf } from '../lib/format'
import { providerCatalog } from '../lib/providers'
import type { SummaryRun, SyncRun } from '../lib/schema'

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
  if (run.details.warningCount) return countOf(run.details.warningCount, 'warning')
  return run.errorCode ? run.details.phase.replaceAll('_', ' ') : 'No warnings'
}

const seconds = (millis: number) => `${(millis / 1_000).toFixed(1)} s`
const size = (bytes: number) =>
  bytes >= 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.ceil(bytes / 1_000)} KB`

// Each provider's time, requests, retries, and download size, then the stages and the snapshot
// the app received. Counts and timings only.
const syncRunDetail = (run: SyncRun) => {
  const { runs = [], stages, snapshotBytes, changes } = run.details
  const changed = changes
    ? [
        changes.added ? `${changes.added} new` : null,
        changes.settled ? `${changes.settled} settled` : null,
        changes.removed ? `${changes.removed} removed` : null,
        changes.amended ? `${changes.amended} amended` : null,
        changes.backfilled ? `${changes.backfilled} older` : null,
      ].filter(Boolean)
    : []
  return [
    changed.length ? changed.join(', ') : null,
    ...runs.map((provider) =>
      [
        provider.provider,
        provider.durationMs === undefined ? null : seconds(provider.durationMs),
        countOf(provider.requests, 'request'),
        provider.retries ? countOf(provider.retries, 'retry', 'retries') : null,
        provider.bytes ? size(provider.bytes) : null,
      ]
        .filter(Boolean)
        .join(' '),
    ),
    stages.fetchMs === undefined ? null : `fetch ${seconds(stages.fetchMs)}`,
    stages.projectMs === undefined ? null : `project ${seconds(stages.projectMs)}`,
    stages.commitMs === undefined ? null : `save ${seconds(stages.commitMs)}`,
    snapshotBytes === undefined ? null : `snapshot ${size(snapshotBytes)}`,
  ]
    .filter(Boolean)
    .join(' · ')
}

const summaryRunTitle = (run: SummaryRun) =>
  `${run.scope === 'portfolio' ? 'Portfolio' : run.scope} ${run.kind === 'explanation' ? 'explanation' : 'summary'}`

// The run's error, then each stage with its time, then the providers that failed.
const summaryRunDetail = (run: SummaryRun) =>
  [
    run.error,
    run.stages
      .map(
        (stage) =>
          `${stage.name} ${stage.millis} ms${stage.outcome === 'ok' ? '' : ` ${stage.outcome}`}${stage.detail && stage.outcome !== 'ok' ? ` (${stage.detail})` : ''}`,
      )
      .join(' · '),
    ...run.providers.map(
      (failure) =>
        `${failure.provider} ${failure.endpoint} · ${failure.httpStatus ?? failure.kind}`,
    ),
  ]
    .filter(Boolean)
    .join(' · ')

export function LogsPage() {
  const query = useFinance()
  const refreshing = useFinanceRefreshState()
  const data = query.data
  const live = useLiveFinance()
  const syncRuns = useQuery({ queryKey: ['sync-runs', data?.revision], queryFn: getSyncRuns })
  const summaryRuns = useQuery({ queryKey: ['summary-runs'], queryFn: getSummaryRuns })
  const calibration = useQuery({ queryKey: ['calibration'], queryFn: getCalibration })
  const changeLog = useQuery({ queryKey: ['change-log', data?.revision], queryFn: getChangeLog })

  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />

  const accountDisplayNames = financeSettings(data).accountNames
  const quoteUpdates = query.marketSymbols.map((ticker) => ({
    ticker,
    updatedAt: (live.data ?? data).holdings
      .filter((holding) => holding.ticker.trim().toUpperCase() === ticker)
      .flatMap((holding) => (holding.marketAsOf ? [holding.marketAsOf] : []))
      .toSorted()
      .at(-1),
  }))
  const connections = providerCatalog.map(({ id, name, status }) => ({
    id,
    name,
    configured: query.integrationStatus[status],
    accounts: data.accounts.filter((account) => account.id.startsWith(`${id}:`)).length,
    health: data.providerStatus?.[id],
  }))
  const configuredCount = connections.filter(({ configured }) => configured).length
  const marketState = {
    active: 'Live',
    loading: 'Checking',
    paused: 'REST polling',
    error: 'Unavailable',
    idle: 'Idle',
  }[live.marketPriceState]

  const lastRun = syncRuns.data?.[0]
  const connectionErrors = connections.filter(({ health }) => health?.error).length
  const marketLabel = live.marketIsSaved ? `Saved · ${marketState.toLowerCase()}` : marketState

  return (
    <div className="page logs-page">
      <WorkspaceHeader
        title="Diagnostics"
        parent={{ label: 'Settings', to: '/settings' }}
        showSnapshot={false}
        showRefresh={false}
      />
      <Card className="diagnostics-summary" aria-label="System status">
        <div className="diagnostics-stat">
          <span className="diagnostics-stat-label">
            <Link2 size={14} aria-hidden="true" />
            Connections
          </span>
          <span className="diagnostics-stat-value">
            <StatusDot
              tone={connectionErrors ? 'negative' : configuredCount ? 'positive' : 'neutral'}
            />
            <strong>
              {configuredCount} of {connections.length}
            </strong>
          </span>
          <small>{connectionErrors ? countOf(connectionErrors, 'error') : 'No errors'}</small>
        </div>
        <div className="diagnostics-stat">
          <span className="diagnostics-stat-label">
            <RefreshCw size={14} aria-hidden="true" />
            Account refresh
          </span>
          <span className="diagnostics-stat-value">
            <StatusDot
              tone={
                refreshing
                  ? 'positive'
                  : lastRun?.outcome === 'committed'
                    ? 'positive'
                    : lastRun
                      ? 'negative'
                      : 'neutral'
              }
            />
            <strong>
              {refreshing
                ? 'Running'
                : lastRun
                  ? lastRun.outcome === 'committed'
                    ? 'Committed'
                    : 'Failed safely'
                  : 'Idle'}
            </strong>
          </span>
          <small>{lastRun ? formatTime(lastRun.finishedAt) : 'No refreshes yet'}</small>
        </div>
        <div className="diagnostics-stat">
          <span className="diagnostics-stat-label">
            <TrendingUp size={14} aria-hidden="true" />
            Market prices
          </span>
          <span className="diagnostics-stat-value">
            <StatusDot
              tone={
                live.marketPriceState === 'error'
                  ? 'negative'
                  : live.marketPriceState === 'active'
                    ? 'positive'
                    : 'neutral'
              }
            />
            <strong>{marketLabel}</strong>
          </span>
          <small>
            {live.marketPriceState === 'error' && live.marketPriceMessage
              ? live.marketPriceMessage
              : countOf(quoteUpdates.length, 'symbol')}
          </small>
        </div>
        <div className="diagnostics-stat">
          <span className="diagnostics-stat-label">
            <Database size={14} aria-hidden="true" />
            Snapshot
          </span>
          <span className="diagnostics-stat-value">
            <strong>
              <time dateTime={data.updatedAt}>{formatTime(data.updatedAt)}</time>
            </strong>
          </span>
          <small>{data.revision != null ? `Revision ${data.revision}` : 'Saved locally'}</small>
        </div>
      </Card>
      <div className="logs-grid">
        <Card className="logs-card">
          <SectionHeading
            title="Connections"
            action={
              <span className="logs-count">
                {configuredCount} of {connections.length} configured
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
                    ? countOf(accounts, 'account')
                    : id === 'plaid' || id === 'snaptrade'
                      ? 'Ready'
                      : 'Configured'
                  : 'Not configured'
              return (
                <div className="connection-row" key={id}>
                  <div>
                    <strong>{name}</strong>
                    {error ? (
                      <small className="negative">{error}</small>
                    ) : health?.updatedAt ? (
                      <small>Checked {formatTime(health.updatedAt)}</small>
                    ) : null}
                  </div>
                  <span className="connection-state">
                    <StatusDot tone={error ? 'negative' : configured ? 'positive' : 'neutral'} />
                    {state}
                  </span>
                </div>
              )
            })}
          </div>
        </Card>
        <Card className="logs-card">
          <SectionHeading
            title="Quote timestamps"
            action={<span className="logs-count">{countOf(quoteUpdates.length, 'symbol')}</span>}
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
        <ScrollCueCard className="logs-card" scrollSelector=".activity-log">
          <SectionHeading title="Sync history" />
          <div className="activity-log">
            {syncRuns.data?.map((run) => (
              <div className="activity-log-row" key={run.id}>
                <StatusDot tone={run.outcome === 'committed' ? 'positive' : 'negative'} />
                <div>
                  <strong>{run.outcome === 'committed' ? 'Committed' : 'Failed safely'}</strong>
                  <small>{syncRunSummary(run)}</small>
                  {syncRunDetail(run) ? <small>{syncRunDetail(run)}</small> : null}
                </div>
                <time dateTime={run.finishedAt}>{formatTime(run.finishedAt)}</time>
              </div>
            ))}
            {syncRuns.isPending ? <EmptyState>Loading sync history…</EmptyState> : null}
            {syncRuns.isError ? <EmptyState>Sync history is unavailable.</EmptyState> : null}
            {!syncRuns.isPending && !syncRuns.isError && !syncRuns.data?.length ? (
              <EmptyState>No refreshes recorded yet.</EmptyState>
            ) : null}
          </div>
        </ScrollCueCard>
        <ScrollCueCard className="logs-card" scrollSelector=".activity-log">
          <SectionHeading title="Summary runs" />
          <div className="activity-log">
            {summaryRuns.data?.map((run) => (
              <div className="activity-log-row" key={run.id}>
                <StatusDot
                  tone={
                    run.outcome === 'ok'
                      ? 'positive'
                      : run.outcome === 'partial'
                        ? 'warning'
                        : 'negative'
                  }
                />
                <div>
                  <strong>
                    {`${summaryRunTitle(run)} · ${run.outcome === 'ok' ? 'Finished' : run.outcome === 'partial' ? 'Partial' : 'Failed'}`}
                  </strong>
                  <small>{summaryRunDetail(run)}</small>
                </div>
                <time dateTime={run.finishedAt}>{formatTime(run.finishedAt)}</time>
              </div>
            ))}
            {summaryRuns.isError ? <EmptyState>Summary runs are unavailable.</EmptyState> : null}
            {!summaryRuns.isPending && !summaryRuns.isError && !summaryRuns.data?.length ? (
              <EmptyState>No summaries made since launch.</EmptyState>
            ) : null}
          </div>
        </ScrollCueCard>
        <ScrollCueCard className="logs-card" scrollSelector=".activity-log">
          <SectionHeading
            title="Calibration"
            action={
              calibration.data?.sessions ? (
                <span className="logs-count">{countOf(calibration.data.sessions, 'session')}</span>
              ) : undefined
            }
          />
          <div className="activity-log">
            {calibration.data?.sessions ? (
              <>
                <div className="activity-log-row">
                  <StatusDot tone="neutral" />
                  <div>
                    <strong>Per session</strong>
                    <small>
                      {`${calibration.data.fundGroups.toFixed(2)} fund groups · ${calibration.data.looseGroups.toFixed(2)} loose groups · ${calibration.data.notable.toFixed(2)} unusual own moves`}
                    </small>
                  </div>
                  <span>
                    {calibration.data.first && calibration.data.last
                      ? `${calibration.data.first} to ${calibration.data.last}`
                      : null}
                  </span>
                </div>
                <div className="activity-log-row">
                  <StatusDot tone="neutral" />
                  <div>
                    <strong>Quiet sessions</strong>
                    <small>Nothing flagged and the portfolio within a typical day</small>
                  </div>
                  <span>{calibration.data.quiet}</span>
                </div>
                {calibration.data.perFund.map((rate) => (
                  <div className="activity-log-row" key={rate.fund}>
                    <StatusDot tone="neutral" />
                    <div>
                      <strong>{rate.fund}</strong>
                      <small>
                        {`${rate.flags} flags in ${countOf(rate.sessions, 'session')} with members`}
                      </small>
                    </div>
                    <span>{`${rate.perMonth.toFixed(2)} a month`}</span>
                  </div>
                ))}
              </>
            ) : calibration.isError ? (
              <EmptyState>Calibration is unavailable.</EmptyState>
            ) : (
              <EmptyState>No sessions saved yet.</EmptyState>
            )}
          </div>
        </ScrollCueCard>
        <Card className="logs-card">
          <SectionHeading
            title="Change log"
            action={
              changeLog.data ? (
                <span className="logs-count">
                  {countOf(changeLog.data.summary.changesets, 'change')}
                </span>
              ) : undefined
            }
          />
          <div className="activity-log">
            {changeLog.data ? (
              <>
                <div className="activity-log-row">
                  <StatusDot
                    tone={
                      !changeLog.data.check
                        ? 'neutral'
                        : changeLog.data.check.outcome === 'matches'
                          ? 'positive'
                          : 'negative'
                    }
                  />
                  <div>
                    <strong>
                      {!changeLog.data.check
                        ? 'Replay not checked yet'
                        : changeLog.data.check.outcome === 'matches'
                          ? 'Replays to your saved data'
                          : changeLog.data.check.outcome === 'differs'
                            ? 'Replay differs from your saved data'
                            : 'Replay could not run'}
                    </strong>
                    <small>
                      {changeLog.data.check?.detail ??
                        (changeLog.data.check
                          ? `${countOf(changeLog.data.check.replayed, 'change')} replayed`
                          : 'Checked a minute after launch')}
                    </small>
                  </div>
                  {changeLog.data.check ? (
                    <time dateTime={changeLog.data.check.checkedAt}>
                      {formatTime(changeLog.data.check.checkedAt)}
                    </time>
                  ) : null}
                </div>
                <div className="activity-log-row">
                  <StatusDot tone="neutral" />
                  <div>
                    <strong>Stored</strong>
                    <small>
                      {`${countOf(changeLog.data.summary.checkpoints, 'checkpoint')} · ${countOf(changeLog.data.summary.sinceCheckpoint, 'change')} since the latest`}
                    </small>
                  </div>
                  <span>{size(changeLog.data.summary.storedBytes)}</span>
                </div>
                {changeLog.data.summary.firstRecordedAt ? (
                  <div className="activity-log-row">
                    <StatusDot tone="neutral" />
                    <div>
                      <strong>Kept since</strong>
                      <small>Every refresh, import, and edit since then</small>
                    </div>
                    <time dateTime={changeLog.data.summary.firstRecordedAt}>
                      {formatTime(changeLog.data.summary.firstRecordedAt)}
                    </time>
                  </div>
                ) : null}
              </>
            ) : changeLog.isError ? (
              <EmptyState>The change log is unavailable.</EmptyState>
            ) : (
              <EmptyState>The change log is kept in the app.</EmptyState>
            )}
          </div>
        </Card>
        <ScrollCueCard className="logs-card" scrollSelector=".activity-log">
          <SectionHeading title="Runtime activity" />
          <div className="activity-log" aria-live="polite">
            {query.runtimeLog.length ? (
              query.runtimeLog.map((entry) => (
                <div className="activity-log-row" key={entry.id}>
                  <StatusDot tone={entry.tone} />
                  <div>
                    <strong>{entry.message}</strong>
                    <small>
                      {entry.source}
                      {entry.detail ? ` · ${entry.detail}` : ''}
                    </small>
                  </div>
                  <time dateTime={entry.at}>{formatTime(entry.at)}</time>
                </div>
              ))
            ) : (
              <EmptyState>No runtime events recorded yet.</EmptyState>
            )}
          </div>
        </ScrollCueCard>
      </div>
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
    </div>
  )
}
