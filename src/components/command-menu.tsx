import { useNavigate } from '@tanstack/react-router'
import {
  CreditCard,
  Database,
  House,
  Landmark,
  ListTree,
  RefreshCw,
  Settings,
  type LucideIcon,
} from 'lucide-react'
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useRefreshFinance } from '../hooks/use-refresh-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'

const destinations = [
  { label: 'Home', to: '/', icon: House },
  { label: 'Accounts', to: '/accounts', icon: Landmark },
  { label: 'Activity', to: '/activities', icon: ListTree },
  { label: 'Settings', to: '/settings', icon: Settings },
  { label: 'Investment accounts', to: '/accounts/investments', icon: Landmark },
  { label: 'Cash & cards', to: '/accounts/cash', icon: CreditCard },
  { label: 'Spending', to: '/activities/spending', icon: CreditCard },
  { label: 'Subscriptions', to: '/activities/subscriptions', icon: RefreshCw },
  { label: 'Trades', to: '/activities/trades', icon: Landmark },
  { label: 'Balance changes', to: '/activities/changes', icon: ListTree },
  { label: 'Benefits', to: '/activities/benefits', icon: CreditCard },
] as const

type MenuAction = {
  label: string
  search: string
  group: 'Navigate' | 'Accounts' | 'Actions'
  icon: LucideIcon
  shortcut?: string
  run: () => unknown
}

export function CommandMenu({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const [search, setSearch] = useState('')
  const [activeIndex, setActiveIndex] = useState(0)
  const actions: MenuAction[] = [
    ...destinations.map(({ label, to, icon }, index) => ({
      label,
      search: `navigate ${label}`,
      group: 'Navigate' as const,
      icon,
      shortcut: index < 4 ? `⌘${index + 1}` : undefined,
      run: () => navigate({ to }),
    })),
    ...(finance.data?.accounts ?? [])
      .filter(({ id }) => id !== 'all')
      .map((account) => ({
        label: accountDisplayName(account.id, account.name, getAccountDisplayNames()),
        search: `${account.name} ${account.institution} ${account.type} account`,
        group: 'Accounts' as const,
        icon: account.type === 'credit' ? CreditCard : Landmark,
        run: () => navigate({ to: '/accounts/$accountId', params: { accountId: account.id } }),
      })),
    {
      label: 'Refresh snapshot',
      search: 'refresh data snapshot',
      group: 'Actions',
      icon: RefreshCw,
      shortcut: '⌘R',
      run: refresh,
    },
    {
      label: 'Review data sources',
      search: 'data sources integrations providers',
      group: 'Actions',
      icon: Database,
      run: () => navigate({ to: '/settings' }),
    },
  ]
  const normalizedSearch = search.trim().toLocaleLowerCase()
  const visibleActions = normalizedSearch
    ? actions.filter((action) => action.search.toLocaleLowerCase().includes(normalizedSearch))
    : actions

  useEffect(() => {
    const element = dialog.current
    if (!element) return undefined
    if (open && !element.open) {
      element.showModal()
      input.current?.focus()
    } else if (!open && element.open) {
      element.close()
    }
    return () => {
      if (element.open) element.close()
    }
  }, [open])

  useEffect(() => setActiveIndex(0), [search])

  const run = (action: MenuAction) => {
    onOpenChange(false)
    void Promise.resolve(action.run()).catch(() => undefined)
  }
  const handleKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      onOpenChange(false)
      return
    }
    if (!visibleActions.length) return
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const offset = event.key === 'ArrowDown' ? 1 : -1
      setActiveIndex(
        (current) => (current + offset + visibleActions.length) % visibleActions.length,
      )
    } else if (event.key === 'Enter') {
      event.preventDefault()
      const action = visibleActions[activeIndex]
      if (action) run(action)
    }
  }

  return (
    <dialog
      ref={dialog}
      className="command-dialog"
      aria-labelledby="command-title"
      onCancel={(event) => {
        event.preventDefault()
        onOpenChange(false)
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onOpenChange(false)
      }}
      onKeyDown={handleKeyDown}
    >
      <h2 id="command-title" className="sr-only">
        Quick actions
      </h2>
      <div className="command-popup">
        <label className="command-input-wrap">
          <span className="sr-only">Search quick actions</span>
          <input
            ref={input}
            type="search"
            value={search}
            placeholder="Go somewhere or run an action…"
            autoComplete="off"
            onChange={(event) => setSearch(event.target.value)}
          />
          <kbd>esc</kbd>
        </label>
        <div className="command-list">
          {visibleActions.length ? (
            (['Navigate', 'Accounts', 'Actions'] as const).map((group) => {
              const grouped = visibleActions.filter((action) => action.group === group)
              if (!grouped.length) return null
              return (
                <section className="command-group" key={group} aria-label={group}>
                  <h3>{group}</h3>
                  {grouped.map((action) => {
                    const index = visibleActions.indexOf(action)
                    const Icon = action.icon
                    return (
                      <button
                        key={`${group}:${action.label}`}
                        type="button"
                        className={index === activeIndex ? 'active' : undefined}
                        tabIndex={-1}
                        onPointerMove={() => setActiveIndex(index)}
                        onClick={() => run(action)}
                      >
                        <Icon size={17} aria-hidden="true" />
                        <span>{action.label}</span>
                        {action.shortcut ? <kbd>{action.shortcut}</kbd> : null}
                      </button>
                    )
                  })}
                </section>
              )
            })
          ) : (
            <p className="command-empty">No matching action.</p>
          )}
        </div>
      </div>
    </dialog>
  )
}
