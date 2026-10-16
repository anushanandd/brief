import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import {
  linkProviderNames,
  useProviderLink,
  type LinkProvider,
} from '../../hooks/use-provider-link'
import { useRefreshFinance } from '../../hooks/use-refresh-finance'
import {
  authenticateSensitiveAction,
  forgetProviderConnection,
  getProviderConnections,
  getSnapTradeConnections,
  isTauri,
  removeIntegrationCredentials,
  saveIntegrationCredentials,
  syncBankStatements,
} from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import { countOf, formatUpdatedAt } from '../../lib/format'
import { providerCatalog, type ProviderId } from '../../lib/providers'
import { Check, FilePlus, Fingerprint, Link2, RefreshCw, Save, Trash2, Wrench, X } from '../icons'
import {
  SettingsBlock,
  SettingsCard,
  SettingsNotice,
  SettingsRow,
  SettingsSection,
  SettingsSwitch,
} from '../settings-ui'
import { Button, StatusDot } from '../ui'

type CredentialProvider = ProviderId
type Credentials = Parameters<typeof saveIntegrationCredentials>[1]

type ProviderDefinition = (typeof providerCatalog)[number] & {
  description: string
  fields: Array<{
    key: keyof Credentials
    label: string
    placeholder: string
    secret?: boolean
  }>
  links?: Array<{ provider: LinkProvider; label: string }>
}

const providerDetails: Record<
  CredentialProvider,
  Omit<ProviderDefinition, 'id' | 'name' | 'status'>
> = {
  plaid: {
    description: 'Banks, cards, brokerages, and stock plans',
    fields: [
      { key: 'clientId', label: 'Client ID', placeholder: 'Plaid client ID' },
      { key: 'secret', label: 'Secret', placeholder: 'Plaid secret', secret: true },
    ],
    links: [
      { provider: 'plaid', label: 'Connect banking' },
      { provider: 'plaid-investments', label: 'Connect investments' },
    ],
  },
  snaptrade: {
    description: 'Brokerage positions, balances, and activity',
    fields: [
      { key: 'clientId', label: 'Client ID', placeholder: 'SnapTrade client ID' },
      { key: 'consumerKey', label: 'Consumer key', placeholder: 'Consumer key', secret: true },
    ],
    links: [{ provider: 'snaptrade', label: 'Connect account' }],
  },
  alpaca: {
    description: 'Live quotes, benchmarks, and price history',
    fields: [
      { key: 'clientId', label: 'API key ID', placeholder: 'Alpaca API key ID' },
      { key: 'secret', label: 'Secret key', placeholder: 'Alpaca secret key', secret: true },
    ],
  },
  alphavantage: {
    description: 'Earnings dates',
    fields: [
      { key: 'clientId', label: 'API key', placeholder: 'Alpha Vantage API key', secret: true },
    ],
  },
  sec: {
    description: 'Sector comparison and company filings; the SEC requires a contact email',
    fields: [{ key: 'clientId', label: 'Contact email', placeholder: 'you@example.com' }],
  },
}

const providers: ProviderDefinition[] = providerCatalog.map((provider) => ({
  ...provider,
  ...providerDetails[provider.id],
}))

const providerOf = (id: CredentialProvider) => providers.find((item) => item.id === id)!
const providerName = (id: CredentialProvider) => providerOf(id).name
const connectionKinds: Record<ProviderConnection['provider'], string> = {
  plaid: 'Banking',
  'plaid-investments': 'Investments',
  snaptrade: 'Brokerage',
}

