import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import { useEffect, useMemo, useRef, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useFinanceRefreshState, useRefreshFinance } from '../hooks/use-refresh-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import {
  activityMatchesSearch,
  activityMethod,
  buildActivities,
  linkedAccountIds,
} from '../lib/activity'
import { localDateKey, validDate } from '../lib/analytics'
import { useOrderedAnalyticsNavigation } from '../lib/analytics-order-preferences'
import { generateFoundationExplanation, getFoundationModelStatus, isTauri } from '../lib/api'
import { chatEvidence } from '../lib/money'
import { secondaryNavigation } from '../lib/navigation'
import { useOrderedNavigation } from '../lib/navigation-preferences'
import type { FinanceSnapshot } from '../lib/schema'
import { transactionDateKey } from '../lib/spending'
import {
  Activity,
  CalendarDays,
  CreditCard,
  Database,
  Filter,
  HelpCircle,
  Landmark,
  List,
  MessageCircle,
  RefreshCw,
  Search,
  TrendingUp,
  type IconComponent,
} from './icons'

type MenuAction = {
  id: string
  label: string
  search: string
  group:
    | 'Activity'
    | 'Filters'
    | 'Navigate'
    | 'Analytics'
    | 'Accounts'
    | 'Holdings'
    | 'Focus'
    | 'Actions'
  icon: IconComponent
  detail?: string
  shortcut?: string
  disabled?: boolean
  run: () => unknown
}

export function heldSecurities(
  holdings: readonly Pick<FinanceSnapshot['holdings'][number], 'ticker' | 'name'>[],
) {
  return [...new Map(holdings.map(({ ticker, name }) => [ticker, name])).entries()].map(
    ([ticker, name]) => ({ ticker, name }),
  )
}

export function paletteQuestion(query: string) {
  const trimmed = query.trim()
  return trimmed.startsWith('?') ? trimmed.slice(1).trim() : null
}

export function paletteFilter(value: string, search: string, keywords: string[] = []) {
  const query = search.trim().toLowerCase()
  if (value.startsWith('holding:') && value.slice(8).toLowerCase() === query) return 2
  const haystack = [value, ...keywords].join(' ').toLowerCase()
  if (!query.split(/\s+/).every((term) => haystack.includes(term))) return 0
  if (value.startsWith('activity:search:')) return 1.5
  return value.startsWith('activity:') || value.startsWith('activity-filter:') ? 0.5 : 1
}

export function paletteDateRange(query: string) {
  const match = /^(?:date:\s*)?(\d{4}-\d{2}-\d{2})(?:\.\.(\d{4}-\d{2}-\d{2}))?$/.exec(query.trim())
  if (!match) return undefined
  const from = validDate(match[1])
  const to = validDate(match[2] ?? match[1])
  return from && to && from <= to ? { from, to } : undefined
}

