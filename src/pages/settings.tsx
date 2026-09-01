import { useQueryClient } from '@tanstack/react-query'
import {
  Check,
  ChevronRight,
  Database,
  Fingerprint,
  HardDrive,
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

const wait = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration))
const isLinkProvider = (provider: string): provider is 'plaid' | 'snaptrade' =>
  provider === 'plaid' || provider === 'snaptrade'

export function SettingsPage() {
  const query = useFinance()
  const queryClient = useQueryClient()
  const { theme, setTheme } = useTheme()
  const [linking, setLinking] = useState<'plaid' | 'snaptrade' | null>(null)
  const [integrationStatus, setIntegrationStatus] = useState({ plaid: false, snaptrade: false })
  const [integrationSaving, setIntegrationSaving] = useState<'plaid' | 'snaptrade' | null>(null)
  const [credentialsUnlocked, setCredentialsUnlocked] = useState(false)
  const [credentialsUnlocking, setCredentialsUnlocking] = useState(false)
  const [plaidClientId, setPlaidClientId] = useState('')
  const [plaidSecret, setPlaidSecret] = useState('')
  const [snaptradeClientId, setSnaptradeClientId] = useState('')
  const [snaptradeConsumerKey, setSnaptradeConsumerKey] = useState('')
  const linkAttempt = useRef(0)
  const linkingProvider = useRef<'plaid' | 'snaptrade' | null>(null)

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

  const saveIntegration = async (provider: 'plaid' | 'snaptrade') => {
    setIntegrationSaving(provider)
    try {
      const status = await saveIntegrationCredentials(
        provider,
        provider === 'plaid'
          ? { clientId: plaidClientId, secret: plaidSecret }
          : { clientId: snaptradeClientId, consumerKey: snaptradeConsumerKey },
      )
      setIntegrationStatus(status)
      if (provider === 'plaid') {
        setPlaidClientId('')
        setPlaidSecret('')
      } else {
        setSnaptradeClientId('')
        setSnaptradeConsumerKey('')
      }
      setCredentialsUnlocked(false)
      toast.success(`${provider === 'plaid' ? 'Plaid' : 'SnapTrade'} credentials saved and tested`)
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

  const connectProvider = async (provider: 'plaid' | 'snaptrade') => {
    if (linkingProvider.current) {
      toast.info(
        `Finish the ${linkingProvider.current === 'plaid' ? 'Plaid' : 'SnapTrade'} connection before starting another one`,
      )
      return
    }
    if (!integrationStatus[provider]) {
      toast.error(`Save your ${provider === 'plaid' ? 'Plaid' : 'SnapTrade'} credentials first`)
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
          toast.success(`${provider === 'plaid' ? 'Plaid' : 'SnapTrade'} connected`)
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
          <div className="integration-form">
            <div className="integration-title">
              <strong>Plaid</strong>
              <StatusDot tone={integrationStatus.plaid ? 'positive' : 'neutral'} />
              <small>{integrationStatus.plaid ? 'Configured' : 'Not configured'}</small>
            </div>
            {integrationStatus.plaid && !credentialsUnlocked ? (
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
                <label>
                  <span>Client ID</span>
                  <input
                    value={plaidClientId}
                    onChange={(event) => setPlaidClientId(event.target.value)}
                    placeholder="Plaid client ID"
                    autoComplete="off"
                  />
                </label>
                <label>
                  <span>Secret</span>
                  <input
                    type="password"
                    value={plaidSecret}
                    onChange={(event) => setPlaidSecret(event.target.value)}
                    placeholder="Plaid secret"
                    autoComplete="new-password"
                  />
                </label>
                <button
                  className="sync-save-button"
                  type="button"
                  disabled={
                    !isTauri() || integrationSaving !== null || !plaidClientId || !plaidSecret
                  }
                  onClick={() => void saveIntegration('plaid')}
                >
                  <Save size={14} />
                  {integrationSaving === 'plaid' ? 'Testing…' : 'Save and test'}
                </button>
              </>
            )}
          </div>
          <div className="integration-form">
            <div className="integration-title">
              <strong>SnapTrade</strong>
              <StatusDot tone={integrationStatus.snaptrade ? 'positive' : 'neutral'} />
              <small>{integrationStatus.snaptrade ? 'Configured' : 'Not configured'}</small>
            </div>
            {integrationStatus.snaptrade && !credentialsUnlocked ? (
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
                <label>
                  <span>Client ID</span>
                  <input
                    value={snaptradeClientId}
                    onChange={(event) => setSnaptradeClientId(event.target.value)}
                    placeholder="SnapTrade client ID"
                    autoComplete="off"
                  />
                </label>
                <label>
                  <span>Consumer key</span>
                  <input
                    type="password"
                    value={snaptradeConsumerKey}
                    onChange={(event) => setSnaptradeConsumerKey(event.target.value)}
                    placeholder="Consumer key"
                    autoComplete="new-password"
                  />
                </label>
                <button
                  className="sync-save-button"
                  type="button"
                  disabled={
                    !isTauri() ||
                    integrationSaving !== null ||
                    !snaptradeClientId ||
                    !snaptradeConsumerKey
                  }
                  onClick={() => void saveIntegration('snaptrade')}
                >
                  <Save size={14} />
                  {integrationSaving === 'snaptrade' ? 'Testing…' : 'Save and test'}
                </button>
              </>
            )}
          </div>
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
        <SectionHeading title="Data sources" action={<RefreshButton />} />
        <div className="provider-list">
          {data.providers.map((provider) => (
            <button
              className="provider-row"
              type="button"
              key={provider.id}
              disabled={!['plaid', 'snaptrade'].includes(provider.id)}
              onClick={() => {
                if (isLinkProvider(provider.id)) void connectProvider(provider.id)
              }}
            >
              <span className="provider-icon">
                {provider.id === 'logos' ? <HardDrive size={17} /> : <Database size={17} />}
              </span>
              <span>
                <strong>{provider.name}</strong>
                <small>{provider.description}</small>
              </span>
              <span className="provider-status">
                <StatusDot tone={provider.status === 'error' ? 'negative' : 'positive'} />
                {linking === provider.id ? 'Waiting for browser…' : provider.lastSync}
              </span>
              <ChevronRight size={15} />
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
