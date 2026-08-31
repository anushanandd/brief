import { Switch } from '@base-ui/react/switch'
import { Check, ChevronRight, Database, HardDrive, KeyRound, Laptop, Moon, Sun } from 'lucide-react'
import { useTheme } from 'next-themes'

import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'

function SettingsToggle({
  label,
  description,
  defaultChecked,
}: {
  label: string
  description: string
  defaultChecked?: boolean
}) {
  return (
    <div className="setting-row">
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <Switch.Root className="switch-root" defaultChecked={defaultChecked} aria-label={label}>
        <Switch.Thumb className="switch-thumb" />
      </Switch.Root>
    </div>
  )
}

export function SettingsPage() {
  const query = useFinance()
  const { theme, setTheme } = useTheme()
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
              onClick={() =>
                window.alert(
                  `${provider.name} uses seeded data in the MVP. Provider credentials are not yet enabled.`,
                )
              }
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
                {provider.lastSync}
              </span>
              <ChevronRight size={15} />
            </button>
          ))}
        </div>
      </Card>

      <Card>
        <SectionHeading title="Privacy and updates" />
        <SettingsToggle
          label="Refresh on launch"
          description="Read provider data after Brief opens."
          defaultChecked
        />
        <SettingsToggle
          label="After-hours quotes"
          description="Include supported premarket and postmarket prices."
          defaultChecked
        />
        <SettingsToggle
          label="Usage diagnostics"
          description="Share anonymous stability information. No financial data."
        />
        <button
          className="secure-action"
          type="button"
          onClick={() =>
            window.alert(
              'Credential management will use the operating system keychain in the provider-enabled release.',
            )
          }
        >
          <span className="provider-icon">
            <KeyRound size={17} />
          </span>
          <span>
            <strong>Manage credentials</strong>
            <small>Stored in the operating system keychain when providers are enabled.</small>
          </span>
          <ChevronRight size={15} />
        </button>
      </Card>

      <p className="settings-footnote">Brief 0.1.0 · Local MVP · Not financial advice</p>
    </div>
  )
}
