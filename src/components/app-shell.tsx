import { useQueryClient, useMutation } from '@tanstack/react-query'
import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import { House, Landmark, ListTree, Settings } from 'lucide-react'
import { useEffect, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { financeQueryKey } from '../hooks/use-finance'
import { useAutoRefreshFinance, useRefreshFinance } from '../hooks/use-refresh-finance'
import { isTauri, recoverFinanceState } from '../lib/api'
import { CommandMenu } from './command-menu'
import { Button } from './ui'

const primaryNavigation = [
  { to: '/', label: 'Home', icon: House, shortcut: '⌘1' },
  { to: '/accounts', label: 'Accounts', icon: Landmark, shortcut: '⌘2' },
  { to: '/activities', label: 'Activity', icon: ListTree, shortcut: '⌘3' },
] as const
const settingsNavigation = {
  to: '/settings',
  label: 'Settings',
  icon: Settings,
  shortcut: '⌘4',
} as const
const navigation = [...primaryNavigation, settingsNavigation]

export function AppShell() {
  const finance = useFinance()
  const navigate = useNavigate()
  const refresh = useRefreshFinance()
  const [commandOpen, setCommandOpen] = useState(false)
  const client = useQueryClient()
  useAutoRefreshFinance(
    finance.data?.updatedAt,
    isTauri() &&
      !finance.data?.recovery &&
      !finance.integrationStatusLoading &&
      Object.values(finance.integrationStatus).some(Boolean),
  )
  const recovery = useMutation({
    mutationFn: recoverFinanceState,
    onSuccess: (snapshot) => {
      client.setQueryData(financeQueryKey, snapshot)
      void client.invalidateQueries({ queryKey: ['transaction-annotations'] })
      void client.invalidateQueries({ queryKey: ['market-snapshots'] })
    },
  })

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return

      if (event.metaKey && event.key.toLowerCase() === 'r') {
        event.preventDefault()
        if (!event.repeat) void refresh().catch(() => undefined)
        return
      }

      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault()
        return
      }

      if (event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setCommandOpen(true)
        return
      }

      if (event.key === ',') {
        event.preventDefault()
        void navigate({ to: '/settings' })
        return
      }

      const item = navigation[Number(event.key) - 1]
      if (item) {
        event.preventDefault()
        void navigate({ to: item.to })
      }
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown, { capture: true })
    if (isTauri()) {
      void listen('open-settings', () => navigate({ to: '/settings' })).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }

    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown, { capture: true })
    }
  }, [navigate, refresh, setCommandOpen])

  return (
    <div className="app-frame">
      <div className="window-drag-region" data-tauri-drag-region aria-hidden="true" />
      <aside className="sidebar" aria-label="Primary navigation">
        <Link to="/" className="brand" aria-label="Brief home">
          <span className="brand-name">Brief</span>
        </Link>

        <nav className="sidebar-nav">
          {primaryNavigation.map(({ to, label, icon: Icon, shortcut }) => (
            <Link
              key={to}
              to={to}
              className="nav-link"
              activeProps={{ className: 'nav-link active' }}
              activeOptions={{ exact: to === '/' }}
              aria-label={label}
              title={`${label} (${shortcut})`}
            >
              <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
              <span>{label}</span>
            </Link>
          ))}
        </nav>
        <nav className="sidebar-nav sidebar-nav-secondary" aria-label="Application settings">
          <Link
            to={settingsNavigation.to}
            className="nav-link"
            activeProps={{ className: 'nav-link active' }}
            aria-label={settingsNavigation.label}
            title={`${settingsNavigation.label} (${settingsNavigation.shortcut})`}
          >
            <Settings size={17} strokeWidth={1.8} aria-hidden="true" />
            <span>{settingsNavigation.label}</span>
          </Link>
        </nav>
      </aside>

      <main className="main-content">
        {finance.data?.recovery ? (
          <section className="sync-notice" aria-label="Local data recovery">
            <h2>Local data needs recovery</h2>
            <p>{finance.data.recovery.message}</p>
            {finance.data.recovery.canRestore ? (
              <Button
                variant="primary"
                disabled={recovery.isPending}
                onClick={() => recovery.mutate(true)}
              >
                Restore retained snapshot
              </Button>
            ) : null}
            <Button
              variant="destructive"
              disabled={recovery.isPending}
              onClick={() => {
                if (
                  window.confirm(
                    'Start a new empty local snapshot? Original files will be retained and provider credentials will remain in Keychain.',
                  )
                )
                  recovery.mutate(false)
              }}
            >
              Start new snapshot
            </Button>
            {recovery.isError ? <p role="alert">{String(recovery.error)}</p> : null}
          </section>
        ) : null}
        {finance.data?.syncWarnings?.length ? (
          <p className="sync-notice" role="status">
            Data notices: {finance.data.syncWarnings.join(' · ')}
          </p>
        ) : null}
        {finance.annotationWarning ? (
          <p className="sync-notice" role="status">
            {finance.annotationWarning}
          </p>
        ) : null}
        {finance.data?.recovery && !finance.data.recovery.canRestore ? null : <Outlet />}
      </main>

      {commandOpen ? <CommandMenu open onOpenChange={setCommandOpen} /> : null}
    </div>
  )
}
