import { useEffect, useState } from 'react'

import { useLatest } from '../hooks/use-latest'
import { visibleActionTargets, type AppAction } from '../lib/action-system'
import { shortcutOverlayOpen } from '../lib/keyboard'

/** How long ⌘ is held alone before its shortcuts show, so ⌘K and ⌘C never flash them. */
export const COMMAND_HINT_DELAY_MS = 300

type CommandHint = {
  id: number
  label: string
  top: number
  left: number
  /** Where the hint sits from `left`: within the control's end, or outside it on either side. */
  side: 'inside' | 'after' | 'before'
}

/** Room a hint needs past a control's right edge before it goes on the left instead. */
const HINT_ROOM = 44

/**
 * The ⌘ shortcut of each visible control that has one: the control names its command in
 * `data-command`, and the command's first single ⌘ chord is its shortcut. A sidebar row has
 * room at its end, so its hint sits inside; any other control keeps its icon and label clear,
 * with the hint beside it, on the right unless the window ends there.
 */
export function commandKeyHints(actions: readonly AppAction[]): CommandHint[] {
  const byId = new Map(actions.map((action) => [action.id, action]))
  return visibleActionTargets().flatMap(({ element }, id) => {
    const command = element.dataset.command
    const binding = command
      ? byId
          .get(command)
          ?.bindings?.find(
            ({ sequence }) => sequence.length === 1 && sequence[0]?.startsWith('Meta+'),
          )
      : undefined
    if (!binding) return []
    const rect = element.getBoundingClientRect()
    const side: CommandHint['side'] = element.classList.contains('sidebar-menu-button')
      ? 'inside'
      : rect.right + HINT_ROOM <= window.innerWidth
        ? 'after'
        : 'before'
    return [
      {
        id,
        label: binding.label,
        top: rect.top + rect.height / 2,
        left: side === 'before' ? rect.left : Math.min(rect.right, window.innerWidth),
        side,
      },
    ]
  })
}

/**
 * Holding ⌘ alone for a moment shows every visible control's ⌘ shortcut: ⌘1 to ⌘9 beside the
 * sidebar's pages, and the rest wherever their controls are. They float over the page, so
 * nothing moves, and screen readers skip them: the controls carry `aria-keyshortcuts`. Letting
 * go of ⌘, pressing any other key, clicking, scrolling, or leaving the window hides them at
 * once; nothing shows while a dialog, the palette, or another overlay is open (`paused`).
 */
export function CommandKeyHints({
  actions,
  paused,
}: {
  actions: readonly AppAction[]
  paused: boolean
}) {
  const [hints, setHints] = useState<CommandHint[]>()
  const latest = useLatest({ actions, paused })
  // Registered once, ahead of the shell's key dispatch, which stops the keys it handles.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const hide = () => {
      clearTimeout(timer)
      timer = undefined
      setHints(undefined)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Meta' && event.repeat) return
      hide()
      if (event.key !== 'Meta' || event.ctrlKey || event.altKey || event.shiftKey) return
      timer = setTimeout(() => {
        timer = undefined
        if (latest.current.paused || shortcutOverlayOpen()) return
        setHints(commandKeyHints(latest.current.actions))
      }, COMMAND_HINT_DELAY_MS)
    }
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === 'Meta' || !event.metaKey) hide()
    }
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('keyup', onKeyUp, true)
    window.addEventListener('pointerdown', hide, true)
    window.addEventListener('scroll', hide, true)
    window.addEventListener('resize', hide)
    window.addEventListener('blur', hide)
    // ⌘Tab leaves without a keyup.
    document.addEventListener('visibilitychange', hide)
    return () => {
      clearTimeout(timer)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('keyup', onKeyUp, true)
      window.removeEventListener('pointerdown', hide, true)
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
      window.removeEventListener('blur', hide)
      document.removeEventListener('visibilitychange', hide)
    }
  }, [latest])

  if (!hints?.length || paused) return null
  return (
    <div className="command-key-hints" aria-hidden="true">
      {hints.map(({ id, label, top, left, side }) => (
        <kbd className="command-key-hint" data-side={side} key={id} style={{ top, left }}>
          {label}
        </kbd>
      ))}
    </div>
  )
}