export function ConnectionsSection() {
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const queryClient = useQueryClient()
  const connections = useQuery({
    queryKey: ['provider-connections', finance.data?.revision],
    queryFn: getProviderConnections,
  })
  const integrationStatus = finance.integrationStatus
  // The saved list shows at once; SnapTrade's live answer replaces it when it arrives.
  const snapTradeConnections = useQuery({
    queryKey: ['snaptrade-connections'],
    queryFn: getSnapTradeConnections,
    enabled: isTauri() && integrationStatus.snaptrade,
    staleTime: 5 * 60_000,
    retry: false,
  })
  const configuredCount = Object.values(integrationStatus).filter(Boolean).length
  const [editingCredentials, setEditingCredentials] = useState<CredentialProvider | null>(null)
  const [credentialsUnlocking, setCredentialsUnlocking] = useState(false)
  const link = useProviderLink({
    refresh,
    onConnected: (provider) => {
      void connections.refetch()
      void queryClient.invalidateQueries({ queryKey: ['snaptrade-connections'] })
      if (provider === 'plaid-statements') {
        toast.success('Statements added. Saving them now.')
        void syncBankStatements()
          .then(() => queryClient.invalidateQueries({ queryKey: ['bank-statements'] }))
          .catch((error) =>
            toast.error('Could not save statements', { description: errorMessage(error) }),
          )
        return
      }
      toast.success(`${linkProviderNames[provider]} connected`)
    },
  })
  const { linking, phase: linkPhase } = link

  const configured = (provider: CredentialProvider) =>
    integrationStatus[providerOf(provider).status]

  const connectProvider = (provider: LinkProvider, itemId?: string) => {
    const credentialProvider =
      provider === 'plaid-investments' || provider === 'plaid-statements' ? 'plaid' : provider
    if (!configured(credentialProvider)) {
      toast.error(`Save your ${providerName(credentialProvider)} credentials first`)
      return
    }
    void link.connect(provider, itemId)
  }

  const beginCredentialEdit = async (provider: CredentialProvider) => {
    if (!configured(provider)) {
      setEditingCredentials(provider)
      return
    }
    setCredentialsUnlocking(true)
    try {
      await authenticateSensitiveAction()
      setEditingCredentials(provider)
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setCredentialsUnlocking(false)
    }
  }

  return (
    <SettingsSection
      id="connections"
      title="Connections"
      action={
        linking ? (
          <Button icon={X} size="compact" onClick={() => void link.cancel()}>
            Cancel connection
          </Button>
        ) : (
          <small className="settings-section-meta">
            {configuredCount} of {providers.length} configured
          </small>
        )
      }
    >
      {isTauri() ? null : (
        <SettingsNotice>Credentials can be added only in the Brief desktop app.</SettingsNotice>
      )}
      <SettingsCard>
        {providers.map((provider) => {
          const isConfigured = configured(provider.id)
          const health = finance.data?.providerStatus?.[provider.id]
          const providerConnections =
            provider.id === 'snaptrade' && snapTradeConnections.data
              ? snapTradeConnections.data
              : (connections.data ?? []).filter(({ provider: kind }) =>
                  provider.id === 'plaid' ? kind.startsWith('plaid') : kind === provider.id,
                )
          const accountCount =
            finance.data?.accounts.filter((account) =>
              account.id.startsWith(`${provider.id === 'plaid' ? 'plaid' : provider.id}:`),
            ).length ?? 0
          const state = !isConfigured
            ? 'Not configured'
            : health?.error
              ? 'Needs attention · saved data retained'
              : health?.updatedAt
                ? `Checked ${formatUpdatedAt(health.updatedAt)}`
                : accountCount
                  ? countOf(accountCount, 'account')
                  : 'Configured'
          const editing = editingCredentials === provider.id
          return (
            <SettingsBlock
              key={provider.id}
              id={`connection-${provider.id}`}
              className="provider-settings-block"
              title={provider.name}
              description={
                <>
                  {isConfigured ? (
                    <span className="provider-card-status">
                      <StatusDot tone={health?.error ? 'negative' : 'positive'} />
                      {state}
                    </span>
                  ) : null}
                  <span>{provider.description}</span>
                </>
              }
              action={
                !isConfigured ? (
                  editing ? null : (
                    <Button
                      icon={Fingerprint}
                      size="compact"
                      variant="positive"
                      disabled={!isTauri()}
                      onClick={() => void beginCredentialEdit(provider.id)}
                    >
                      Add credentials
                    </Button>
                  )
                ) : linking === 'snaptrade' && provider.id === 'snaptrade' ? (
                  <Button
                    icon={Check}
                    size="compact"
                    disabled={linkPhase !== 'browser'}
                    onClick={link.browserDone}
                  >
                    Done in browser
                  </Button>
                ) : provider.links?.length ? (
                  <span className="provider-card-actions">
                    {provider.links.map((providerLink) => (
                      <Button
                        key={providerLink.provider}
                        icon={Link2}
                        size="compact"
                        disabled={Boolean(linking)}
                        onClick={() => connectProvider(providerLink.provider)}
                      >
                        {linking === providerLink.provider
                          ? linkPhase === 'refreshing'
                            ? 'Refreshing…'
                            : linkPhase === 'checking'
                              ? 'Checking…'
                              : 'Waiting for browser…'
                          : providerLink.label}
                      </Button>
                    ))}
                  </span>
                ) : null
              }
            >
              {health?.error ? (
                <p className="provider-card-error" role="status">
                  {health.error}
                </p>
              ) : null}

              {isConfigured && (provider.id === 'plaid' || providerConnections.length) ? (
                <ProviderConnections
                  connections={providerConnections}
                  pending={connections.isPending}
                  failed={connections.isError}
                  linking={Boolean(linking)}
                  repair={(connection) => connectProvider(connection.provider, connection.itemId)}
                  addStatements={(connection) =>
                    connectProvider('plaid-statements', connection.itemId)
                  }
                />
              ) : null}

              {editing ? (
                <CredentialForm provider={provider} done={() => setEditingCredentials(null)} />
              ) : isConfigured ? (
                <KeychainCredentials
                  provider={provider}
                  unlocking={credentialsUnlocking}
                  replace={() => void beginCredentialEdit(provider.id)}
                />
              ) : null}
            </SettingsBlock>
          )
        })}
      </SettingsCard>
      <PlaidOptions configured={configured('plaid')} />
    </SettingsSection>
  )
}

