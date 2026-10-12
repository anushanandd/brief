import { Popover } from '@base-ui/react/popover'
import { Link } from '@tanstack/react-router'
import { Fragment, useMemo, useState, type ReactNode } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { useFinanceRefreshState } from '../hooks/use-refresh-finance'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { AnalyticsSearch, AnalyticsChart } from '../lib/analytics'
import { formatUpdatedAt } from '../lib/format'
import {
  marketUpdateIntervals,
  saveMarketUpdateInterval,
  useMarketUpdateInterval,
} from '../lib/market-preferences'
import { providerCatalog } from '../lib/providers'
import type { IntegrationStatus } from '../lib/schema'
import type { SpendingRange } from '../lib/spending'
import { RefreshButton } from './data-state'
import { type IconComponent, Moon, Sun, Sunrise, Sunset, TrendingUp } from './icons'
import { SettingsLink } from './settings-dialog'
import { ChartRangeSelect, StatusDot } from './ui'
import { usePopupPortal } from './ui/popup-container'

type WorkspaceParent = {
  label: string
  to: '/' | '/accounts' | '/spending' | '/activities' | '/settings'
}

export type WorkspaceBreadcrumb = {
  label: string
  to:
    | '/accounts'
    | '/activities'
    | '/holdings'
    | '/analytics'
    | '/spending'
    | '/recurring'
    | '/taxes'
  search?: Omit<AnalyticsSearch, 'range'> & {
    range?: AnalyticsSearch['range'] | SpendingRange
    category?: string
    method?: string
    ticker?: string
    analysis?: AnalyticsChart
  }
}

export function WorkspaceHeader({
  title,
  parent,
  breadcrumbs,
  detail,
  status,
  actions,
  showSnapshot = true,
  showRefresh = true,
}: {
  title: string
  parent?: WorkspaceParent
  breadcrumbs?: WorkspaceBreadcrumb[]
  detail?: string
  status?: ReactNode
  actions?: ReactNode
  showSnapshot?: boolean
  showRefresh?: boolean
}) {
  const data = useFinance().data
  const hasWarning = Boolean(
    data?.recovery ||
    data?.syncWarnings?.length ||
    Object.values(data?.providerStatus ?? {}).some(({ error }) => error),
  )

  return (
    <header className="page-header workspace-header">
      <div className="workspace-title">
        <h1 className={parent || breadcrumbs?.length ? 'page-route' : undefined}>
          {breadcrumbs?.length ? (
            breadcrumbs.map((crumb, index) => (
              <Fragment key={`${index}-${crumb.label}`}>
                {index > 0 ? (
                  <span className="page-route-separator" aria-hidden="true">
                    /
                  </span>
                ) : null}
                {index === breadcrumbs.length - 1 ? (
                  <span aria-current="page">{crumb.label}</span>
                ) : (
                  <Link
                    to={crumb.to}
                    search={crumb.search ?? {}}
                    activeOptions={{ exact: true }}
                    data-command={
                      crumb.to === '/analytics' && crumb.search?.chart
                        ? `navigation.report.${crumb.search.chart}`
                        : `navigation.open.${crumb.to}`
                    }
                  >
                    {crumb.label}
                  </Link>
                )}
              </Fragment>
            ))
          ) : parent ? (
            <>
              {parent.to === '/settings' ? (
                <SettingsLink target="advanced">{parent.label}</SettingsLink>
              ) : (
                <Link
                  to={parent.to}
                  activeOptions={{ exact: true }}
                  data-command={`navigation.open.${parent.to}`}
                >
                  {parent.label}
                </Link>
              )}
              <span className="page-route-separator" aria-hidden="true">
                /
              </span>
              <span aria-current="page">{title}</span>
            </>
          ) : (
            title
          )}
        </h1>
        {detail ? <p>{detail}</p> : null}
      </div>
      <div className="workspace-actions">
        {status || (showSnapshot && data?.updatedAt) ? (
          // The header status only reports market state, so the popover's market detail follows it.
          <DataStatus showMarket={Boolean(status)}>
            {status}
            {showSnapshot && data?.updatedAt ? (
              <span className={`freshness snapshot-freshness${hasWarning ? ' warning' : ''}`}>
                {status ? '· ' : null}
                <span>
                  Saved <time dateTime={data.updatedAt}>{formatUpdatedAt(data.updatedAt)}</time>
                  {hasWarning ? ' · Data needs attention' : ''}
                </span>
              </span>
            ) : null}
          </DataStatus>
        ) : null}
        {actions}
        {showRefresh ? <RefreshButton /> : null}
      </div>
    </header>
  )
}

