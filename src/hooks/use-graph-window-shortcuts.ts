import { useMemo, type Dispatch, type SetStateAction } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import { adjacentGraphWindow, graphWindows } from '../lib/graph-preferences'

// Ids name the range, so saved usage and pinned commands keep working.
const rangeIds = ['graph-week', 'graph-month', 'graph-quarter', 'graph-year', 'graph-all']

/** W, M, Q, Y, and A choose a chart's range; ⌘⌃← and ⌘⌃→ step through them. */
export function useGraphWindowShortcuts(setGraphWindow: Dispatch<SetStateAction<number>>) {
  const actions = useMemo<AppAction[]>(() => {
    const step = (direction: -1 | 1) => () =>
      setGraphWindow((current) => adjacentGraphWindow(current, direction))
    return [
      ...graphWindows.map(({ label, settingsLabel, secs }, index) => ({
        id: `graph.range.${rangeIds[index]}`,
        title: `Show ${settingsLabel}`,
        search: `chart graph range ${settingsLabel}`,
        group: 'View',
        scope: 'page' as const,
        bindings: [{ sequence: [label.toLowerCase()], label }],
        repeatable: true,
        run: () => setGraphWindow(secs),
      })),
      {
        id: 'graph.window.previous',
        title: 'Previous date view',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['Meta+Control+ArrowLeft'], label: '⌘⌃←' }],
        repeatable: true,
        run: step(-1),
      },
      {
        id: 'graph.window.next',
        title: 'Next date view',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['Meta+Control+ArrowRight'], label: '⌘⌃→' }],
        repeatable: true,
        run: step(1),
      },
    ]
  }, [setGraphWindow])
  useRegisterActions(actions)
}

/** ⌘← and ⌘→ step through a page's selector; bare arrows are left to time on the page. */
export function useGraphAccountShortcuts(onDirection: (direction: -1 | 1) => void) {
  const actions = useMemo<AppAction[]>(
    () => [
      {
        id: 'graph.selection.previous',
        title: 'Previous in selector',
        search: 'previous account holding report chart selector strip',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['Meta+ArrowLeft'], label: '⌘←' }],
        repeatable: true,
        run: () => onDirection(-1),
      },
      {
        id: 'graph.selection.next',
        title: 'Next in selector',
        search: 'next account holding report chart selector strip',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['Meta+ArrowRight'], label: '⌘→' }],
        repeatable: true,
        run: () => onDirection(1),
      },
    ],
    [onDirection],
  )
  useRegisterActions(actions)
}
