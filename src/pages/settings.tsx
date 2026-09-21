import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  Cpu,
  Database,
  Fingerprint,
  HardDrive,
  Link2,
  Palette,
  RefreshCw,
  Save,
  ScrollText,
  Trash2,
  Unlink,
  Wrench,
  X,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'

import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { FilterSelect } from '../components/filter-select'
import { Button, Card, RangeSelector, SectionHeading, StatusDot } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useRefreshFinance } from '../hooks/use-refresh-finance'
import {
  accountDisplayName,
  getAccountDisplayNames,
  saveAccountDisplayNames,
} from '../lib/account-name-preferences'
import {
  accountStartDate,
  getAccountStartDates,
  saveAccountStartDates,
} from '../lib/account-start-date-preferences'
import {
  authenticateSensitiveAction,
  beginProviderLink,
  cancelProviderLink,
  type ProviderLinkSession,
  getFoundationModelStatus,
  isTauri,
  pollProviderLink,
  saveIntegrationCredentials,
  saveAccountLink,
  getProviderConnections,
  forgetProviderConnection,
} from '../lib/api'
import { dashboardAccountViews } from '../lib/dashboard-account-views'
import { formatCurrency } from '../lib/format'
import {
  getChartAccountPreferences,
  getDefaultGraphWindow,
  graphWindows,
  reconcileChartAccountPreferences,
  saveChartAccountPreferences,
  saveDefaultGraphWindow,
} from '../lib/graph-preferences'
import {
  getDefaultHoldingChartRange,
  holdingChartRanges,
  saveDefaultHoldingChartRange,
} from '../lib/holding-prices'
import { getExternalLogosEnabled, saveExternalLogosEnabled } from '../lib/logos'
import {
  getMarketUpdateInterval,
  marketUpdateIntervals,
  saveMarketUpdateInterval,
} from '../lib/market-preferences'
import { movePage, savePageOrder, useOrderedNavigation } from '../lib/navigation-preferences'
import { platinumBenefitOptions } from '../lib/spending'
import {
  getHiddenPlatinumBenefitIds,
  getSpendingAccountId,
  saveHiddenPlatinumBenefitIds,
  saveSpendingAccountId,
} from '../lib/spending-preferences'

const wait = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration))
type CredentialProvider = 'plaid' | 'snaptrade' | 'alpaca' | 'alphavantage'
type LinkProvider = 'plaid' | 'plaid-investments' | 'snaptrade'
const credentialProviders: Array<{
  id: CredentialProvider
  name: string
  fields: Array<{ label: string; placeholder: string; secret?: boolean }>
}> = [
  {
    id: 'plaid',
    name: 'Plaid',
    fields: [
      { label: 'Client ID', placeholder: 'Plaid client ID' },
      { label: 'Secret', placeholder: 'Plaid secret' },
    ],
  },
  {
    id: 'snaptrade',
    name: 'SnapTrade',
    fields: [
      { label: 'Client ID', placeholder: 'SnapTrade client ID' },
      { label: 'Consumer key', placeholder: 'Consumer key' },
    ],
  },
  {
    id: 'alpaca',
    name: 'Alpaca',
    fields: [
      { label: 'API key ID', placeholder: 'Alpaca API key ID' },
      { label: 'Secret key', placeholder: 'Alpaca secret key' },
    ],
  },
  {
    id: 'alphavantage',
    name: 'Alpha Vantage',
    fields: [{ label: 'API key', placeholder: 'Alpha Vantage API key', secret: true }],
  },
]
const providerName = (provider: CredentialProvider) =>
  credentialProviders.find(({ id }) => id === provider)?.name ?? provider
const isLinkProvider = (provider: string): provider is LinkProvider =>
  provider === 'plaid' || provider === 'plaid-investments' || provider === 'snaptrade'
const linkProviderName = (provider: LinkProvider) =>
  provider === 'plaid-investments'
    ? 'Plaid Investments'
    : provider === 'plaid'
      ? 'Plaid'
      : 'SnapTrade'