type ProviderConnection = NonNullable<Awaited<ReturnType<typeof getProviderConnections>>>[number]

/** A provider's connected institutions, each with repair and, for Plaid, a local forget. */
function ProviderConnections({
  connections,
  pending,
  failed,
  linking,
  repair,
  addStatements,
}: {
  connections: ProviderConnection[]
  pending: boolean
  failed: boolean
  /** A connection is in progress, so none can start. */
  linking: boolean
  repair: (connection: ProviderConnection) => void
  /** Adds statements to a Plaid bank connection through Link. */
  addStatements: (connection: ProviderConnection) => void
}) {
  const queryClient = useQueryClient()
  const [forgettingItem, setForgettingItem] = useState<string | null>(null)
  const forgetConnection = useMutation({
    mutationFn: async (itemId: string) => {
      await authenticateSensitiveAction()
      await forgetProviderConnection(itemId)
    },
    onSuccess: (_, itemId) => {
      queryClient.setQueriesData<ProviderConnection[]>(
        { queryKey: ['provider-connections'] },
        (current) => current?.filter((connection) => connection.itemId !== itemId),
      )
      setForgettingItem(null)
      toast.success('Connection forgotten. Refresh to update.')
      void queryClient.invalidateQueries({ queryKey: ['provider-connections'] })
    },
    onError: (error) =>
      toast.error('Could not forget connection', {
        description: errorMessage(error),
      }),
  })

  return (
    <div className="provider-connections">
      {pending ? <p className="settings-copy">Loading institutions…</p> : null}
      {failed ? <p className="settings-copy">Connected institutions are unavailable.</p> : null}
      {connections.map((connection) => (
        <div className="provider-connection-row" key={connection.itemId}>
          <span>
            <strong>{connection.name}</strong>
            <small>
              {connection.error ?? 'Connected'} · {connectionKinds[connection.provider]}
              {connection.statements ? ' · Statements' : ''}
            </small>
          </span>
          <div className="provider-connection-actions">
            {connection.statements === false ? (
              <Button
                icon={FilePlus}
                size="compact"
                disabled={linking || forgetConnection.isPending}
                onClick={() => addStatements(connection)}
              >
                Add statements
              </Button>
            ) : null}
            <Button
              icon={Wrench}
              size="compact"
              variant="positive"
              disabled={linking || forgetConnection.isPending}
              onClick={() => repair(connection)}
            >
              Repair
            </Button>
            {connection.provider === 'snaptrade' ? null : (
              <Button
                icon={Trash2}
                size="compact"
                variant="destructive"
                disabled={linking || forgetConnection.isPending}
                onClick={() => setForgettingItem(connection.itemId)}
              >
                Forget locally
              </Button>
            )}
          </div>
          {forgettingItem === connection.itemId ? (
            <div className="connection-confirmation" role="group">
              <p>
                Forget {connection.name}? Saved accounts disappear after the next successful
                refresh. This does not revoke access at Plaid or your bank.
              </p>
              <div>
                <Button icon={X} size="compact" onClick={() => setForgettingItem(null)}>
                  Cancel
                </Button>
                <Button
                  icon={Trash2}
                  size="compact"
                  variant="destructive"
                  disabled={forgetConnection.isPending}
                  onClick={() => forgetConnection.mutate(connection.itemId)}
                >
                  {forgetConnection.isPending ? 'Authenticating…' : 'Confirm forget'}
                </Button>
              </div>
            </div>
          ) : null}
        </div>
      ))}
      {!pending && !failed && !connections.length ? (
        <p className="settings-copy">No institutions connected yet.</p>
      ) : null}
    </div>
  )
}

