import { listen } from '@tauri-apps/api/event'
import { useEffect, type Dispatch, type SetStateAction } from 'react'

import { isTauri } from '../lib/api'
import { adjacentGraphWindow, graphWindows } from '../lib/graph-preferences'

const shortcutLabels: Record<string, string> = {
  'graph-week': '1W',
  'graph-month': '1M',
  'graph-quarter': '3M',
  'graph-all': 'All',
}

type ArrowShortcutEvent = Pick<KeyboardEvent, 'altKey' | 'ctrlKey' | 'key' | 'metaKey' | 'shiftKey'>

export function graphAccountShortcut(event: ArrowShortcutEvent) {
  if (
    !event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey ||
    (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')
  ) {
    return undefined
  }
  return event.key === 'ArrowLeft' ? 'graph-previous' : 'graph-next'
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
      const next = graphWindows.find(({ label }) => label === shortcutLabels[shortcut])
      if (next) setGraphWindow(next.secs)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      const shortcut = graphWindowShortcut(event)
      if (
        !shortcut ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }
      event.preventDefault()
      runShortcut(shortcut)
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen<string>('graph-shortcut', (event) => runShortcut(event.payload)).then((stop) => {
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
