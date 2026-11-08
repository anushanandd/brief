import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  normalizeActionToken,
  recordActionUsage,
  resolveBinding,
  type AppAction,
} from '../lib/action-system'
import { shortcutInput, shortcutOverlayOpen } from '../lib/keyboard'
import { contextualListActions } from '../lib/keyboard-regions'

/**
 * Runs actions from the keyboard: single keys and sequences, with the key guide open while one
 * is pending. The actions on offer are `baseActions` plus the focused list's motions. `paused`
 * stops listening while another surface, such as the palette, owns the keyboard.
 */
export function useKeyboardDispatch(
  baseActions: readonly AppAction[],
  focusRegion: HTMLElement | undefined,
  paused: boolean,
) {
  const [keySequence, setKeySequence] = useState<string[]>([])
  const [keyGuideOpen, setKeyGuideOpen] = useState(false)

  const actions = useMemo(() => {
    const byId = new Map<string, AppAction>()
    for (const action of baseActions) byId.set(action.id, action)
    for (const action of contextualListActions(focusRegion)) byId.set(action.id, action)
    return [...byId.values()]
    // The guide opening re-reads the focused list, whose rows may have changed.
  }, [baseActions, focusRegion, keyGuideOpen])

  const closeKeyState = useCallback(() => {
    setKeyGuideOpen(false)
    setKeySequence([])
  }, [])

  const executeAction = useCallback(
    (action: AppAction) => {
      if (action.disabled) return
      closeKeyState()
      recordActionUsage(action.id)
      void Promise.resolve(action.run()).catch(() => undefined)
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

      if (event.key === 'Escape' && (keyGuideOpen || keySequence.length)) {
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

      const sequence = [...keySequence, token]
      const resolution = resolveBinding(actions, sequence)
      if (!resolution.exact && !resolution.pending) {
        if (keySequence.length) {
          event.preventDefault()
          event.stopImmediatePropagation()
          closeKeyState()
        }
        return
      }
      event.preventDefault()
      event.stopImmediatePropagation()
      if (resolution.exact && !resolution.pending) {
        executeAction(resolution.exact.action)
        return
      }
      setKeySequence(sequence)
      setKeyGuideOpen(true)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [actions, closeKeyState, executeAction, keyGuideOpen, keySequence, paused])

  const openKeyGuide = useCallback(() => setKeyGuideOpen(true), [])
  return {
    actions,
    keySequence,
    keyGuideOpen,
    openKeyGuide,
    closeKeyState,
    executeAction,
  }
}
