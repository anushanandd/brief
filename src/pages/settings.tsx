import { useQueryClient } from '@tanstack/react-query'
import {
  Check,
  ChevronRight,
  Database,
  Fingerprint,
  HardDrive,
  ImageIcon,
  Laptop,
  Moon,
  Save,
  Sun,
} from 'lucide-react'
import { useTheme } from 'next-themes'
import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'

import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { financeQueryKey } from '../hooks/use-finance'
import {
  authenticateSensitiveAction,
  beginProviderLink,
  getIntegrationStatus,
  isTauri,
  pollProviderLink,
  openExternalUrl,
  refreshFinanceSnapshot,
  saveIntegrationCredentials,
} from '../lib/api'
import {
  getDefaultGraphWindow,
  graphWindows,
  saveDefaultGraphWindow,
} from '../lib/graph-preferences'

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

export function SettingsPage() {
  const query = useFinance()
  const queryClient = useQueryClient()
  const { theme, setTheme } = useTheme()
  const [linking, setLinking] = useState<LinkProvider | null>(null)
  const [integrationStatus, setIntegrationStatus] = useState({
    plaid: false,
    snaptrade: false,
    alpaca: false,
  })
  const [integrationSaving, setIntegrationSaving] = useState<CredentialProvider | null>(null)
  const [credentialsUnlocked, setCredentialsUnlocked] = useState(false)
  const [credentialsUnlocking, setCredentialsUnlocking] = useState(false)
  const [defaultGraphWindow, setDefaultGraphWindow] = useState(getDefaultGraphWindow)
  const [credentials, setCredentials] = useState<Record<CredentialProvider, [string, string]>>({
    plaid: ['', ''],
    snaptrade: ['', ''],
    alpaca: ['', ''],
  })
  const linkAttempt = useRef(0)
  const linkingProvider = useRef<LinkProvider | null>(null)

  useEffect(() => {
    void getIntegrationStatus()
      .then(setIntegrationStatus)
      .catch(() => undefined)
    return () => {
      linkAttempt.current += 1
      linkingProvider.current = null
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
      setIntegrationStatus(status)
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

  const connectProvider = async (provider: LinkProvider) => {
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
      const session = await beginProviderLink(provider)
      toast.info('Finish connecting in your browser')
      for (let attempt = 0; attempt < 150; attempt += 1) {
        await wait(2_000)
        if (linkAttempt.current !== attemptId) return
        const result = await pollProviderLink(session)
        if (result.status === 'connected') {
          const snapshot = await refreshFinanceSnapshot()
          queryClient.setQueryData(financeQueryKey, snapshot)
          toast.success(`${linkProviderName(provider)} connected`)
          return
        }
      }
      throw new Error('The connection window expired. Try again when you are ready.')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      if (linkAttempt.current === attemptId) {
        linkingProvider.current = null
        setLinking(null)
      }
    }
  }
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const providers = [
    ...data.providers,
    ...(data.providers.some((provider) => provider.id === 'plaid-investments')
      ? []
      : [
          {
            id: 'plaid-investments',
            name: 'Plaid Investments',
            description: 'Brokerage and stock plan accounts',
            status: 'error' as const,
            lastSync: 'Not connected',
          },
        ]),
  ].map((provider) =>
    provider.id === 'alpaca'
      ? {
          ...provider,
          status: integrationStatus.alpaca ? ('ready' as const) : ('error' as const),
          lastSync: integrationStatus.alpaca ? 'Configured' : 'Not configured',
        }
      : provider,
  )

  return (
    <div className="page settings-page">
      <header className="page-header">
        <div>
          <h1>Settings</h1>
        </div>
      </header>

      <Card>
        <SectionHeading title="Integrations" />
        <p className="settings-copy">
          Bring your own provider keys. Secrets and provider access tokens stay in the macOS
          Keychain on this Mac. Settings remain available without unlocking; system authentication
          protects credential changes.
        </p>
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
                    <p>Credentials are stored securely and are never displayed.</p>
                    <button
                      className="credential-edit-button"
                      type="button"
                      disabled={!isTauri() || credentialsUnlocking}
                      onClick={() => void unlockCredentials()}
                    >
                      <Fingerprint size={15} />
                      {credentialsUnlocking ? 'Authenticating…' : 'Edit credentials'}
                    </button>
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
                    <button
                      className="sync-save-button"
                      type="button"
                      disabled={
                        !isTauri() || integrationSaving !== null || values.some((value) => !value)
                      }
                      onClick={() => void saveIntegration(id)}
                    >
                      <Save size={14} />
                      {integrationSaving === id ? 'Testing…' : 'Save and test'}
                    </button>
                  </>
                )}
              </div>
            )
          })}
        </div>
      </Card>

      <Card>
        <SectionHeading title="Appearance" />
        <div className="theme-options" role="radiogroup" aria-label="Appearance">
          {[
            { id: 'light', label: 'Light', icon: Sun },
            { id: 'dark', label: 'Dark', icon: Moon },
            { id: 'system', label: 'System', icon: Laptop },
          ].map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={theme === id}
              className={theme === id ? 'active' : ''}
              onClick={() => setTheme(id)}
            >
              <Icon size={18} />
              <span>{label}</span>
              {theme === id ? <Check size={14} /> : null}
            </button>
          ))}
        </div>
      </Card>

      <Card>
        <SectionHeading title="Charts" />
        <p className="settings-copy">Default graph range</p>
        <div
          className="theme-options graph-window-options"
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
      </Card>

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
                {provider.id === 'logos' ? (
                  <ImageIcon size={17} />
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
                <StatusDot tone={provider.status === 'error' ? 'negative' : 'positive'} />
                {linking === provider.id ? 'Waiting for browser…' : provider.lastSync}
              </span>
              {isLinkProvider(provider.id) ? <ChevronRight size={15} /> : <span />}
            </button>
          ))}
        </div>
      </Card>

      <p className="settings-footnote">
        Brief 0.1.0 · Not financial advice ·{' '}
        <a
          href="https://logo.dev"
          target="_blank"
          rel="noopener noreferrer"
          onClick={(event) => {
            if (!isTauri()) return
            event.preventDefault()
            void openExternalUrl('https://logo.dev')
          }}
        >
          Logos by Logo.dev
        </a>
      </p>
    </div>
  )
}