/** New credentials for a provider, saved to Keychain only once the provider accepts them. */
function CredentialForm({ provider, done }: { provider: ProviderDefinition; done: () => void }) {
  const queryClient = useQueryClient()
  const [values, setValues] = useState(() => provider.fields.map(() => ''))
  const [saving, setSaving] = useState(false)

  const save = async () => {
    setSaving(true)
    try {
      const saved: Credentials = { clientId: '' }
      provider.fields.forEach(({ key }, index) => {
        saved[key] = values[index]
      })
      const status = await saveIntegrationCredentials(provider.id, saved)
      queryClient.setQueryData(['integration-status'], status)
      void queryClient.invalidateQueries({ queryKey: ['market-snapshots'] })
      toast.success(`${provider.name} credentials saved and tested`)
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSaving(false)
      done()
    }
  }

  return (
    <form
      className="provider-credential-form"
      aria-label={`${provider.name} credentials`}
      onSubmit={(event) => {
        event.preventDefault()
        void save()
      }}
    >
      {provider.fields.map(({ label, placeholder, secret }, index) => (
        <label key={label}>
          <span>{label}</span>
          <input
            type={secret ? 'password' : 'text'}
            value={values[index]}
            onChange={(event) => setValues((current) => current.with(index, event.target.value))}
            placeholder={placeholder}
            autoComplete={secret ? 'new-password' : 'off'}
          />
        </label>
      ))}
      <div className="provider-credential-actions">
        <small>Saved to macOS Keychain and tested before use.</small>
        <Button type="button" icon={X} size="compact" onClick={done}>
          Cancel
        </Button>
        <Button
          type="submit"
          icon={Save}
          size="compact"
          variant="positive"
          disabled={!isTauri() || saving || values.some((value) => !value)}
        >
          {saving ? 'Testing…' : 'Save and test'}
        </Button>
      </div>
    </form>
  )
}

/** Saved credentials: replaced after authenticating, or removed after confirming. */
function KeychainCredentials({
  provider,
  unlocking,
  replace,
}: {
  provider: ProviderDefinition
  /** Authentication for a replacement is in progress. */
  unlocking: boolean
  replace: () => void
}) {
  const queryClient = useQueryClient()
  const [confirming, setConfirming] = useState(false)

  const remove = async () => {
    try {
      await authenticateSensitiveAction()
      const status = await removeIntegrationCredentials(provider.id)
      queryClient.setQueryData(['integration-status'], status)
      if (provider.id === 'plaid') queryClient.setQueryData(['provider-connections'], [])
      if (provider.id === 'snaptrade')
        queryClient.removeQueries({ queryKey: ['snaptrade-connections'] })
      void queryClient.invalidateQueries({ queryKey: ['provider-connections'] })
      void queryClient.invalidateQueries({ queryKey: ['market-snapshots'] })
      setConfirming(false)
      toast.success(`${provider.name} credentials removed from Keychain`)
    } catch (error) {
      toast.error('Could not remove credentials', { description: errorMessage(error) })
    }
  }

  return (
    <>
      <div className="provider-credential-summary">
        <Fingerprint size={14} aria-hidden="true" />
        <small>Credentials in macOS Keychain</small>
        <Button
          icon={RefreshCw}
          size="compact"
          variant="ghost"
          disabled={!isTauri() || unlocking}
          onClick={replace}
        >
          {unlocking ? 'Authenticating…' : 'Replace'}
        </Button>
        <Button
          icon={Trash2}
          size="compact"
          variant="destructive"
          onClick={() => setConfirming(true)}
        >
          Remove
        </Button>
      </div>
      {confirming ? (
        <div className="connection-confirmation" role="group">
          <p>
            Remove {provider.name} credentials from Keychain? Saved financial data remains on this
            Mac. Plaid institution tokens are also removed locally.
          </p>
          <div>
            <Button icon={X} size="compact" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              icon={Fingerprint}
              size="compact"
              variant="destructive"
              onClick={() => void remove()}
            >
              Authenticate and remove
            </Button>
          </div>
        </div>
      ) : null}
    </>
  )
}

