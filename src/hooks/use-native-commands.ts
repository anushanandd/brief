import { useLocation, useRouter } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Menu } from '@tauri-apps/api/menu'
import { useEffect } from 'react'

import type { AppAction } from '../lib/action-system'
import { isTauri } from '../lib/api'
import {
  commandMenuSections,
  copyText,
  openedViewHref,
  rowAmountText,
  rowCopyText,
  setCommandMenu,
  viewLink,
} from '../lib/native-commands'
import { useLatest } from './use-latest'

const copy = (text: string) => () => void copyText(text).catch(() => undefined)

const editable = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  Boolean(target.closest('input, textarea, select, [contenteditable="true"]'))

/**
 * Brief as a Mac app: every app and page command in the menu bar (and the menu's own Settings
 * and value-hiding items), `brief://` links that open a view, ⌘C on a focused row, and a
 * context menu for rows and the view itself.
 */
export function useNativeCommands(
  actions: readonly AppAction[],
  executeAction: (action: AppAction) => void,
) {
  const router = useRouter()
  const location = useLocation()
  const latest = useLatest({ actions, executeAction, location })

  // Pages register and drop commands as they mount, so the menu follows after they settle.
  useEffect(() => {
    if (!isTauri()) return undefined
    const timer = setTimeout(
      () => void setCommandMenu(commandMenuSections(actions)).catch(() => undefined),
      150,
    )
    return () => clearTimeout(timer)
  }, [actions])

  useEffect(() => {
    if (!isTauri()) return undefined
    const routes = Object.keys(router.routesByPath)
    const open = (view: string) => {
      const href = openedViewHref(view, routes)
      if (href) void router.navigate({ href })
    }
    let disposed = false
    const stops: Array<() => void> = []
    const keep = (stop: () => void) => (disposed ? stop() : stops.push(stop))
    const run = (id: string) => {
      const action = latest.current.actions.find((candidate) => candidate.id === id)
      if (action) latest.current.executeAction(action)
    }
    void listen<string>('run-action', ({ payload }) => run(payload)).then(keep)
    // The menu bar's own items: Settings… and Hide or Show Values.
    void listen('open-settings', () => run('settings.open')).then(keep)
    void listen('toggle-sensitive-values', () => run('privacy.toggle-values')).then(keep)
    void listen<string>('open-view', ({ payload }) => open(payload)).then(keep)
    void invoke<string | null>('take_opened_view')
      .then((view) => view && open(view))
      .catch(() => undefined)
    return () => {
      disposed = true
      for (const stop of stops) stop()
    }
  }, [router])

  useEffect(() => {
    // With nothing selected, ⌘C (the Edit menu's Copy) copies the focused row as it reads.
    const onCopy = (event: ClipboardEvent) => {
      if (editable(event.target) || window.getSelection()?.toString().trim()) return
      const row = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
      if (!row || !event.clipboardData) return
      event.clipboardData.setData('text/plain', rowCopyText(row))
      event.preventDefault()
    }
    document.addEventListener('copy', onCopy)
    return () => document.removeEventListener('copy', onCopy)
  }, [])

  useEffect(() => {
    if (!isTauri()) return undefined
    const onContextMenu = (event: MouseEvent) => {
      // Fields keep the system menu: cut, paste, spelling.
      if (editable(event.target)) return
      event.preventDefault()
      const target = event.target instanceof HTMLElement ? event.target : null
      const selection = window.getSelection()?.toString().trim()
      const row = target?.closest<HTMLElement>('[data-keyboard-row]')
      row?.focus({ preventScroll: true })
      const opener = row?.matches('[data-keyboard-open]')
        ? row
        : row?.querySelector<HTMLElement>('[data-keyboard-open]')
      const amount = row ? rowAmountText(row) : undefined
      const palette = latest.current.actions.find(({ id }) => id === 'palette.open')
      const items = [
        ...(selection ? [{ text: 'Copy', action: copy(selection) }] : []),
        ...(opener ? [{ text: 'Open', action: () => opener.click() }] : []),
        ...(row ? [{ text: 'Copy Row', action: copy(rowCopyText(row)) }] : []),
        ...(amount ? [{ text: 'Copy Amount', action: copy(amount) }] : []),
        ...(selection || row ? [{ item: 'Separator' as const }] : []),
        { text: 'Copy Link to This View', action: copy(viewLink(latest.current.location)) },
        ...(palette
          ? [{ text: 'Command Palette…', action: () => latest.current.executeAction(palette) }]
          : []),
      ]
      void Menu.new({ items })
        .then((menu) => menu.popup())
        .catch(() => undefined)
    }
    document.addEventListener('contextmenu', onContextMenu)
    return () => document.removeEventListener('contextmenu', onContextMenu)
  }, [])
}