export function CommandMenu({
  open,
  onOpenChange,
  onShowShortcuts,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onShowShortcuts: () => void
}) {
  const pages = useOrderedNavigation()
  const analyticsNavigation = useOrderedAnalyticsNavigation()
  const navigate = useNavigate()
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const isRefreshing = useFinanceRefreshState()
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const request = useRef<AbortController | undefined>(undefined)
  const [query, setQuery] = useState('')
  const [answer, setAnswer] = useState<string>()
  const [askError, setAskError] = useState<string>()
  const [asking, setAsking] = useState(false)
  const trimmedQuery = query.trim()
  const question = paletteQuestion(query)
  const askMode = question !== null
  const model = useQuery({
    queryKey: ['foundation-model-status'],
    queryFn: getFoundationModelStatus,
    enabled: askMode && (question?.length ?? 0) >= 4 && isTauri() && Boolean(finance.data),
    staleTime: 60_000,
  })
  const canAsk = Boolean(finance.data && model.data?.state === 'available')
  const names = getAccountDisplayNames()
  const activities = useMemo(
    () => (finance.data ? buildActivities(finance.data, getAccountDisplayNames()) : []),
    [finance.data],
  )
  const activityAccountIds = linkedAccountIds(
    activities.map(({ accountId }) => accountId),
    finance.data?.accountLinks,
  )
  const activityTerm = trimmedQuery
    .replace(/^(description|account|category|method|location|amount|date):\s*/i, '')
    .trim()
  const dateRange = paletteDateRange(trimmedQuery)
  const isRangeQuery = Boolean(dateRange && dateRange.from !== dateRange.to)
  const activityMatches =
    !askMode && activityTerm.length >= 2 && !isRangeQuery
      ? activities.filter((activity) => activityMatchesSearch(activity, trimmedQuery))
      : []
  const recentDates =
    !dateRange && /^date:/i.test(trimmedQuery)
      ? [
          ...new Set(
            activities
              .map((activity) =>
                validDate(transactionDateKey(activity.date, finance.data?.updatedAt ?? '')),
              )
              .filter((date): date is string => Boolean(date)),
          ),
        ].slice(0, 12)
      : []
  const focusRegions =
    open && typeof document !== 'undefined'
      ? [...document.querySelectorAll<HTMLElement>('[data-keyboard-region]')].filter(
          (region) =>
            region.getClientRects().length > 0 &&
            !region.parentElement?.closest('[data-keyboard-region]') &&
            Boolean(region.getAttribute('aria-label')),
        )
      : []
  const actions: MenuAction[] = [
    ...(trimmedQuery && !askMode
      ? [
          ...(!isRangeQuery
            ? [
                {
                  id: `activity:search:${trimmedQuery}`,
                  label: `Search Activity for “${trimmedQuery}”`,
                  detail: !finance.data
                    ? 'Local activity unavailable'
                    : activityTerm.length < 2
                      ? 'Search local activity'
                      : `${activityMatches.length} matching ${activityMatches.length === 1 ? 'entry' : 'entries'}`,
                  search: `activity search ${trimmedQuery}`,
                  group: 'Activity' as const,
                  icon: Search,
                  disabled: !finance.data,
                  run: () => navigate({ to: '/activities', search: { q: trimmedQuery } }),
                },
              ]
            : []),
          ...(activityTerm.length >= 2 && !isRangeQuery
            ? activityMatches.slice(0, 8).map((activity) => {
                const date = transactionDateKey(activity.date, finance.data?.updatedAt ?? '')
                return {
                  id: `activity:${activity.id}`,
                  label: activity.title,
                  detail: [activity.description, activity.account, activity.category, date]
                    .filter(Boolean)
                    .join(' · '),
                  search: `activity ${trimmedQuery}`,
                  group: 'Activity' as const,
                  icon: Activity,
                  run: () =>
                    navigate({
                      to: '/activities',
                      search: {
                        q: `description:${activity.title}`,
                        from: validDate(date),
                        to: validDate(date),
                      },
                    }),
                }
              })
            : []),
          ...(finance.data?.accounts ?? [])
            .filter(({ id }) => id !== 'all' && activityAccountIds.has(id))
            .map((account) => {
              const label = accountDisplayName(account.id, account.name, names)
              return {
                id: `activity-filter:account:${account.id}`,
                label: `Account: ${label}`,
                search: `activity filter account:${label} ${account.name} ${account.institution}`,
                group: 'Filters' as const,
                icon: Filter,
                run: () => navigate({ to: '/activities', search: { account: account.id } }),
              }
            }),
          ...[...new Set(activities.map(({ category }) => category || 'Other'))].map(
            (category) => ({
              id: `activity-filter:category:${category}`,
              label: `Category: ${category}`,
              search: `activity filter category:${category}`,
              group: 'Filters' as const,
              icon: Filter,
              run: () => navigate({ to: '/activities', search: { category } }),
            }),
          ),
          ...[...new Set(activities.map(activityMethod).filter(Boolean))].map((method) => ({
            id: `activity-filter:method:${method}`,
            label: `Method: ${method}`,
            search: `activity filter method:${method} channel:${method}`,
            group: 'Filters' as const,
            icon: Filter,
            run: () => navigate({ to: '/activities', search: { method } }),
          })),
          ...(dateRange
            ? [
                {
                  id: 'activity-filter:date',
                  label:
                    dateRange.from === dateRange.to
                      ? `Date: ${dateRange.from}`
                      : `Dates: ${dateRange.from} – ${dateRange.to}`,
                  search: `activity filter ${trimmedQuery}`,
                  group: 'Filters' as const,
                  icon: CalendarDays,
                  run: () => navigate({ to: '/activities', search: dateRange }),
                },
              ]
            : []),
          ...recentDates.map((date) => ({
            id: `activity-filter:date:${date}`,
            label: `Date: ${date}`,
            search: `activity filter date:${date}`,
            group: 'Filters' as const,
            icon: CalendarDays,
            run: () => navigate({ to: '/activities', search: { from: date, to: date } }),
          })),
        ]
      : []),
    ...pages.map(({ label, to, icon, ...destination }) => ({
      id: `navigate:${to}`,
      label,
      search: `navigate ${label}`,
      group: 'Navigate' as const,
      icon,
      shortcut: 'shortcut' in destination ? destination.shortcut : undefined,
      run: () => navigate({ to }),
    })),
    ...(trimmedQuery
      ? [
          ...secondaryNavigation.map(({ to, label, icon }) => ({
            id: `navigate:${to}`,
            label,
            search: `navigate ${label}`,
            group: 'Navigate' as const,
            icon,
            run: () => navigate({ to }),
          })),
          ...analyticsNavigation.map(({ label, icon, search }) => ({
            id: `analytics:${search.chart}`,
            label,
            search: `analytics chart ${label}`,
            group: 'Analytics' as const,
            icon,
            run: () => navigate({ to: '/analytics', search }),
          })),
          ...(finance.data?.accounts ?? [])
            .filter(({ id }) => id !== 'all')
            .map((account) => {
              const label = accountDisplayName(account.id, account.name, names)
              return {
                id: `account:${account.id}`,
                label,
                search: `${label} ${account.name} ${account.institution} ${account.type} account`,
                group: 'Accounts' as const,
                icon: account.type === 'credit' ? CreditCard : Landmark,
                run: () => navigate({ to: '/accounts', search: { account: account.id } }),
              }
            }),
          ...heldSecurities(finance.data?.holdings ?? []).map(({ ticker, name }) => ({
            id: `holding:${ticker}`,
            label: `${ticker} · ${name}`,
            search: `${ticker} ${name} holding security`,
            group: 'Holdings' as const,
            icon: TrendingUp,
            run: () => navigate({ to: '/holdings', search: { ticker } }),
          })),
        ]
      : []),
    ...focusRegions.map((region, index) => ({
      id: `focus:${index}`,
      label: `Focus ${region.getAttribute('aria-label')}`,
      search: `focus list ${region.getAttribute('aria-label')}`,
      group: 'Focus' as const,
      icon: List,
      run: () => {
        window.requestAnimationFrame(() => {
          const target = region.querySelector<HTMLElement>('[data-keyboard-row]') ?? region
          target.focus()
          target.scrollIntoView({ block: 'nearest' })
        })
      },
    })),
    {
      id: 'refresh',
      label: isRefreshing ? 'Refreshing snapshot…' : 'Refresh snapshot',
      search: 'refresh data snapshot',
      group: 'Actions',
      icon: RefreshCw,
      shortcut: '⌘R',
      disabled: isRefreshing || Boolean(finance.data?.recovery),
      run: refresh,
    },
    {
      id: 'shortcuts',
      label: 'Keyboard shortcuts',
      search: 'keyboard shortcuts help',
      group: 'Actions',
      icon: HelpCircle,
      run: onShowShortcuts,
    },
    ...(trimmedQuery
      ? [
          {
            id: 'sources',
            label: 'Review data sources',
            search: 'data sources integrations providers connections',
            group: 'Actions' as const,
            icon: Database,
            run: () => navigate({ to: '/settings', hash: 'data-sources' }),
          },
        ]
      : []),
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
    if (!finance.data || !canAsk || question === null || question.length < 4 || asking) return
    request.current?.abort()
    const controller = new AbortController()
    request.current = controller
    setAsking(true)
    setAnswer(undefined)
    setAskError(undefined)
    try {
      const response = await generateFoundationExplanation(
        question,
        JSON.stringify(chatEvidence(finance.data, localDateKey(Date.now() / 1000))),
        controller.signal,
      )
      setAnswer(response)
    } catch (error) {
      if (!controller.signal.aborted)
        setAskError(error instanceof Error ? error.message : String(error))
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
      className="command-dialog command-palette-dialog"
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
        <Command label="Quick actions" shouldFilter={!askMode} filter={paletteFilter}>
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
                setAskError(undefined)
              }}
              placeholder="Search activity, pages, accounts · ? to ask"
            />
            <kbd>esc</kbd>
          </label>
          <Command.List className="command-list" label="Quick actions">
            <Command.Empty className="command-empty">
              <span role="status">
                {askMode
                  ? !finance.data
                    ? 'Load a snapshot to ask Brief.'
                    : !isTauri()
                      ? 'Apple Intelligence requires the Brief desktop app.'
                      : question.length < 4
                        ? 'Type at least four characters after ? to ask Brief.'
                        : model.isPending
                          ? 'Checking Apple Intelligence…'
                          : model.data?.state !== 'available'
                            ? (model.data?.message ?? 'Apple Intelligence is unavailable.')
                            : 'Ask Brief is ready.'
                  : 'No matching action or activity.'}
              </span>
            </Command.Empty>
            {askMode && canAsk && question.length >= 4 ? (
              <Command.Group className="command-group" heading="Ask Brief">
                <Command.Item value="ask-brief" disabled={asking} onSelect={() => void askBrief()}>
                  <MessageCircle size={17} aria-hidden="true" />
                  <span>{asking ? 'Thinking on this Mac…' : `Ask “${question}”`}</span>
                  <kbd>↵</kbd>
                </Command.Item>
              </Command.Group>
            ) : null}
            {!askMode &&
              (
                [
                  'Holdings',
                  'Navigate',
                  'Analytics',
                  'Accounts',
                  'Activity',
                  'Filters',
                  'Focus',
                  'Actions',
                ] as const
              ).map((group) => {
                const grouped = actions.filter((action) => action.group === group)
                if (!grouped.length) return null
                return (
                  <Command.Group className="command-group" heading={group} key={group}>
                    {grouped.map((action) => {
                      const Icon = action.icon
                      return (
                        <Command.Item
                          key={action.id}
                          className={action.detail ? 'command-result-item' : undefined}
                          value={action.id}
                          keywords={[action.label, action.search]}
                          disabled={action.disabled}
                          aria-busy={action.id === 'refresh' ? isRefreshing : undefined}
                          onSelect={() => run(action)}
                        >
                          <Icon size={17} aria-hidden="true" />
                          <span className="command-item-copy">
                            <span>{action.label}</span>
                            {action.detail ? <small>{action.detail}</small> : null}
                          </span>
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
          {askError ? (
            <p className="command-answer" role="alert">
              {askError}
            </p>
          ) : null}
        </Command>
      </div>
    </dialog>
  )
}
