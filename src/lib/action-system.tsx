import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useState,
  type ReactNode,
} from 'react'

import type { IconComponent } from '../components/icons'
import { preference } from './preference'

/**
 * Where an action's keys work: anywhere (`app`), on the page with nothing over it (`page`), in
 * the focused list (`region`), on selected rows (`selection`), or inside one open dialog
 * (`dialog`, which names it in `dialog`).
 */
export type ActionScope = 'app' | 'page' | 'region' | 'selection' | 'dialog'

export type ActionBinding = {
  sequence: readonly string[]
  label: string
}

export type AppAction = {
  id: string
  title: string
  search?: string
  group?: string
  detail?: string
  icon?: IconComponent
  scope: ActionScope
  /** A `dialog` action's dialog; its keys work only while that dialog holds the keyboard. */
  dialog?: Element | null
  bindings?: readonly ActionBinding[]
  disabled?: boolean
  disabledReason?: string
  run: () => unknown
}

type Registration = {
  id: string
  actions: readonly AppAction[]
}

type ActionRegistry = {
  register: (registration: Registration) => void
  unregister: (id: string) => void
}

const emptyRegistry: ActionRegistry = {
  register: () => undefined,
  unregister: () => undefined,
}

const ActionRegistryContext = createContext<ActionRegistry>(emptyRegistry)
const RegisteredActionsContext = createContext<readonly AppAction[]>([])
const QueueActionContext = createContext<(id: string) => void>(() => undefined)

/** How long a queued action waits for its page to register it. */
const QUEUE_WAIT_MS = 10_000

/**
 * Runs an action once a page registers it, enabled: how a verb opens its page from anywhere
 * and then does its work. A newer request replaces an older one; one never registered lapses.
 */
export const useQueueAction = () => useContext(QueueActionContext)

export function ActionProvider({ children }: { children: ReactNode }) {
  const [registrations, setRegistrations] = useState<ReadonlyMap<string, readonly AppAction[]>>(
    () => new Map(),
  )
  const register = useCallback(({ id, actions }: Registration) => {
    setRegistrations((current) => {
      if (current.get(id) === actions) return current
      const next = new Map(current)
      next.set(id, actions)
      return next
    })
  }, [])
  const unregister = useCallback((id: string) => {
    setRegistrations((current) => {
      if (!current.has(id)) return current
      const next = new Map(current)
      next.delete(id)
      return next
    })
  }, [])
  const actions = useMemo(() => [...registrations.values()].flat(), [registrations])
  const value = useMemo(() => ({ register, unregister }), [register, unregister])
  const [queued, setQueued] = useState<string | null>(null)
  useEffect(() => {
    if (!queued) return undefined
    const action = actions.find(({ id, disabled }) => id === queued && !disabled)
    if (action) {
      setQueued(null)
      void Promise.resolve(action.run()).catch(() => undefined)
      return undefined
    }
    const timer = setTimeout(() => setQueued(null), QUEUE_WAIT_MS)
    return () => clearTimeout(timer)
  }, [actions, queued])
  return (
    <ActionRegistryContext.Provider value={value}>
      <RegisteredActionsContext.Provider value={actions}>
        <QueueActionContext.Provider value={setQueued}>{children}</QueueActionContext.Provider>
      </RegisteredActionsContext.Provider>
    </ActionRegistryContext.Provider>
  )
}

/**
 * Tests only: receives every action a render registers. Rendering to a string runs no
 * effects, so this is how a route test reads the registry its page would fill.
 */
export const ActionCollectorContext = createContext<((action: AppAction) => void) | null>(null)

export function useRegisterActions(actions: readonly AppAction[]) {
  const id = useId()
  const collect = useContext(ActionCollectorContext)
  if (collect) for (const action of actions) collect(action)
  const { register, unregister } = useContext(ActionRegistryContext)
  useEffect(() => {
    register({ id, actions })
    return () => unregister(id)
  }, [actions, id, register, unregister])
}

export function useRegisteredActions() {
  return useContext(RegisteredActionsContext)
}

export const actionScopePriority: Record<ActionScope, number> = {
  selection: 5,
  region: 4,
  dialog: 3,
  page: 2,
  app: 1,
}

/**
 * Scopes whose single keys act at once, even one that starts a shortcut elsewhere: C is a
 * category for the selection or in the inspector, and starts a cash account's jump otherwise.
 */
const immediateScopes: ReadonlySet<ActionScope> = new Set(['selection', 'dialog'])

export function normalizeActionToken(event: KeyboardEvent) {
  const modifiers = [
    event.metaKey ? 'Meta' : '',
    event.ctrlKey ? 'Control' : '',
    event.altKey ? 'Alt' : '',
  ].filter(Boolean)
  let key = event.key
  if (key === ' ') key = 'Space'
  else if (key.length === 1 && !event.shiftKey) key = key.toLowerCase()
  else if (key.length === 1 && event.shiftKey && key !== '?') key = key.toUpperCase()
  return [...modifiers, key].join('+')
}

export function bindingStartsWith(binding: ActionBinding, prefix: readonly string[]) {
  return prefix.every((token, index) => binding.sequence[index] === token)
}

export function actionBindings(actions: readonly AppAction[]) {
  return actions.flatMap((action) =>
    (action.bindings ?? []).map((binding) => ({ action, binding })),
  )
}

export function resolveBinding(actions: readonly AppAction[], sequence: readonly string[]) {
  const candidates = actionBindings(actions)
    .filter(({ binding }) => bindingStartsWith(binding, sequence))
    .toSorted(
      (left, right) =>
        actionScopePriority[right.action.scope] - actionScopePriority[left.action.scope],
    )
  const exact = candidates.find(({ binding }) => binding.sequence.length === sequence.length)
  const wins = exact && immediateScopes.has(exact.action.scope) ? exact.action.scope : undefined
  const pending = candidates.some(
    ({ action, binding }) =>
      binding.sequence.length > sequence.length && (!wins || action.scope === wins),
  )
  return { candidates, exact, pending }
}

