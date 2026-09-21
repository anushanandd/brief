import { useQueryClient, useMutation } from '@tanstack/react-query'
import { Link, Outlet, useNavigate, useLocation } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import { FilePlus, History } from 'lucide-react'
import { useEffect, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { financeQueryKey } from '../hooks/use-finance'
import { useAutoRefreshFinance, useRefreshFinance } from '../hooks/use-refresh-finance'
import { isTauri, recoverFinanceState } from '../lib/api'
import { shortcutInput, shortcutOverlayOpen } from '../lib/keyboard'
import { navigation, settingsNavigation } from '../lib/navigation'
import { adjacentPage, useOrderedNavigation } from '../lib/navigation-preferences'
import { CommandMenu } from './command-menu'
import { ShortcutHelp } from './shortcut-help'
import { Button } from './ui'

export function browserHistoryDirection({ metaKey, key }: Pick<KeyboardEvent, 'metaKey' | 'key'>) {
  if (!metaKey) return undefined
  if (key === '[') return 'back' as const
  if (key === ']') return 'forward' as const
  return undefined
}

type BareShortcutEvent = Pick<KeyboardEvent, 'altKey' | 'ctrlKey' | 'key' | 'metaKey' | 'shiftKey'>

const hasModifier = (event: BareShortcutEvent) =>
  event.altKey || event.ctrlKey || event.metaKey || event.shiftKey

export function navigationShortcutIndex(event: BareShortcutEvent, isEditing: boolean) {
  if (isEditing || hasModifier(event)) return undefined
  const index = Number(event.key) - 1
  return Number.isInteger(index) && index >= 0 && index < navigation.length ? index : undefined
}

export function sidebarShortcutDirection(event: BareShortcutEvent, isEditing: boolean) {
  if (isEditing || !event.metaKey || event.altKey || event.ctrlKey || event.shiftKey)
    return undefined
  return event.key === 'ArrowUp' ? -1 : event.key === 'ArrowDown' ? 1 : undefined
}

export function listNavigationAction(event: BareShortcutEvent, isEditing: boolean) {
  if (isEditing || hasModifier(event)) return undefined
  if (event.key.toLowerCase() === 'j') return 'next' as const
  if (event.key.toLowerCase() === 'k') return 'previous' as const
  if (event.key === 'Enter') return 'open' as const
  return undefined
}

export function shortcutHelpShortcut(event: BareShortcutEvent, isEditing: boolean) {
  return event.key === '?' && !isEditing && !event.altKey && !event.ctrlKey && !event.metaKey
}

export function AppShell() {
  const pages = useOrderedNavigation()
  const pathname = useLocation({ select: (location) => location.pathname })
  const settingsLast = pages.at(-1)?.to === '/settings'
  const SettingsIcon = settingsNavigation.icon
  const finance = useFinance()
  const navigate = useNavigate()
  const refresh = useRefreshFinance()
  const [commandOpen, setCommandOpen] = useState(false)
  const [shortcutHelpOpen, setShortcutHelpOpen] = useState(false)
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
      const target = event.target
      const isEditing = shortcutInput(target)
      if (
        event.defaultPrevented ||
        event.isComposing ||
        commandOpen ||
        shortcutHelpOpen ||
        shortcutOverlayOpen()
      )
        return

      if (shortcutHelpShortcut(event, isEditing)) {
        event.preventDefault()
        setShortcutHelpOpen(true)
        return
      }

      const navigationIndex = navigationShortcutIndex(event, isEditing)
      if (navigationIndex != null) {
        event.preventDefault()
        void navigate({ to: pages[navigationIndex].to })
        return
      }

      const pageDirection = sidebarShortcutDirection(event, isEditing)
      if (pageDirection) {
        event.preventDefault()
        void navigate({
          to: adjacentPage(
            pages.map(({ to }) => to),
            pathname,
            pageDirection,
          ),
        })
        return
      }

      const listAction = listNavigationAction(event, isEditing)
      if (listAction) {
        const rows = [...document.querySelectorAll<HTMLElement>('[data-keyboard-row]')].filter(
          (row) => row.getClientRects().length > 0,
        )
        const focusedRow =
          document.activeElement instanceof HTMLElement
            ? document.activeElement.closest<HTMLElement>('[data-keyboard-row]')
            : null
        if (listAction === 'open') {
          const opener = focusedRow?.matches('[data-keyboard-open]')
            ? focusedRow
            : focusedRow?.querySelector<HTMLElement>('[data-keyboard-open]')
          if (opener) {
            event.preventDefault()
            opener.click()
          }
        } else if (rows.length) {
          event.preventDefault()
          const currentIndex = focusedRow
            ? rows.indexOf(focusedRow)
            : listAction === 'next'
              ? -1
              : 0
          const nextIndex =
            listAction === 'next'
              ? (currentIndex + 1) % rows.length
              : (currentIndex - 1 + rows.length) % rows.length
          rows[nextIndex].focus()
          rows[nextIndex].scrollIntoView({ block: 'nearest' })
        }
        return
      }

      if (!(event.metaKey || event.ctrlKey)) return

      const historyDirection = browserHistoryDirection(event)
      if (historyDirection) {
        event.preventDefault()
        if (historyDirection === 'back') window.history.back()
        else window.history.forward()
        return
      }

      if (event.metaKey && event.key.toLowerCase() === 'r') {
        event.preventDefault()
        if (!event.repeat) void refresh().catch(() => undefined)
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
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen('open-settings', () => navigate({ to: '/settings' })).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }

    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [commandOpen, navigate, refresh, shortcutHelpOpen, pages, pathname])

  return (
    <div className="app-frame">
      <div className="window-drag-region" data-tauri-drag-region aria-hidden="true" />
      <aside className="sidebar" aria-label="Primary navigation">
        <Link to="/" className="brand" aria-label="Brief home">
          <span className="brand-name">Brief</span>
        </Link>

        <nav className="sidebar-nav">
          {(settingsLast ? pages.slice(0, -1) : pages).map(({ to, label, icon: Icon }) => (
            <Link
              key={to}
              to={to}
              className="nav-link"
              activeProps={{ className: 'nav-link active' }}
              activeOptions={{ exact: to === '/' }}
              aria-label={label}
            >
              <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
              <span>{label}</span>
            </Link>
          ))}
        </nav>
        {settingsLast ? (
          <nav className="sidebar-nav sidebar-nav-secondary" aria-label="Application settings">
            <Link
              to={settingsNavigation.to}
              className="nav-link"
              activeProps={{ className: 'nav-link active' }}
              aria-label={settingsNavigation.label}
            >
              <SettingsIcon size={17} strokeWidth={1.8} aria-hidden="true" />
              <span>{settingsNavigation.label}</span>
            </Link>
          </nav>
        ) : null}
      </aside>

      <main className="main-content">
        {finance.data?.recovery ? (
          <section className="sync-notice" aria-label="Local data recovery">
            <h2>Local data needs recovery</h2>
            <p>{finance.data.recovery.message}</p>
            {finance.data.recovery.canRestore ? (
              <Button
                icon={History}
                variant="primary"
                disabled={recovery.isPending}
                onClick={() => recovery.mutate(true)}
              >
                Restore retained snapshot
              </Button>
            ) : null}
            <Button
              icon={FilePlus}
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
      {shortcutHelpOpen ? <ShortcutHelp onClose={() => setShortcutHelpOpen(false)} /> : null}
    </div>
  )
}
