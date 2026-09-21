import { listen } from '@tauri-apps/api/event'
import { useEffect, type Dispatch, type SetStateAction } from 'react'

import { isTauri } from '../lib/api'
import { adjacentGraphWindow, graphWindowForKey } from '../lib/graph-preferences'
import { pageShortcutBlocked } from '../lib/keyboard'

const shortcutKeys: Record<string, string> = {
  'graph-week': 'W',
  'graph-month': 'M',
  'graph-quarter': 'Q',
  'graph-all': 'A',
}

type ArrowShortcutEvent = Pick<KeyboardEvent, 'altKey' | 'ctrlKey' | 'key' | 'metaKey' | 'shiftKey'>

export function graphAccountShortcut(
  event: ArrowShortcutEvent,
  modifier: 'command' | 'none' = 'command',
) {
  if (
    event.metaKey !== (modifier === 'command') ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey ||
    (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')
  ) {
    return undefined
  }
  return event.key === 'ArrowLeft' ? 'graph-previous' : 'graph-next'
}

export function graphRangeShortcut(event: ArrowShortcutEvent) {
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return undefined
  return graphWindowForKey(event.key)
}

export function graphWindowShortcut(event: ArrowShortcutEvent) {
  if (
    !event.metaKey ||
    !event.ctrlKey ||
    event.shiftKey ||
    event.altKey ||
    (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')
  ) {
    return undefined
  }
  return event.key === 'ArrowLeft' ? 'graph-window-previous' : 'graph-window-next'
}

export function useGraphWindowShortcuts(setGraphWindow: Dispatch<SetStateAction<number>>) {
  useEffect(() => {
    const runShortcut = (shortcut: string) => {
      if (shortcut === 'graph-window-previous' || shortcut === 'graph-window-next') {
        setGraphWindow((current) =>
          adjacentGraphWindow(current, shortcut === 'graph-window-previous' ? -1 : 1),
        )
        return
      }
      const next = graphWindowForKey(shortcutKeys[shortcut] ?? '')
      if (next != null) setGraphWindow(next)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const shortcut = graphWindowShortcut(event)
      const range = graphRangeShortcut(event)
      if ((!shortcut && range == null) || pageShortcutBlocked(event)) {
        return
      }
      event.preventDefault()
      if (shortcut) runShortcut(shortcut)
      else if (range != null) setGraphWindow(range)
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen<string>('graph-shortcut', (event) => {
        if (!pageShortcutBlocked()) runShortcut(event.payload)
      }).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [setGraphWindow])
}

export function useGraphAccountShortcuts(
  onDirection: (direction: -1 | 1) => void,
  modifier: 'command' | 'none' = 'command',
) {
  useEffect(() => {
    const runShortcut = (shortcut: string) => {
      if (shortcut === 'graph-previous') onDirection(-1)
      else if (shortcut === 'graph-next') onDirection(1)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const shortcut = graphAccountShortcut(event, modifier)
      if (!shortcut || pageShortcutBlocked(event)) return
      event.preventDefault()
      runShortcut(shortcut)
    }
    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen<string>('graph-shortcut', (event) => {
        if (!pageShortcutBlocked()) runShortcut(event.payload)
      }).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [onDirection, modifier])
}