export function marketSessionLabel(session: string | undefined, now = new Date()) {
  if (session !== 'Market closed') return session
  const weekday = new Intl.DateTimeFormat('en-US', {
    weekday: 'short',
    timeZone: 'America/New_York',
  }).format(now)
  return weekday === 'Sat' || weekday === 'Sun' ? 'Weekend' : session
}

const sessionIcons: Record<string, IconComponent> = {
  'Pre-market': Sunrise,
  'Regular market': Sun,
  'After hours': Sunset,
  Overnight: Moon,
  'Market closed': Moon,
}

export function MarketStatus() {
  const query = useLiveFinance()
  const marketIsActive = query.marketPriceState === 'active'
  const marketIsLoading = query.marketPriceState === 'loading'
  const session = marketSessionLabel(query.marketSession)
  const SessionIcon = sessionIcons[query.marketSession ?? ''] ?? TrendingUp
  return (
    <span
      className="freshness market-freshness"
      data-tone={
        marketIsActive ? 'positive' : query.marketPriceState === 'error' ? 'negative' : 'neutral'
      }
      aria-live="polite"
    >
      <SessionIcon size={12} aria-hidden="true" />
      {marketIsActive
        ? (session ?? 'Live prices')
        : marketIsLoading
          ? query.marketIsSaved
            ? 'Saved prices · updating'
            : 'Checking market prices'
          : query.marketPriceState === 'error'
            ? 'Prices unavailable'
            : (session ?? 'Prices saved')}
    </span>
  )
}

type ConnectionHealth = { updatedAt: string | null; error: string | null }

/** One row per provider: configured providers report their last check or that they need attention. */
export function connectionStates(
  integrations: IntegrationStatus,
  health: Record<string, ConnectionHealth> = {},
  marketError = false,
) {
  return providerCatalog.map(({ id, name, status }) => {
    const provider = health[id]
    const configured = integrations[status]
    const failed = Boolean(provider?.error) || (id === 'alpaca' && marketError)
    return {
      id,
      name,
      tone: !configured ? 'neutral' : failed ? 'negative' : 'positive',
      state: !configured ? 'Not set up' : failed ? 'Needs attention' : 'Connected',
      checkedAt: configured && !failed ? (provider?.updatedAt ?? undefined) : undefined,
    }
  })
}

export function marketPriceLabel(
  state: 'active' | 'loading' | 'error' | 'idle',
  saved: boolean,
  enabled: boolean,
) {
  if (!enabled) return 'Off'
  if (state === 'active') return 'Live'
  if (state === 'error') return 'Unavailable'
  if (state === 'loading') return saved ? 'Saved · updating' : 'Checking'
  return saved ? 'Saved' : 'Not updating'
}

const timeFormatter = new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: '2-digit' })

function DataStatus({ showMarket, children }: { showMarket: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(false)
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'status.open',
          title: 'Show data status',
          search: 'freshness saved prices connections providers health',
          group: 'View',
          scope: 'page',
          run: () => setOpen(true),
        },
      ],
      [],
    ),
  )
  const { container, positionMethod } = usePopupPortal()
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        className="workspace-status"
        aria-label="Data status"
        data-command="status.open"
      >
        {children}
      </Popover.Trigger>
      <Popover.Portal container={container}>
        <Popover.Positioner
          side="bottom"
          align="end"
          sideOffset={6}
          positionMethod={positionMethod}
          className="data-status-positioner"
        >
          <Popover.Popup className="data-status-popup">
            <Popover.Title className="sr-only">Data status</Popover.Title>
            {open ? (
              <DataStatusDetail showMarket={showMarket} close={() => setOpen(false)} />
            ) : null}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}

