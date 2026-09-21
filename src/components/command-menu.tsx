import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import {
  CreditCard,
  Database,
  Landmark,
  MessageCircle,
  RefreshCw,
  type LucideIcon,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useRefreshFinance } from '../hooks/use-refresh-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { generateFoundationExplanation, getFoundationModelStatus } from '../lib/api'
import { chatEvidence } from '../lib/money'
import { commandDestinations } from '../lib/navigation'
import { useOrderedNavigation } from '../lib/navigation-preferences'
import { getSpendingAccountId } from '../lib/spending-preferences'

type MenuAction = {
  id: string
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
  const pages = useOrderedNavigation()
  const navigate = useNavigate()
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const request = useRef<AbortController | undefined>(undefined)
  const [query, setQuery] = useState('')
  const [answer, setAnswer] = useState<string>()
  const [asking, setAsking] = useState(false)
  const actions: MenuAction[] = [
    ...[
      ...pages,
      ...commandDestinations.filter((item) => !pages.some(({ to }) => to === item.to)),
    ].map(({ label, to, icon, ...destination }) => ({
      id: `navigate:${to}`,
      label,
      search: `navigate ${label}`,
      group: 'Navigate' as const,
      icon,
      shortcut: 'shortcut' in destination ? destination.shortcut : undefined,
      run: () => navigate({ to }),
    })),
    ...(finance.data?.accounts ?? [])
      .filter(({ id }) => id !== 'all' && id !== getSpendingAccountId())
      .map((account) => ({
        id: `account:${account.id}`,
        label: accountDisplayName(account.id, account.name, getAccountDisplayNames()),
        search: `${accountDisplayName(account.id, account.name, getAccountDisplayNames())} ${account.name} ${account.institution} ${account.type} account`,
        group: 'Accounts' as const,
        icon: account.type === 'credit' ? CreditCard : Landmark,
        run: () => navigate({ to: '/accounts', search: { account: account.id } }),
      })),
    {
      id: 'refresh',
      label: 'Refresh snapshot',
      search: 'refresh data snapshot',
      group: 'Actions',
      icon: RefreshCw,
      shortcut: '⌘R',
      run: refresh,
    },
    {
      id: 'sources',
      label: 'Review data sources',
      search: 'data sources integrations providers',
      group: 'Actions',
      icon: Database,
      run: () => navigate({ to: '/settings' }),
    },
  ]
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

  useEffect(
    () => () => {
      request.current?.abort()
    },
    [],
  )

  const askBrief = async () => {
    if (!finance.data || query.trim().length < 4 || asking) return
    request.current?.abort()
    const controller = new AbortController()
    request.current = controller
    setAsking(true)
    setAnswer(undefined)
    try {
      const status = await getFoundationModelStatus()
      if (status.state !== 'available') throw new Error(status.message)
      const response = await generateFoundationExplanation(
        JSON.stringify({ question: query.trim(), evidence: chatEvidence(finance.data) }),
        controller.signal,
        'chat',
      )
      setAnswer(response)
    } catch (error) {
      if (!controller.signal.aborted)
        setAnswer(error instanceof Error ? error.message : String(error))
    } finally {
      if (!controller.signal.aborted) setAsking(false)
    }
  }

  const run = (action: MenuAction) => {
    onOpenChange(false)
    void Promise.resolve(action.run()).catch(() => undefined)
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
    >
      <h2 id="command-title" className="sr-only">
        Quick actions
      </h2>
      <div className="command-popup">
        <Command label="Quick actions">
          <label className="command-input-wrap">
            <span className="sr-only">Search quick actions</span>
            <Command.Input
              ref={input}
              value={query}
              onValueChange={(value) => {
                request.current?.abort()
                request.current = undefined
                setAsking(false)
                setQuery(value)
                setAnswer(undefined)
              }}
              placeholder="Go somewhere, run an action, or ask Brief…"
            />
            <kbd>esc</kbd>
          </label>
          <Command.List className="command-list" label="Quick actions">
            <Command.Empty className="command-empty">No matching action.</Command.Empty>
            {query.trim().length >= 4 ? (
              <Command.Group className="command-group" heading="Ask Brief">
                <Command.Item
                  value={`ask brief ${query}`}
                  keywords={[query, 'ask brief apple intelligence']}
                  disabled={asking}
                  onSelect={() => void askBrief()}
                >
                  <MessageCircle size={17} aria-hidden="true" />
                  <span>{asking ? 'Thinking on this Mac…' : `Ask “${query.trim()}”`}</span>
                  <kbd>↵</kbd>
                </Command.Item>
              </Command.Group>
            ) : null}
            {(['Navigate', 'Accounts', 'Actions'] as const).map((group) => {
              const grouped = actions.filter((action) => action.group === group)
              if (!grouped.length) return null
              return (
                <Command.Group className="command-group" heading={group} key={group}>
                  {grouped.map((action) => {
                    const Icon = action.icon
                    return (
                      <Command.Item
                        key={action.id}
                        value={action.id}
                        keywords={[action.label, action.search]}
                        onSelect={() => run(action)}
                      >
                        <Icon size={17} aria-hidden="true" />
                        <span>{action.label}</span>
                        {action.shortcut ? <kbd>{action.shortcut}</kbd> : null}
                      </Command.Item>
                    )
                  })}
                </Command.Group>
              )
            })}
          </Command.List>
          {answer ? (
            <div className="command-answer" aria-live="polite">
              <strong>Brief</strong>
              <p>{answer}</p>
            </div>
          ) : null}
        </Command>
      </div>
    </dialog>
  )
}
