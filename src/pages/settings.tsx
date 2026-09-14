import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import {
  ArrowDown,
  ArrowUp,
  Check,
  ChevronRight,
  Cpu,
  Database,
  Fingerprint,
  HardDrive,
  Save,
  ScrollText,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'

import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Button, Card, SectionHeading, StatusDot } from '../components/ui'
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
import { getExternalLogosEnabled, saveExternalLogosEnabled } from '../lib/logos'
import { platinumBenefitOptions } from '../lib/spending'
import {
  getHiddenPlatinumBenefitIds,
  getSpendingAccountId,
  saveHiddenPlatinumBenefitIds,
  saveSpendingAccountId,
} from '../lib/spending-preferences'

const wait = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration))
type CredentialProvider = 'plaid' | 'snaptrade' | 'alpaca'
type LinkProvider = 'plaid' | 'plaid-investments' | 'snaptrade'
const credentialProviders: Array<{
  id: CredentialProvider
  name: string
  fields: [{ label: string; placeholder: string }, { label: string; placeholder: string }]
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
  const [credentials, setCredentials] = useState<Record<CredentialProvider, [string, string]>>({
    plaid: ['', ''],
    snaptrade: ['', ''],
    alpaca: ['', ''],
  })
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
            : { clientId, secret },
      )
      queryClient.setQueryData(['integration-status'], status)
      void queryClient.invalidateQueries({ queryKey: ['market-snapshots'] })
      setCredentials((current) => ({ ...current, [provider]: ['', ''] }))
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
        const result = await pollProviderLink(session)
        if (linkAttempt.current !== attemptId) return
        if (result.status === 'connected') {
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
        lastSync: foundationModelStatus.message,
      }
    }
    const credentialProvider = provider.id === 'plaid-investments' ? 'plaid' : provider.id
    const configured = integrationStatus[credentialProvider]
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
        : provider.id === 'alpaca' && configured
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
      <header className="page-header workspace-header settings-header">
        <h1>Settings</h1>
        {linking ? <Button onClick={() => void cancelLink()}>Cancel connection</Button> : null}
      </header>

      <div className="settings-sections">
        <section className="settings-section">
          <h2 className="settings-section-heading">Accounts & connections</h2>

          <Card>
            <SectionHeading
              title="Bank connections"
              detail="Repair a specific Plaid connection or forget it locally. Previously saved balances remain until the next successful refresh; recorded history is retained."
            />
            {connections.isError ? (
              <p className="settings-copy">
                Could not load connections.{' '}
                <Button onClick={() => void connections.refetch()}>Retry</Button>
              </p>
            ) : null}
            {connections.isPending ? <p className="settings-copy">Loading connections…</p> : null}
            {connections.data?.map((connection) => (
              <div className="provider-row connection-row" key={connection.itemId}>
                <div>
                  <strong>{connection.name}</strong>
                  <p className="settings-copy">
                    {connection.error ?? 'Connected'} ·{' '}
                    {connection.provider === 'plaid-investments' ? 'Investments' : 'Banking'}
                  </p>
                </div>
                <Button
                  disabled={Boolean(linking)}
                  onClick={() => void connectProvider(connection.provider, connection.itemId)}
                >
                  Repair
                </Button>
                <Button
                  variant="destructive"
                  disabled={Boolean(linking)}
                  onClick={() => {
                    if (
                      !window.confirm(
                        `Forget ${connection.name} locally? Saved accounts disappear on the next refresh. This does not revoke access at Plaid or your bank.`,
                      )
                    )
                      return
                    void forgetProviderConnection(connection.itemId)
                      .then(() => connections.refetch())
                      .then(() =>
                        toast.success('Connection forgotten locally. Refresh to update accounts.'),
                      )
                      .catch(() => toast.error('Could not forget connection'))
                  }}
                >
                  Forget locally
                </Button>
              </div>
            ))}
            {connections.data?.length === 0 ? (
              <p className="settings-copy">No saved Plaid connections.</p>
            ) : null}
          </Card>

          <Card>
            <SectionHeading title="Integrations" />
            <p className="settings-copy">Provider keys stay in the macOS Keychain.</p>
            <div className="integration-grid">
              {credentialProviders.map(({ id, name, fields }) => {
                const configured = integrationStatus[id]
                const values = credentials[id]
                return (
                  <div className="integration-form" key={id}>
                    <div className="integration-title">
                      <strong>{name}</strong>
                      <StatusDot tone={configured ? 'positive' : 'neutral'} />
                      <small>{configured ? 'Configured' : 'Not configured'}</small>
                    </div>
                    {configured && !credentialsUnlocked ? (
                      <div className="integration-locked">
                        <Button
                          size="compact"
                          disabled={!isTauri() || credentialsUnlocking}
                          onClick={() => void unlockCredentials()}
                        >
                          <Fingerprint size={15} />
                          {credentialsUnlocking ? 'Authenticating…' : 'Edit credentials'}
                        </Button>
                      </div>
                    ) : (
                      <>
                        {fields.map(({ label, placeholder }, index) => (
                          <label key={label}>
                            <span>{label}</span>
                            <input
                              type={index ? 'password' : 'text'}
                              value={values[index]}
                              onChange={(event) =>
                                setCredentials((current) => {
                                  const next: [string, string] = [...current[id]]
                                  next[index] = event.target.value
                                  return { ...current, [id]: next }
                                })
                              }
                              placeholder={placeholder}
                              autoComplete={index ? 'new-password' : 'off'}
                            />
                          </label>
                        ))}
                        <Button
                          variant="primary"
                          size="compact"
                          disabled={
                            !isTauri() ||
                            integrationSaving !== null ||
                            values.some((value) => !value)
                          }
                          onClick={() => void saveIntegration(id)}
                        >
                          <Save size={14} />
                          {integrationSaving === id ? 'Testing…' : 'Save and test'}
                        </Button>
                      </>
                    )}
                  </div>
                )
              })}
            </div>
          </Card>

          <Card>
            <SectionHeading
              title="Account names"
              detail="Set the names Brief shows for bank, brokerage, retirement, and credit accounts."
            />
            <div className="account-name-settings">
              {renameableAccounts.map((account) => (
                <label className="account-name-setting" key={account.id}>
                  <span>
                    <strong>{account.name}</strong>
                    <small>
                      {account.institution} · {account.type}
                    </small>
                  </span>
                  <input
                    type="text"
                    maxLength={80}
                    value={accountDisplayNames[account.id] ?? ''}
                    placeholder="Use provider name"
                    aria-label={`Display name for ${account.name}`}
                    onChange={(event) => updateAccountDisplayName(account.id, event.target.value)}
                  />
                </label>
              ))}
              {!renameableAccounts.length ? (
                <p className="settings-copy">No connected accounts to rename.</p>
              ) : null}
            </div>
            <p className="settings-copy account-name-note">
              Display names stay on this Mac and do not change provider or account data. Clear a
              field to use its provider name.
            </p>
          </Card>

          <Card>
            <SectionHeading
              title="Account start dates"
              detail="Inferred from each account’s earliest imported transaction or trade."
            />
            <div className="account-name-settings">
              {renameableAccounts.map((account) => (
                <label className="account-name-setting" key={account.id}>
                  <span>
                    <strong>
                      {accountDisplayName(account.id, account.name, accountDisplayNames)}
                    </strong>
                    <small>
                      {accountStartDates[account.id] ? 'Custom date' : 'First imported activity'}
                    </small>
                  </span>
                  <input
                    type="date"
                    max={data.updatedAt.slice(0, 10)}
                    value={accountStartDate(data, account.id, accountStartDates) ?? ''}
                    aria-label={`Start date for ${account.name}`}
                    onChange={(event) => updateAccountStartDate(account.id, event.target.value)}
                  />
                </label>
              ))}
              {!renameableAccounts.length ? (
                <p className="settings-copy">No connected accounts to edit.</p>
              ) : null}
            </div>
            <p className="settings-copy account-name-note">
              Dates stay on this Mac. Clear a custom date to use the first imported activity again.
            </p>
          </Card>

          {(data.possibleDuplicateAccounts?.length ?? 0) ||
          Object.keys(data.accountLinks ?? {}).length ? (
            <Card>
              <SectionHeading
                title="Linked accounts"
                detail="Brief never removes a possible duplicate automatically. Confirm only accounts that represent the same assets."
              />
              <div className="account-name-settings">
                {(data.possibleDuplicateAccounts ?? []).map((candidate) => (
                  <div className="account-name-setting" key={candidate.plaidAccountId}>
                    <span>
                      <strong>Possible duplicate</strong>
                      <small>{candidate.description}</small>
                    </span>
                    <Button
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

        <section className="settings-section">
          <h2 className="settings-section-heading">Home & display</h2>

          <Card>
            <SectionHeading title="Charts" />
            <p className="settings-copy">Choose the default range for Home charts.</p>
            <div
              className="graph-window-options"
              role="radiogroup"
              aria-label="Default graph range"
            >
              {graphWindows.map(({ secs, settingsLabel }) => (
                <button
                  key={secs}
                  type="button"
                  role="radio"
                  aria-checked={defaultGraphWindow === secs}
                  className={defaultGraphWindow === secs ? 'active' : ''}
                  onClick={() => {
                    saveDefaultGraphWindow(secs)
                    setDefaultGraphWindow(secs)
                  }}
                >
                  <span>{settingsLabel}</span>
                  {defaultGraphWindow === secs ? <Check size={14} /> : null}
                </button>
              ))}
            </div>
            <fieldset className="chart-account-settings">
              <legend>Charts shown on Home</legend>
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
                        <button
                          type="button"
                          disabled={index === 0}
                          aria-label={`Move ${accountDisplayName(chart.accountId, chart.name, accountDisplayNames)} up`}
                          onClick={() => moveChartAccount(index, -1)}
                        >
                          <ArrowUp size={14} />
                        </button>
                        <button
                          type="button"
                          disabled={index === chartAccountPreferences.length - 1}
                          aria-label={`Move ${accountDisplayName(chart.accountId, chart.name, accountDisplayNames)} down`}
                          onClick={() => moveChartAccount(index, 1)}
                        >
                          <ArrowDown size={14} />
                        </button>
                      </div>
                    </div>
                  )
                })}
              </div>
              <small>Unchecked accounts stay available elsewhere in Brief.</small>
            </fieldset>
          </Card>

          <Card>
            <SectionHeading title="Company logos" />
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
                <strong>Load actual company and merchant logos</strong>
                <small>
                  Sends ticker or merchant-domain identifiers to Logo.dev or Plaid when those marks
                  are displayed. Off uses private local initials.
                </small>
              </span>
            </label>
          </Card>
        </section>

        <section className="settings-section">
          <h2 className="settings-section-heading">Spending & benefits</h2>

          <Card>
            <SectionHeading title="Spending account" />
            <label className="settings-field">
              <span>Credit account</span>
              <select
                value={selectedSpendingAccount}
                disabled={!creditAccounts.length}
                onChange={(event) => {
                  const accountId = event.target.value
                  saveSpendingAccountId(accountId)
                  setSpendingAccountId(accountId)
                }}
              >
                <option value="">
                  {creditAccounts.length ? 'Choose an account' : 'No credit accounts connected'}
                </option>
                {creditAccounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {accountDisplayName(account.id, account.name, accountDisplayNames)} ·{' '}
                    {account.institution}
                  </option>
                ))}
              </select>
              <small>Transactions and card benefits use this account.</small>
            </label>
            <fieldset className="benefit-visibility-settings">
              <legend>Visible Platinum credits</legend>
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
              <small>Choose which credits appear in Spending.</small>
            </fieldset>
          </Card>
        </section>

        <section className="settings-section">
          <h2 className="settings-section-heading">Privacy & data</h2>

          <Card>
            <SectionHeading title="Data sources" action={<RefreshButton />} />
            <div className="provider-list">
              {providers.map((provider) => (
                <button
                  className="provider-row"
                  type="button"
                  key={provider.id}
                  disabled={!isLinkProvider(provider.id)}
                  onClick={() => {
                    if (isLinkProvider(provider.id)) void connectProvider(provider.id)
                  }}
                >
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
                    <small>{provider.description}</small>
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
                    {linking === provider.id ? 'Waiting for browser…' : provider.lastSync}
                  </span>
                  {isLinkProvider(provider.id) ? <ChevronRight size={15} /> : <span />}
                </button>
              ))}
            </div>
          </Card>

          <Card>
            <SectionHeading title="Diagnostics" />
            <Link to="/logs" className="settings-page-link">
              <span className="provider-icon">
                <ScrollText size={17} />
              </span>
              <span>
                <strong>Logs</strong>
                <small>Provider health, quote timestamps, and recent runtime activity</small>
              </span>
              <ChevronRight size={15} />
            </Link>
          </Card>
        </section>
      </div>

      <p className="settings-footnote">Brief 0.1.0 · Not financial advice</p>
    </div>
  )
}
