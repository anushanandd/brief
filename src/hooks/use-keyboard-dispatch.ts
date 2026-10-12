import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  normalizeActionToken,
  recordActionUsage,
  resolveBinding,
  type AppAction,
} from '../lib/action-system'
import { shortcutInput, shortcutOverlayOpen } from '../lib/keyboard'
import { contextualListActions, currentKeyboardRegion } from '../lib/keyboard-regions'

/**
 * Runs actions from the keyboard: single keys and sequences (with the key guide open while one
 * is pending), a count prefix for motions, and `.` to repeat the last repeatable action. The
 * actions on offer are `baseActions` plus the focused list's motions. `paused` stops listening
 * while another surface, such as the palette, owns the keyboard.
 */
export function useKeyboardDispatch(
  baseActions: readonly AppAction[],
  focusRegion: HTMLElement | undefined,
  paused: boolean,
) {
  const [keySequence, setKeySequence] = useState<string[]>([])
  const [keyCount, setKeyCount] = useState('')
  const [keyGuideOpen, setKeyGuideOpen] = useState(false)
  const [repeatedAction, setRepeatedAction] = useState<AppAction>()

  const actions = useMemo(() => {
    const byId = new Map<string, AppAction>()
    for (const action of baseActions) byId.set(action.id, action)
    for (const action of contextualListActions(focusRegion)) byId.set(action.id, action)
    if (repeatedAction) {
      const repeated = byId.get(repeatedAction.id)
      if (repeated?.repeatable)
        byId.set('action.repeat', {
          id: 'action.repeat',
          title: `Repeat ${repeated.title}`,
          search: 'repeat last action dot',
          group: 'Here',
          icon: repeated.icon,
          scope: repeated.scope,
          bindings: [{ sequence: ['.'], label: '.' }],
          run: (count) => repeated.run(count),
        })
    }
    return [...byId.values()]
    // The guide opening re-reads the focused list, whose rows may have changed.
  }, [baseActions, focusRegion, keyGuideOpen, repeatedAction])

  const closeKeyState = useCallback(() => {
    setKeyGuideOpen(false)
    setKeySequence([])
    setKeyCount('')
  }, [])

  const executeAction = useCallback(
    (action: AppAction, count?: number) => {
      if (action.disabled) return
      closeKeyState()
      recordActionUsage(action.id)
      if (action.repeatable) setRepeatedAction(action)
      void Promise.resolve(action.run(count)).catch(() => undefined)
    },
    [closeKeyState],
  )

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || paused || shortcutOverlayOpen()) return
      const isEditing = shortcutInput(event.target)
      const token = normalizeActionToken(event)
      const modified = event.metaKey || event.ctrlKey || event.altKey
      // Command arrows move the insertion point in a text field, so they stay with the field.
      if (isEditing && (!modified || event.key.startsWith('Arrow'))) return

      if (event.key === 'Escape' && (keyGuideOpen || keySequence.length || keyCount)) {
        event.preventDefault()
        event.stopImmediatePropagation()
        closeKeyState()
        return
      }
      if (event.key === 'Backspace' && keySequence.length) {
        event.preventDefault()
        event.stopImmediatePropagation()
        setKeySequence((current) => current.slice(0, -1))
        setKeyGuideOpen(true)
        return
      }

      const region = typeof document !== 'undefined' ? currentKeyboardRegion() : undefined
      if (
        !modified &&
        !keySequence.length &&
        region &&
        /^\d$/.test(event.key) &&
        !(event.key === '0' && !keyCount)
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        setKeyCount((current) => `${current}${event.key}`.slice(0, 3))
        setKeyGuideOpen(true)
        return
      }

      const sequence = [...keySequence, token]
      const resolution = resolveBinding(actions, sequence)
      if (!resolution.exact && !resolution.pending) {
        if (keySequence.length || keyCount) {
          event.preventDefault()
          event.stopImmediatePropagation()
          closeKeyState()
        }
        return
      }
      event.preventDefault()
      event.stopImmediatePropagation()
      if (resolution.exact && !resolution.pending) {
        const count = keyCount ? Number(keyCount) : undefined
        if (!count || resolution.exact.binding.acceptsCount) {
          executeAction(resolution.exact.action, count)
        } else closeKeyState()
        return
      }
      setKeySequence(sequence)
      setKeyGuideOpen(true)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [actions, closeKeyState, executeAction, keyCount, keyGuideOpen, keySequence, paused])

  const openKeyGuide = useCallback(() => setKeyGuideOpen(true), [])
  return {
    actions,
    keySequence,
    keyCount,
    keyGuideOpen,
    openKeyGuide,
    closeKeyState,
    executeAction,
  }
}