/**
 * Bindings that can't both work. A shortcut that is also the start of a longer one would wait
 * for a key that may never come; a selection's or dialog's key is the exception, since it wins
 * while rows are selected or the dialog is open. Two enabled actions on one key in one scope
 * leave which runs to chance.
 */
export function bindingConflicts(actions: readonly AppAction[]) {
  const bindings = actionBindings(actions)
  const sameKey = (left: ActionBinding, right: ActionBinding) =>
    left.sequence.length === right.sequence.length && bindingStartsWith(left, right.sequence)
  return bindings.flatMap((short, index) => [
    ...(immediateScopes.has(short.action.scope)
      ? []
      : bindings
          .filter(
            (long) =>
              long.binding.sequence.length > short.binding.sequence.length &&
              bindingStartsWith(long.binding, short.binding.sequence),
          )
          .map(
            (long) =>
              `${short.binding.label} (${short.action.id}) starts ${long.binding.label} (${long.action.id})`,
          )),
    ...bindings
      .slice(index + 1)
      .filter(
        (other) =>
          other.action.id !== short.action.id &&
          other.action.scope === short.action.scope &&
          other.action.dialog === short.action.dialog &&
          !short.action.disabled &&
          !other.action.disabled &&
          sameKey(short.binding, other.binding),
      )
      .map((other) => `${short.binding.label} runs both ${short.action.id} and ${other.action.id}`),
  ])
}

export function longestCommonPrefix(values: readonly string[]) {
  const [first, ...rest] = values
  if (first === undefined) return ''
  let prefix = first
  for (const value of rest) {
    let index = 0
    const limit = Math.min(prefix.length, value.length)
    while (
      index < limit &&
      prefix.charAt(index).toLowerCase() === value.charAt(index).toLowerCase()
    )
      index += 1
    prefix = prefix.slice(0, index)
    if (!prefix) break
  }
  return prefix
}

export function paletteCompletion(query: string, candidates: readonly string[]) {
  const leading = query.match(/^\s*/)?.[0] ?? ''
  const typed = query.slice(leading.length)
  if (!typed) return undefined
  const matches = [...new Set(candidates)]
    .filter((candidate) => candidate.toLowerCase().startsWith(typed.toLowerCase()))
    .toSorted((left, right) => left.length - right.length || left.localeCompare(right))
  const [only, ...others] = matches
  if (only === undefined) return undefined
  const target = others.length ? longestCommonPrefix(matches) : only
  if (target.length <= typed.length) return undefined
  const nextBoundary = target.slice(typed.length).search(/\s+/)
  const stepEnd = nextBoundary < 0 ? target.length : typed.length + nextBoundary + 1
  return {
    full: `${leading}${target}`,
    step: `${leading}${target.slice(0, stepEnd)}`,
  }
}

const targetSelector = [
  'a[href]',
  'button:not(:disabled)',
  'input:not(:disabled)',
  'select:not(:disabled)',
  'textarea:not(:disabled)',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

const hintAlphabet = 'asdfghjklqwertyuiopzxcvbnm'

export function hintCode(index: number, count: number) {
  if (count <= hintAlphabet.length) return hintAlphabet.charAt(index)
  const width = count <= hintAlphabet.length ** 2 ? 2 : 3
  let value = index
  let code = ''
  for (let position = 0; position < width; position += 1) {
    code = hintAlphabet.charAt(value % hintAlphabet.length) + code
    value = Math.floor(value / hintAlphabet.length)
  }
  return code
}

export function visibleActionTargets(root: ParentNode = document) {
  const elements = [...root.querySelectorAll<HTMLElement>(targetSelector)].filter((element) => {
    if (element.closest('[aria-hidden="true"], [inert], .focus-hints, .command-dialog'))
      return false
    const style = window.getComputedStyle(element)
    return (
      style.visibility !== 'hidden' &&
      style.display !== 'none' &&
      element.getClientRects().length > 0
    )
  })
  return elements.flatMap((element, index) => {
    const label =
      element.getAttribute('aria-label')?.trim() ||
      element.getAttribute('title')?.trim() ||
      element.textContent?.replace(/\s+/g, ' ').trim()
    return label ? [{ element, label, code: hintCode(index, elements.length) }] : []
  })
}

type ActionUsage = Record<string, { count: number; lastUsed: number }>

const isUsage = (value: unknown): value is ActionUsage[string] =>
  value !== null &&
  typeof value === 'object' &&
  'count' in value &&
  typeof value.count === 'number' &&
  'lastUsed' in value &&
  typeof value.lastUsed === 'number'

/** How often and how recently each action ran, as an optional local ranking hint. */
const actionUsage = preference<ActionUsage>(
  'brief:action-usage',
  (raw) => {
    try {
      const parsed: unknown = JSON.parse(raw ?? '{}')
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {}
      return Object.fromEntries(
        Object.entries(parsed).filter((entry): entry is [string, ActionUsage[string]] =>
          isUsage(entry[1]),
        ),
      )
    } catch {
      return {}
    }
  },
  JSON.stringify,
)

export const getActionUsage = actionUsage.get

export function recordActionUsage(id: string) {
  const usage = actionUsage.get()
  actionUsage.save({
    ...usage,
    [id]: { count: (usage[id]?.count ?? 0) + 1, lastUsed: Date.now() },
  })
}

export function forgetActionUsage(id: string) {
  const { [id]: _forgotten, ...usage } = actionUsage.get()
  actionUsage.save(usage)
}