/** Billed Plaid products stay explicit: one-off live balances and opt-in subscriptions. */
function PlaidOptions({ configured }: { configured: boolean }) {
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const { updateSettings } = useFinanceEdits()
  const [refreshing, setRefreshing] = useState(false)
  const settings = financeSettings(finance.data)
  const enabled = settings.plaidLiabilities
  const unavailable = !configured || !isTauri() || Boolean(finance.data?.recovery)
  const toggle =
    (key: 'plaidInvestmentTransactions' | 'plaidRecurring', label: string) => (checked: boolean) =>
      void updateSettings((current) => ({ ...current, [key]: checked })).catch((error) =>
        toast.error(`Could not save ${label}`, {
          description: errorMessage(error),
        }),
      )
  return (
    <SettingsCard>
      <SettingsRow
        id="plaid-live-balances"
        title="Live bank balances"
        description="Asks each bank now. Plaid bills each request."
      >
        <Button
          icon={RefreshCw}
          size="compact"
          disabled={unavailable || refreshing}
          aria-busy={refreshing || undefined}
          onClick={() => {
            setRefreshing(true)
            void refresh({ liveBalances: true })
              .catch(() => undefined)
              .finally(() => setRefreshing(false))
          }}
        >
          {refreshing ? 'Refreshing…' : 'Refresh live balances'}
        </Button>
      </SettingsRow>
      <SettingsRow
        id="plaid-statement-details"
        title="Card statement details"
        description="Statement and due dates. Billed per connection; may need Repair."
      >
        <SettingsSwitch
          labelledBy="plaid-statement-details-title"
          describedBy="plaid-statement-details-description"
          checked={enabled}
          disabled={unavailable}
          onCheckedChange={(checked) =>
            void updateSettings((current) => ({ ...current, plaidLiabilities: checked })).catch(
              (error) =>
                toast.error('Could not save statement details', {
                  description: errorMessage(error),
                }),
            )
          }
        />
      </SettingsRow>
      <BankStatementsRow unavailable={unavailable} />
      <SettingsRow
        id="plaid-investment-activity"
        title="Investment activity"
        description="Trades and dividends for Plaid investments. Billed per connection."
      >
        <SettingsSwitch
          labelledBy="plaid-investment-activity-title"
          describedBy="plaid-investment-activity-description"
          checked={settings.plaidInvestmentTransactions}
          disabled={unavailable}
          onCheckedChange={toggle('plaidInvestmentTransactions', 'investment activity')}
        />
      </SettingsRow>
      <SettingsRow
        id="plaid-recurring-streams"
        title="Recurring streams"
        description="Catches cancelled bills. Billed per connection."
      >
        <SettingsSwitch
          labelledBy="plaid-recurring-streams-title"
          describedBy="plaid-recurring-streams-description"
          checked={settings.plaidRecurring}
          disabled={unavailable}
          onCheckedChange={toggle('plaidRecurring', 'recurring streams')}
        />
      </SettingsRow>
    </SettingsCard>
  )
}

/**
 * Statement PDFs for the bank connections the user added them to. Brief checks for new ones
 * after a refresh, at most daily; this checks now.
 */
function BankStatementsRow({ unavailable }: { unavailable: boolean }) {
  const queryClient = useQueryClient()
  const check = useMutation({
    mutationFn: syncBankStatements,
    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: ['bank-statements'] })
      if (result.failures.length)
        toast.error('Some statements could not be checked', {
          description: result.failures.join(' '),
        })
      else
        toast.success(
          result.saved ? `${countOf(result.saved, 'statement')} saved` : 'No new statements',
        )
    },
    onError: (error) =>
      toast.error('Could not check statements', { description: errorMessage(error) }),
  })
  return (
    <SettingsRow
      id="plaid-bank-statements"
      title="Bank statements"
      description="Bank PDFs for checking and savings. Billed per statement off Plaid's Trial plan."
    >
      <Button
        icon={RefreshCw}
        size="compact"
        disabled={unavailable || check.isPending}
        aria-busy={check.isPending || undefined}
        onClick={() => check.mutate()}
      >
        {check.isPending ? 'Checking…' : 'Check now'}
      </Button>
    </SettingsRow>
  )
}