const dataSources = [
  { id: 'plaid', name: 'Plaid', description: 'Bank and credit card accounts' },
  {
    id: 'plaid-investments',
    name: 'Plaid Investments',
    description: 'Brokerage and stock plan accounts',
  },
  { id: 'snaptrade', name: 'SnapTrade', description: 'Investment accounts' },
  { id: 'alpaca', name: 'Alpaca', description: 'Market quotes' },
  { id: 'alphavantage', name: 'Alpha Vantage', description: 'Earnings dates, news and sentiment' },
  {
    id: 'intelligence',
    name: 'Apple Intelligence',
    description: 'Private, on-device explanations',
  },
  { id: 'local', name: 'Local cache', description: 'On-device financial snapshot' },
] as const

export function SettingsPage() {
  const queryClient = useQueryClient()
  const query = useFinance()
  const refresh = useRefreshFinance()
  const connections = useQuery({
    queryKey: ['provider-connections', query.data?.revision],
    queryFn: getProviderConnections,
  })
  const orderedPages = useOrderedNavigation()
  const [forgettingItem, setForgettingItem] = useState<string | null>(null)
  const forgetConnection = useMutation({
    mutationFn: forgetProviderConnection,
    onSuccess: (_, itemId) => {
      queryClient.setQueriesData<NonNullable<typeof connections.data>>(
        { queryKey: ['provider-connections'] },
        (current) => current?.filter((connection) => connection.itemId !== itemId),
      )
      setForgettingItem(null)
      toast.success('Connection forgotten locally. Refresh to update accounts.')
      void queryClient.invalidateQueries({ queryKey: ['provider-connections'] })
    },
    onError: (error) => {
      toast.error('Could not forget connection', {
        description: error instanceof Error ? error.message : String(error),
      })
    },
  })
  const [linking, setLinking] = useState<LinkProvider | null>(null)
  const foundationModel = useQuery({
    queryKey: ['foundation-model-status'],
    queryFn: getFoundationModelStatus,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const [integrationSaving, setIntegrationSaving] = useState<CredentialProvider | null>(null)
  const [credentialsUnlocked, setCredentialsUnlocked] = useState(false)
  const [credentialsUnlocking, setCredentialsUnlocking] = useState(false)
  const [defaultGraphWindow, setDefaultGraphWindow] = useState(getDefaultGraphWindow)
  const [holdingChartRange, setHoldingChartRange] = useState(getDefaultHoldingChartRange)
  const [marketUpdateInterval, setMarketUpdateInterval] = useState(getMarketUpdateInterval)
  const [savedChartAccountPreferences, setSavedChartAccountPreferences] = useState(
    getChartAccountPreferences,
  )
  const [accountDisplayNames, setAccountDisplayNames] = useState(getAccountDisplayNames)
  const [accountStartDates, setAccountStartDates] = useState(getAccountStartDates)
  const [externalLogosEnabled, setExternalLogosEnabled] = useState(getExternalLogosEnabled)
  const [spendingAccountId, setSpendingAccountId] = useState(getSpendingAccountId)
  const [hiddenPlatinumBenefitIds, setHiddenPlatinumBenefitIds] = useState(
    getHiddenPlatinumBenefitIds,
  )
  const [credentials, setCredentials] = useState<Record<CredentialProvider, string[]>>({
    plaid: ['', ''],
    snaptrade: ['', ''],
    alpaca: ['', ''],
    alphavantage: [''],
  })
  const [linkPhase, setLinkPhase] = useState<'browser' | 'checking' | 'refreshing'>('browser')
  const browserCompleted = useRef(false)
  const linkAttempt = useRef(0)
  const activeSession = useRef<ProviderLinkSession | null>(null)
  const linkingProvider = useRef<LinkProvider | null>(null)

  useEffect(() => {
    return () => {
      linkAttempt.current += 1
      linkingProvider.current = null
      if (activeSession.current)
        void cancelProviderLink(activeSession.current).catch(() => undefined)
      activeSession.current = null
    }
  }, [])

  const unlockCredentials = async () => {
    setCredentialsUnlocking(true)
    try {
      await authenticateSensitiveAction()
      setCredentialsUnlocked(true)
      toast.success('Credential editing unlocked')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      setCredentialsUnlocking(false)
    }
  }

  const saveIntegration = async (provider: CredentialProvider) => {
    setIntegrationSaving(provider)
    try {
      const [clientId, secret] = credentials[provider]
      const status = await saveIntegrationCredentials(
        provider,
        provider === 'plaid'
          ? { clientId, secret }
          : provider === 'snaptrade'
            ? { clientId, consumerKey: secret }
            : provider === 'alpaca'
              ? { clientId, secret }
              : { clientId },
      )
      queryClient.setQueryData(['integration-status'], status)
      void queryClient.invalidateQueries({ queryKey: ['market-snapshots'] })
      setCredentials((current) => ({
        ...current,
        [provider]: current[provider].map(() => ''),
      }))
      setCredentialsUnlocked(false)
      toast.success(`${providerName(provider)} credentials saved and tested`)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (message.includes('Authenticate before replacing saved credentials')) {
        setCredentialsUnlocked(false)
      }
      toast.error(message)
    } finally {
      setIntegrationSaving(null)
    }
  }

  const cancelLink = async () => {
    linkAttempt.current += 1
    linkingProvider.current = null
    setLinking(null)
    const session = activeSession.current
    activeSession.current = null
    if (session)
      await cancelProviderLink(session).catch(() =>
        toast.error('Could not cancel the native connection check'),
      )
  }
  const connectProvider = async (provider: LinkProvider, itemId?: string) => {
    if (linkingProvider.current) {
      toast.info(`Finish the ${linkProviderName(linkingProvider.current)} connection first`)
      return
    }
    const credentialProvider = provider === 'plaid-investments' ? 'plaid' : provider
    if (!integrationStatus[credentialProvider]) {
      toast.error(`Save your ${providerName(credentialProvider)} credentials first`)
      return
    }
    const attemptId = linkAttempt.current + 1
    linkAttempt.current = attemptId
    linkingProvider.current = provider
    setLinking(provider)
    setLinkPhase('browser')
    browserCompleted.current = false
    try {
      const session = await beginProviderLink(provider, itemId)
      if (linkAttempt.current !== attemptId) {
        await cancelProviderLink(session)
        return
      }
      activeSession.current = session
      toast.info('Finish connecting in your browser')
      for (let attempt = 0; attempt < 150; attempt += 1) {
        await wait(2_000)
        if (linkAttempt.current !== attemptId) return
        const result = await pollProviderLink(session, browserCompleted.current)
        if (linkAttempt.current !== attemptId) return
        if (result.status === 'connected') {
          setLinkPhase('refreshing')
          await refresh()
          if (linkAttempt.current !== attemptId) return
          void connections.refetch()
          toast.success(`${linkProviderName(provider)} connected`)
          return
        }
      }
      throw new Error('The connection window expired. Try again when you are ready.')
    } catch (error) {
      if (linkAttempt.current === attemptId)
        toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      if (linkAttempt.current === attemptId) {
        if (activeSession.current)
          void cancelProviderLink(activeSession.current).catch(() => undefined)
        activeSession.current = null
        linkingProvider.current = null
        setLinking(null)
      }
    }
  }
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const integrationStatus = query.integrationStatus
  const isConfigured = (provider: CredentialProvider) =>
    provider === 'alphavantage' ? integrationStatus.alphaVantage : integrationStatus[provider]
  const foundationModelStatus = foundationModel.data ?? {
    state: 'unavailable',
    message: foundationModel.isLoading ? 'Checking availability…' : 'Unavailable',
  }
  const creditAccounts = data.accounts.filter(({ type }) => type === 'credit')
  const selectedSpendingAccount = creditAccounts.some(({ id }) => id === spendingAccountId)
    ? spendingAccountId
    : ''
  const chartViews = dashboardAccountViews(data)
  const chartViewsById = new Map(chartViews.map((view) => [view.accountId, view]))
  const chartAccountPreferences = reconcileChartAccountPreferences(
    chartViews.map(({ accountId }) => accountId),
    savedChartAccountPreferences,
  )
  const visibleChartAccountCount = chartAccountPreferences.filter(({ visible }) => visible).length
  const renameableAccounts = data.accounts
    .filter(({ id }) => id !== 'all')
    .toSorted(
      (left, right) =>
        left.institution.localeCompare(right.institution) || left.name.localeCompare(right.name),
    )
  const updateAccountDisplayName = (accountId: string, name: string) => {
    const next = { ...accountDisplayNames }
    if (name) next[accountId] = name
    else delete next[accountId]
    setAccountDisplayNames(next)
    saveAccountDisplayNames(next)
  }
  const updateAccountStartDate = (accountId: string, date: string) => {
    const next = { ...accountStartDates }
    if (date) next[accountId] = date
    else delete next[accountId]
    setAccountStartDates(saveAccountStartDates(next))
  }
  const updateChartAccountPreferences = (next: typeof chartAccountPreferences) => {
    saveChartAccountPreferences(next)
    setSavedChartAccountPreferences(next)
  }
  const moveChartAccount = (index: number, direction: -1 | 1) => {
    const destination = index + direction
    if (destination < 0 || destination >= chartAccountPreferences.length) return
    const next = [...chartAccountPreferences]
    const current = next[index]
    next[index] = next[destination]
    next[destination] = current
    updateChartAccountPreferences(next)
  }
  const providers = dataSources.map((provider) => {
    if (provider.id === 'local') return { ...provider, status: 'local', lastSync: 'Current' }
    if (provider.id === 'intelligence') {
      return {
        ...provider,
        status: foundationModelStatus.state === 'available' ? 'ready' : 'error',
        lastSync:
          foundationModelStatus.state === 'available' ? 'Ready' : foundationModelStatus.message,
      }
    }
    const credentialProvider = provider.id === 'plaid-investments' ? 'plaid' : provider.id
    const configured = isConfigured(credentialProvider)
    const health = data.providerStatus?.[credentialProvider]
    const connected = data.accounts.some(
      (account) =>
        account.id.startsWith(`${provider.id === 'plaid-investments' ? 'plaid' : provider.id}:`) &&
        (provider.id !== 'plaid' || account.type === 'cash' || account.type === 'credit') &&
        (provider.id !== 'plaid-investments' ||
          account.type === 'brokerage' ||
          account.type === 'retirement'),
    )
    return {
      ...provider,
      status: !configured ? 'neutral' : health?.error ? 'error' : 'ready',
      lastSync: health?.error
        ? `Using saved data · ${health.error}`
        : (provider.id === 'alpaca' || provider.id === 'alphavantage') && configured
          ? 'Configured'
          : connected
            ? 'Connected'
            : configured
              ? 'Ready to connect'
              : 'Not configured',
    }
  })

  return (
    <div className="page settings-page">
      <WorkspaceHeader
        title="Settings"
        showSnapshot={false}
        showRefresh={false}
        actions={
          <>
            {linking ? (
              <Button icon={X} onClick={() => void cancelLink()}>
                Cancel connection
              </Button>
            ) : null}
            <Link to="/settings/design" className="button-base button-secondary button-default">
              <Palette size={16} aria-hidden="true" />
              <span className="sr-only">Design</span>
            </Link>
            <Link to="/logs" className="button-base button-secondary button-default">
              <ScrollText size={16} aria-hidden="true" />
              <span className="sr-only">Logs</span>
            </Link>
          </>
        }
      />

      <div className="settings-sections">
        <section className="settings-section settings-section-pair">
          <h2 className="settings-section-heading">Display</h2>
          <Card>
            <SectionHeading title="Page order" />
            <div className="page-order-list">
              {orderedPages.map(({ to, label, icon: Icon }, index) => (
                <div className="page-order-row" key={to}>
                  <Icon size={16} aria-hidden="true" />
                  <span>{label}</span>
                  <Button
                    icon={ArrowUp}
                    disabled={index === 0}
                    aria-label={`Move ${label} page up`}
                    onClick={() =>
                      savePageOrder(
                        movePage(
                          orderedPages.map((page) => page.to),
                          index,
                          -1,
                        ),
                      )
                    }
                  />
                  <Button
                    icon={ArrowDown}
                    disabled={index === orderedPages.length - 1}
                    aria-label={`Move ${label} page down`}
                    onClick={() =>
                      savePageOrder(
                        movePage(
                          orderedPages.map((page) => page.to),
                          index,
                          1,
                        ),
                      )
                    }
                  />
                </div>
              ))}
            </div>
          </Card>
          <Card>
            <SectionHeading title="Charts" />
            <div className="settings-control-row">
              <span>Home range</span>
              <RangeSelector
                className="graph-window-options"
                label="Default graph range"
                options={graphWindows.map(({ secs, label, settingsLabel }) => ({
                  value: secs,
                  label,
                  accessibleLabel: settingsLabel,
                }))}
                value={defaultGraphWindow}
                onValueChange={(secs) => {
                  saveDefaultGraphWindow(secs)
                  setDefaultGraphWindow(secs)
                }}
              />
            </div>
            <div className="settings-control-row">
              <span>Holdings range</span>
              <RangeSelector
                className="graph-window-options"
                label="Default Holdings graph range"
                options={holdingChartRanges}
                value={holdingChartRange}
                onValueChange={(value) => {
                  saveDefaultHoldingChartRange(value)
                  setHoldingChartRange(value)
                }}
              />
            </div>
            <div className="settings-control-row">
              <span>Live price updates</span>
              <RangeSelector
                className="graph-window-options"
                label="Live price updates"
                options={marketUpdateIntervals.map(({ seconds, label, settingsLabel }) => ({
                  value: seconds,
                  label,
                  accessibleLabel: settingsLabel,
                }))}
                value={marketUpdateInterval}
                onValueChange={(seconds) => {
                  saveMarketUpdateInterval(seconds)
                  setMarketUpdateInterval(seconds)
                }}
              />
            </div>

            <div className="settings-control-row">
              <label className="external-logo-setting">
                <input
                  type="checkbox"
                  checked={externalLogosEnabled}
                  onChange={(event) => {
                    saveExternalLogosEnabled(event.target.checked)
                    setExternalLogosEnabled(event.target.checked)
                  }}
                />
                <span>
                  <strong>Company and merchant logos</strong>
                </span>
              </label>
              <span className="muted">
                Requests ticker and merchant logos from Logo.dev or Plaid. Turn off to use local
                initials.
              </span>
            </div>
          </Card>
          <Card>
            <div className="chart-account-settings">
              <SectionHeading title="Home accounts" />
              <div className="chart-account-options">
                {chartAccountPreferences.map((preference, index) => {
                  const chart = chartViewsById.get(preference.accountId)
                  if (!chart) return null
                  return (
                    <div className="chart-account-option" key={preference.accountId}>
                      <label>
                        <input
                          type="checkbox"
                          checked={preference.visible}
                          disabled={preference.visible && visibleChartAccountCount === 1}
                          onChange={(event) =>
                            updateChartAccountPreferences(
                              chartAccountPreferences.map((item) =>
                                item.accountId === preference.accountId
                                  ? { ...item, visible: event.target.checked }
                                  : item,
                              ),
                            )
                          }
                        />
                        <span>
                          <strong>
                            {accountDisplayName(chart.accountId, chart.name, accountDisplayNames)}
                          </strong>
                          <small>
                            {chart.institution} · {formatCurrency(chart.currentValue)}
                          </small>
                        </span>
                      </label>
                      <div className="chart-account-order">
                        <Button
                          size="icon"
                          variant="ghost"
                          type="button"
                          disabled={index === 0}
                          aria-label={`Move ${accountDisplayName(chart.accountId, chart.name, accountDisplayNames)} up`}
                          onClick={() => moveChartAccount(index, -1)}
                        >
                          <ArrowUp size={14} aria-hidden="true" />
                        </Button>
                        <Button
                          size="icon"
                          variant="ghost"
                          type="button"
                          disabled={index === chartAccountPreferences.length - 1}
                          aria-label={`Move ${accountDisplayName(chart.accountId, chart.name, accountDisplayNames)} down`}
                          onClick={() => moveChartAccount(index, 1)}
                        >
                          <ArrowDown size={14} aria-hidden="true" />
                        </Button>
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          </Card>
        </section>
        <section className="settings-section">
          <h2 className="settings-section-heading">Spending</h2>
          <Card>
            <SectionHeading title="Spending account" />
            <div className="settings-field">
              <span>Credit account</span>
              <FilterSelect
                label="Credit account"
                value={selectedSpendingAccount}
                disabled={!creditAccounts.length}
                options={[
                  {
                    value: '',
                    label: creditAccounts.length
                      ? 'Choose an account'
                      : 'No credit accounts connected',
                  },
                  ...creditAccounts.map((account) => ({
                    value: account.id,
                    label: `${accountDisplayName(account.id, account.name, accountDisplayNames)} · ${account.institution}`,
                  })),
                ]}
                onValueChange={(accountId) => {
                  saveSpendingAccountId(accountId)
                  setSpendingAccountId(accountId)
                }}
              />
            </div>
            <fieldset className="benefit-visibility-settings" id="platinum-benefits">
              <legend>Visible Platinum benefits</legend>
              <div className="benefit-visibility-options">
                {platinumBenefitOptions.map((benefit) => (
                  <label key={benefit.id}>
                    <input
                      type="checkbox"
                      checked={!hiddenPlatinumBenefitIds.includes(benefit.id)}
                      onChange={(event) => {
                        const next = event.target.checked
                          ? hiddenPlatinumBenefitIds.filter((id) => id !== benefit.id)
                          : [...hiddenPlatinumBenefitIds, benefit.id]
                        saveHiddenPlatinumBenefitIds(next)
                        setHiddenPlatinumBenefitIds(next)
                      }}
                    />
                    <span>{benefit.name}</span>
                  </label>
                ))}
              </div>
            </fieldset>
          </Card>
        </section>
        <section className="settings-section">
          <h2 className="settings-section-heading">Accounts</h2>
          <Card>
            <SectionHeading title="Account details" />
            <div className="settings-table-scroll">
              <table className="settings-account-table">
                <thead>
                  <tr>
                    <th scope="col">Account</th>
                    <th scope="col">Display name</th>
                    <th scope="col">Start date</th>
                  </tr>
                </thead>
                <tbody>
                  {renameableAccounts.map((account) => (
                    <tr key={account.id}>
                      <th scope="row">
                        <strong>{account.name}</strong>
                        <small>
                          {account.institution} · {account.type}
                        </small>
                      </th>
                      <td>
                        <input
                          type="text"
                          maxLength={80}
                          value={accountDisplayNames[account.id] ?? ''}
                          placeholder="Provider name"
                          aria-label={`Display name for ${account.name}`}
                          onChange={(event) =>
                            updateAccountDisplayName(account.id, event.target.value)
                          }
                        />
                      </td>
                      <td>
                        <input
                          type="date"
                          max={data.updatedAt.slice(0, 10)}
                          value={accountStartDate(data, account.id, accountStartDates) ?? ''}
                          aria-label={`Start date for ${account.name}`}
                          onChange={(event) =>
                            updateAccountStartDate(account.id, event.target.value)
                          }
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {!renameableAccounts.length ? (
              <p className="settings-copy">No connected accounts to edit.</p>
            ) : null}
          </Card>
          {(data.possibleDuplicateAccounts?.length ?? 0) ||
          Object.keys(data.accountLinks ?? {}).length ? (
            <Card>
              <SectionHeading title="Linked accounts" />
              <div className="account-name-settings">
                {(data.possibleDuplicateAccounts ?? []).map((candidate) => (
                  <div className="account-name-setting" key={candidate.plaidAccountId}>
                    <span>
                      <strong>Possible duplicate</strong>
                      <small>{candidate.description}</small>
                    </span>
                    <Button
                      icon={Link2}
                      onClick={() => {
                        void saveAccountLink(candidate.plaidAccountId, candidate.snaptradeAccountId)
                          .then(() => refresh())
                          .then(() => toast.success('Accounts linked after provider confirmation'))
                          .catch((error) =>
                            toast.error(error instanceof Error ? error.message : String(error)),
                          )
                      }}
                    >
                      Confirm same account
                    </Button>
                  </div>
                ))}
                {Object.entries(data.accountLinks ?? {}).map(
                  ([plaidAccountId, snaptradeAccountId]) => (
                    <div className="account-name-setting" key={plaidAccountId}>
                      <span>
                        <strong>Confirmed linked account</strong>
                        <small>
                          {plaidAccountId} → {snaptradeAccountId}
                        </small>
                      </span>
                      <Button
                        icon={Unlink}
                        variant="destructive"
                        onClick={() => {
                          void saveAccountLink(plaidAccountId)
                            .then(() => refresh())
                            .then(() => toast.success('Account link removed'))
                            .catch((error) =>
                              toast.error(error instanceof Error ? error.message : String(error)),
                            )
                        }}
                      >
                        Unlink
                      </Button>
                    </div>
                  ),
                )}
              </div>
            </Card>
          ) : null}
        </section>
        <section className="settings-section settings-section-pair">
          <h2 className="settings-section-heading">Connections</h2>
          <Card>
            <SectionHeading title="Data sources" action={<RefreshButton />} />
            <div className="provider-list">
              {providers.map((provider) => (
                <div className="provider-row" key={provider.id}>
                  <span className="provider-icon">
                    {provider.id === 'intelligence' ? (
                      <Cpu size={17} />
                    ) : provider.id === 'local' ? (
                      <HardDrive size={17} />
                    ) : (
                      <Database size={17} />
                    )}
                  </span>
                  <span>
                    <strong>{provider.name}</strong>
                  </span>
                  <span className="provider-status">
                    <StatusDot
                      tone={
                        provider.status === 'error'
                          ? 'negative'
                          : provider.status === 'neutral'
                            ? 'neutral'
                            : 'positive'
                      }
                    />
                    {linking === provider.id
                      ? linkPhase === 'refreshing'
                        ? 'Refreshing accounts…'
                        : linkPhase === 'checking'
                          ? 'Checking connection…'
                          : 'Waiting for browser…'
                      : provider.lastSync}
                  </span>
                  {linking === 'snaptrade' && provider.id === 'snaptrade' ? (
                    <Button
                      disabled={linkPhase !== 'browser'}
                      onClick={() => {
                        browserCompleted.current = true
                        setLinkPhase('checking')
                      }}
                    >
                      Done in browser
                    </Button>
                  ) : isLinkProvider(provider.id) ? (
                    <Button
                      aria-label={`Connect ${provider.name}`}
                      disabled={Boolean(linking)}
                      onClick={() => {
                        if (isLinkProvider(provider.id)) void connectProvider(provider.id)
                      }}
                    >
                      <ChevronRight size={16} aria-hidden="true" />
                    </Button>
                  ) : (
                    <span />
                  )}
                </div>
              ))}
            </div>
          </Card>
          <div className="settings-section">
            <Card>
              <SectionHeading title="Bank connections" />
              {connections.isError ? (
                <p className="settings-copy">
                  Could not load connections.{' '}
                  <Button icon={RefreshCw} onClick={() => void connections.refetch()}>
                    Retry
                  </Button>
                </p>
              ) : null}
              {connections.isPending ? <p className="settings-copy">Loading connections…</p> : null}
              {connections.data?.map((connection) => (
                <div className="provider-row provider-connection-row" key={connection.itemId}>
                  <div>
                    <strong>{connection.name}</strong>
                    <p className="settings-copy">
                      {connection.error ?? 'Connected'} ·{' '}
                      {connection.provider === 'plaid-investments' ? 'Investments' : 'Banking'}
                    </p>
                  </div>
                  <Button
                    icon={Wrench}
                    disabled={Boolean(linking) || forgetConnection.isPending}
                    onClick={() => void connectProvider(connection.provider, connection.itemId)}
                  >
                    Repair
                  </Button>
                  <Button
                    icon={Trash2}
                    variant="destructive"
                    disabled={Boolean(linking) || forgetConnection.isPending}
                    onClick={() => setForgettingItem(connection.itemId)}
                  >
                    Forget locally
                  </Button>
                  {forgettingItem === connection.itemId ? (
                    <div
                      className="connection-confirmation"
                      role="group"
                      aria-label={`Forget ${connection.name} locally`}
                    >
                      <p>
                        Forget {connection.name}? Saved accounts disappear after the next successful
                        refresh. This does not revoke access at Plaid or your bank.
                      </p>
                      <div>
                        <Button
                          icon={X}
                          disabled={forgetConnection.isPending}
                          onClick={() => setForgettingItem(null)}
                        >
                          Cancel
                        </Button>
                        <Button
                          icon={Trash2}
                          variant="destructive"
                          disabled={forgetConnection.isPending || Boolean(linking)}
                          onClick={() => forgetConnection.mutate(connection.itemId)}
                        >
                          {forgetConnection.isPending ? 'Forgetting…' : 'Confirm forget'}
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              ))}
              {connections.data?.length === 0 ? (
                <p className="settings-copy">No saved Plaid connections.</p>
              ) : null}
            </Card>
            <Card>
              <SectionHeading title="Provider credentials" />

              <div className="integration-grid">
                {credentialProviders.map(({ id, name, fields }) => {
                  const configured = isConfigured(id)
                  const values = credentials[id]
                  return (
                    <details className="integration-form" key={id}>
                      <summary className="integration-title">
                        <strong>{name}</strong>
                        <StatusDot tone={configured ? 'positive' : 'neutral'} />
                        <small>{configured ? 'Configured' : 'Not configured'}</small>
                      </summary>
                      {configured && !credentialsUnlocked ? (
                        <div className="integration-locked">
                          <Button
                            icon={Fingerprint}
                            size="compact"
                            disabled={!isTauri() || credentialsUnlocking}
                            onClick={() => void unlockCredentials()}
                          >
                            {credentialsUnlocking ? 'Authenticating…' : 'Edit credentials'}
                          </Button>
                        </div>
                      ) : (
                        <>
                          {fields.map(({ label, placeholder, secret }, index) => (
                            <label key={label}>
                              <span>{label}</span>
                              <input
                                type={secret || index ? 'password' : 'text'}
                                value={values[index]}
                                onChange={(event) =>
                                  setCredentials((current) => {
                                    const next = [...current[id]]
                                    next[index] = event.target.value
                                    return { ...current, [id]: next }
                                  })
                                }
                                placeholder={placeholder}
                                autoComplete={secret || index ? 'new-password' : 'off'}
                              />
                            </label>
                          ))}
                          <Button
                            icon={Save}
                            variant="primary"
                            size="compact"
                            disabled={
                              !isTauri() ||
                              integrationSaving !== null ||
                              values.some((value) => !value)
                            }
                            onClick={() => void saveIntegration(id)}
                          >
                            {integrationSaving === id ? 'Testing…' : 'Save and test'}
                          </Button>
                        </>
                      )}
                    </details>
                  )
                })}
              </div>
            </Card>
          </div>
        </section>
      </div>
    </div>
  )
}
