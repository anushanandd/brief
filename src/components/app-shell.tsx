import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import { LayoutDashboard, Settings } from 'lucide-react'
import { useEffect, useState } from 'react'

import { isTauri } from '../lib/api'
import { CommandMenu } from './command-menu'

const navigation = [
  { to: '/', label: 'Overview', icon: LayoutDashboard, shortcut: '⌘1' },
  { to: '/settings', label: 'Settings', icon: Settings, shortcut: '⌘2' },
] as const

export function AppShell() {
  const navigate = useNavigate()
  const [commandOpen, setCommandOpen] = useState(false)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return

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
  }, [navigate, setCommandOpen])

  return (
    <div className="app-frame">
      <div className="window-drag-region" data-tauri-drag-region aria-hidden="true" />
      <aside className="sidebar" aria-label="Primary navigation">
        <Link to="/" className="brand" aria-label="Brief overview">
          <span className="brand-mark">b</span>
        </Link>

        <nav className="sidebar-nav">
          {navigation.map(({ to, label, icon: Icon, shortcut }) => (
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
            </Link>
          ))}
        </nav>
      </aside>

      <main className="main-content">
        <Outlet />
      </main>

      <nav className="mobile-nav" aria-label="Primary navigation">
        {navigation.map(({ to, label, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            className="mobile-nav-link"
            activeProps={{ className: 'mobile-nav-link active' }}
            activeOptions={{ exact: to === '/' }}
          >
            <Icon size={19} strokeWidth={1.8} aria-hidden="true" />
            <span>{label}</span>
          </Link>
        ))}
      </nav>

      <CommandMenu open={commandOpen} onOpenChange={setCommandOpen} />
    </div>
  )
}