function DataStatusDetail({ showMarket, close }: { showMarket: boolean; close: () => void }) {
  const live = useLiveFinance()
  const refreshing = useFinanceRefreshState()
  const marketUpdateInterval = useMarketUpdateInterval()
  const data = live.data
  const warnings = [
    ...(data?.recovery ? [data.recovery.message] : []),
    ...(data?.syncWarnings ?? []),
    ...(live.annotationWarning ? [live.annotationWarning] : []),
  ]
  const connections = connectionStates(
    live.integrationStatus,
    data?.providerStatus,
    live.marketPriceState === 'error',
  )
  const session = marketSessionLabel(live.marketSession)
  const SessionIcon = sessionIcons[live.marketSession ?? ''] ?? TrendingUp

  return (
    <>
      {showMarket ? (
        <section className="data-status-section" aria-labelledby="data-status-market">
          <h3 id="data-status-market">Market</h3>
          <dl className="data-status-rows">
            {session ? (
              <div>
                <dt>Session</dt>
                <dd>
                  <SessionIcon size={12} aria-hidden="true" />
                  {session}
                  {live.marketNextTransitionAt ? (
                    <span className="data-status-muted">
                      until{' '}
                      <time dateTime={live.marketNextTransitionAt}>
                        {timeFormatter.format(new Date(live.marketNextTransitionAt))}
                      </time>
                    </span>
                  ) : null}
                </dd>
              </div>
            ) : null}
            <div>
              <dt>Prices</dt>
              <dd data-tone={live.marketPriceState === 'error' ? 'negative' : undefined}>
                {marketPriceLabel(
                  live.marketPriceState,
                  live.marketIsSaved,
                  live.liveMarketEnabled,
                )}
              </dd>
            </div>
            {live.marketFeedLabel ? (
              <div>
                <dt>Feed</dt>
                <dd>
                  {live.marketFeedLabel}
                  {live.marketDelayMinutes ? (
                    <span className="data-status-muted">
                      {live.marketDelayMinutes}-minute delay
                    </span>
                  ) : null}
                </dd>
              </div>
            ) : null}
            {live.marketAsOf ? (
              <div>
                <dt>Observed</dt>
                <dd>
                  <time dateTime={live.marketAsOf}>{formatUpdatedAt(live.marketAsOf)}</time>
                </dd>
              </div>
            ) : null}
          </dl>
          {live.marketPriceMessage ? (
            <p className="data-status-note">{live.marketPriceMessage}</p>
          ) : !live.liveMarketEnabled ? (
            <p className="data-status-note">
              {data?.recovery
                ? 'Paused during recovery.'
                : 'Live prices need an Alpaca connection.'}
            </p>
          ) : null}
          {live.liveMarketEnabled ? (
            <div className="data-status-setting">
              <span>Price updates</span>
              <ChartRangeSelect
                label="Live price updates"
                options={marketUpdateIntervals.map(({ seconds, settingsLabel }) => ({
                  value: seconds,
                  label: settingsLabel,
                  accessibleLabel: settingsLabel,
                }))}
                value={marketUpdateInterval}
                onValueChange={saveMarketUpdateInterval}
              />
            </div>
          ) : null}
        </section>
      ) : null}

      <section className="data-status-section" aria-labelledby="data-status-snapshot">
        <h3 id="data-status-snapshot">Accounts</h3>
        <dl className="data-status-rows">
          <div>
            <dt>Saved</dt>
            <dd>
              {refreshing ? (
                'Refreshing…'
              ) : data?.updatedAt ? (
                <time dateTime={data.updatedAt}>{formatUpdatedAt(data.updatedAt)}</time>
              ) : (
                'Unavailable'
              )}
            </dd>
          </div>
        </dl>
        {warnings.length ? (
          <ul className="data-status-warnings">
            {warnings.map((warning, index) => (
              <li key={`${index}-${warning}`}>{warning}</li>
            ))}
          </ul>
        ) : null}
      </section>

      <section className="data-status-section" aria-labelledby="data-status-connections">
        <h3 id="data-status-connections">Connections</h3>
        <ul className="data-status-connections">
          {connections.map((connection) => (
            <li key={connection.id} data-configured={connection.tone !== 'neutral' || undefined}>
              <StatusDot tone={connection.tone} />
              <span>{connection.name}</span>
              <span className="data-status-muted">
                {connection.checkedAt ? (
                  <>
                    Checked{' '}
                    <time dateTime={connection.checkedAt}>
                      {formatUpdatedAt(connection.checkedAt)}
                    </time>
                  </>
                ) : (
                  connection.state
                )}
              </span>
            </li>
          ))}
        </ul>
      </section>

      <div className="data-status-links" onClick={close}>
        <SettingsLink target="connections">Manage connections</SettingsLink>
        <Link to="/logs">Diagnostics</Link>
      </div>
    </>
  )
}
