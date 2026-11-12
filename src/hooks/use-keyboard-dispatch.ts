import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  normalizeActionToken,
  recordActionUsage,
  resolveBinding,
  type AppAction,
} from '../lib/action-system'
import { openModal, shortcutInput, shortcutOverlayOpen } from '../lib/keyboard'
import { contextualListActions } from '../lib/keyboard-regions'

/** Controls that act on Enter themselves, so a list's "open" leaves the key to them. */
const activatingControl =
  'button, a[href], summary, [role="button"], [role="switch"], [role="checkbox"], [role="link"]'

/**
 * A control in an open dialog that Space presses or types into, which keeps the key rather
 * than running the dialog's own Space. Focus left on the page behind it doesn't count.
 */
export function spaceActivates(target: EventTarget | null, dialog: Element | null | undefined) {
  return (
    target instanceof Element &&
    Boolean(dialog?.contains(target)) &&
    Boolean(
      target.closest(
        'button, a[href], input, textarea, select, summary, [contenteditable]:not([contenteditable="false"]), [role="button"], [role="link"], [role="checkbox"], [role="switch"], [role="menuitem"], [role="option"], [role="tab"]',
      ),
    )
  )
}

/**
 * The actions a key press may run. While a dialog, menu, or popover is open the page behind it
 * is out of reach: app actions (palette, undo, hide values, navigation) still run, list motions
 * only for a list inside the open dialog, and dialog actions only for the dialog that holds the
 * keyboard. Enter on a button inside a row presses the button rather than opening the row, and
 * Enter or Space on a control inside the dialog presses the control.
 */
export function actionsFor(
  actions: readonly AppAction[],
  focusRegion: HTMLElement | undefined,
  event?: Pick<KeyboardEvent, 'key' | 'target'>,
): readonly AppAction[] {
  const target = event?.target instanceof Element ? event.target : null
  const onControl = Boolean(
    target?.closest(activatingControl) && !target.matches('[data-keyboard-row]'),
  )
  const pressesControl = event?.key === 'Enter' && onControl
  const overlay = shortcutOverlayOpen()
  const modal = overlay ? openModal() : undefined
  const regionReachable = !overlay || Boolean(modal && focusRegion && modal.contains(focusRegion))
  const pressesDialogControl =
    event?.key === ' '
      ? spaceActivates(target, modal)
      : event?.key === 'Enter' && onControl && Boolean(modal?.contains(target))
  if (!overlay && !pressesControl) return actions.filter(({ scope }) => scope !== 'dialog')
  return actions.filter((action) =>
    action.scope === 'app'
      ? true
      : action.scope === 'region'
        ? regionReachable && !pressesControl
        : action.scope === 'dialog'
          ? Boolean(modal) && action.dialog === modal && !pressesDialogControl
          : !overlay,
  )
}

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
      if (event.defaultPrevented || event.isComposing || paused) return
      const available = actionsFor(actions, focusRegion, event)
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
      const resolution = resolveBinding(available, sequence)
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
  }, [actions, closeKeyState, executeAction, focusRegion, keyGuideOpen, keySequence, paused])

  const openKeyGuide = useCallback(() => setKeyGuideOpen(true), [])
  // The guide offers what a key would run now; it re-reads when it opens over a dialog.
  const guideActions = useMemo(
    () => (keyGuideOpen ? actionsFor(actions, focusRegion) : actions),
    [actions, focusRegion, keyGuideOpen],
  )
  return {
    actions,
    guideActions,
    keySequence,
    keyGuideOpen,
    openKeyGuide,
    closeKeyState,
    executeAction,
  }
}
