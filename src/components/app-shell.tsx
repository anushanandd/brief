import { Link, Outlet, useNavigate } from '@tanstack/react-router'
import {
  ChartNoAxesCombined,
  Command,
  LayoutDashboard,
  Settings,
  ShoppingBag,
  WalletCards,
} from 'lucide-react'
import { useEffect } from 'react'

import { useUiStore } from '../store/ui'
import { CommandMenu } from './command-menu'

const navigation = [
  { to: '/', label: 'Overview', icon: LayoutDashboard, shortcut: '⌘1' },
  {
    to: '/analytics',
    label: 'Analytics',
    icon: ChartNoAxesCombined,
    shortcut: '⌘2',
  },
  { to: '/holdings', label: 'Holdings', icon: WalletCards, shortcut: '⌘3' },
  { to: '/spending', label: 'Spending', icon: ShoppingBag, shortcut: '⌘4' },
  { to: '/settings', label: 'Settings', icon: Settings, shortcut: '⌘5' },
] as const

export function AppShell() {
  const navigate = useNavigate()
  const setCommandOpen = useUiStore((state) => state.setCommandOpen)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const isTyping =
        event.target instanceof HTMLInputElement ||
        event.target instanceof HTMLTextAreaElement ||
        (event.target instanceof HTMLElement && event.target.isContentEditable)

      if (event.key === '/' && !isTyping) {
        const search = document.querySelector<HTMLInputElement>('[data-search]')
        if (search) {
          event.preventDefault()
          search.focus()
        }
        return
      }

      if (!(event.metaKey || event.ctrlKey)) return

      if (event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setCommandOpen(true)
        return
      }

      const item = navigation[Number(event.key) - 1]
      if (item) {
        event.preventDefault()
        void navigate({ to: item.to })
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [navigate, setCommandOpen])

  return (
    <div className="app-frame">
      <aside className="sidebar" aria-label="Primary navigation">
        <Link to="/" className="brand" aria-label="Brief overview">
          <span className="brand-mark">b</span>
          <span>brief</span>
        </Link>

        <nav className="sidebar-nav">
          {navigation.map(({ to, label, icon: Icon, shortcut }) => (
            <Link
              key={to}
              to={to}
              className="nav-link"
              activeProps={{ className: 'nav-link active' }}
              activeOptions={{ exact: to === '/' }}
            >
              <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
              <span>{label}</span>
              <kbd>{shortcut}</kbd>
            </Link>
          ))}
        </nav>

        <button className="command-trigger" onClick={() => setCommandOpen(true)} type="button">
          <Command size={16} aria-hidden="true" />
          <span>Quick actions</span>
          <kbd>⌘K</kbd>
        </button>

        <div className="privacy-note">
          <span className="privacy-lock" aria-hidden="true" />
          <span>
            Local workspace
            <small>Your data stays on this device.</small>
          </span>
        </div>
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

      <CommandMenu />
    </div>
  )
}
