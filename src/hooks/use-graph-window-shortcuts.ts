import { useMemo, type Dispatch, type SetStateAction } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  adjacentGraphWindow,
  graphWindows,
  scrollPeriodDays,
  sessionGraphEnd,
  setGraphEnd,
  shiftedGraphEnd,
  useGraphEnd,
  useGraphWindow,
  type GraphScroll,
} from '../lib/graph-preferences'
import { useLatest } from './use-latest'

// Ids name the range, so saved usage and pinned commands keep working.
const rangeIds = [
  'graph-day',
  'graph-week',
  'graph-month',
  'graph-quarter',
  'graph-year',
  'graph-all',
]

/**
 * D, W, M, Q, Y, and A choose a chart's range, and ⌘⌃← and ⌘⌃→ step through them. With
 * `scroll`, [ and ] move the page a period back or ahead in time, and } brings it to today.
 */
export function graphRangeActions(
  setGraphWindow: Dispatch<SetStateAction<number>>,
  window: number,
  scrolled: boolean,
  scroll?: GraphScroll,
): AppAction[] {
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
      run: () => setGraphWindow(secs),
    })),
    {
      id: 'graph.window.previous',
      title: 'Previous date view',
      group: 'View',
      scope: 'page',
      bindings: [{ sequence: ['Meta+Control+ArrowLeft'], label: '⌘⌃←' }],
      run: step(-1),
    },
    {
      id: 'graph.window.next',
      title: 'Next date view',
      group: 'View',
      scope: 'page',
      bindings: [{ sequence: ['Meta+Control+ArrowRight'], label: '⌘⌃→' }],
      run: step(1),
    },
    ...graphScrollActions(window, scrolled, scroll),
  ]
}

/**
 * [ and ] move a chart a period back or ahead in time, and } brings it to today. They read the
 * end day when run, so the list changes only when the page goes from today to scrolled back.
 */
export function graphScrollActions(
  window: number,
  scrolled: boolean,
  scroll: GraphScroll | undefined,
): AppAction[] {
  const days = scrollPeriodDays(window)
  const move = (direction: -1 | 1) => () =>
    scroll &&
    days &&
    setGraphEnd(
      shiftedGraphEnd(sessionGraphEnd(), direction * days, window, scroll.today, scroll.first),
    )
  const unscrollable = days ? undefined : 'A day or all time ends today'
  return scroll
    ? ([
        {
          id: 'graph.end.earlier',
          title: 'Earlier period',
          search: 'chart graph scroll back in time earlier past',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['['], label: '[' }],
          disabled: Boolean(unscrollable),
          disabledReason: unscrollable,
          run: move(-1),
        },
        {
          id: 'graph.end.later',
          title: 'Later period',
          search: 'chart graph scroll ahead in time later',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: [']'], label: ']' }],
          disabled: !scrolled,
          disabledReason: scrolled ? undefined : 'Already showing today',
          run: move(1),
        },
        {
          id: 'graph.end.today',
          title: 'Back to today',
          search: 'chart graph scroll today now latest',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['}'], label: '}' }],
          disabled: !scrolled,
          disabledReason: scrolled ? undefined : 'Already showing today',
          run: () => setGraphEnd(undefined),
        },
      ] satisfies AppAction[])
    : []
}

/**
 * The same bounds while the days are the same: pages rebuild theirs from live chart data, and
 * the actions built from them would otherwise change, and the menu bar with them, every tick.
 */
export function useScrollBounds(scroll: GraphScroll | undefined) {
  const { today, first } = scroll ?? {}
  return useMemo(() => (today ? { today, first } : undefined), [today, first])
}

/** The page's range keys; see `graphRangeActions`. */
export function useGraphWindowShortcuts(
  setGraphWindow: Dispatch<SetStateAction<number>>,
  scroll?: GraphScroll,
) {
  const [window] = useGraphWindow()
  const scrolled = Boolean(useGraphEnd()[0])
  const bounds = useScrollBounds(scroll)
  const actions = useMemo(
    () => graphRangeActions(setGraphWindow, window, scrolled, bounds),
    [setGraphWindow, window, scrolled, bounds],
  )
  useRegisterActions(actions)
}

/** The scroll keys alone, for a chart whose page registers its range keys elsewhere. */
export function useGraphScrollShortcuts(scroll: GraphScroll | undefined) {
  const [window] = useGraphWindow()
  const scrolled = Boolean(useGraphEnd()[0])
  const bounds = useScrollBounds(scroll)
  const actions = useMemo(
    () => graphScrollActions(window, scrolled, bounds),
    [window, scrolled, bounds],
  )
  useRegisterActions(actions)
}

/**
 * ⌘↑ and ⌘↓ step through a page's selector; bare arrows are left to the page's lists. Off
 * while something inside the selected view (one person) takes the keys instead.
 */
export function useGraphAccountShortcuts(onDirection: (direction: -1 | 1) => void, enabled = true) {
  const latest = useLatest(onDirection)
  const actions = useMemo<AppAction[]>(
    () =>
      enabled
        ? [
            {
              id: 'graph.selection.previous',
              title: 'Previous in selector',
              search: 'previous account holding report chart selector strip',
              group: 'View',
              scope: 'page',
              bindings: [{ sequence: ['Meta+ArrowUp'], label: '⌘↑' }],
              run: () => latest.current(-1),
            },
            {
              id: 'graph.selection.next',
              title: 'Next in selector',
              search: 'next account holding report chart selector strip',
              group: 'View',
              scope: 'page',
              bindings: [{ sequence: ['Meta+ArrowDown'], label: '⌘↓' }],
              run: () => latest.current(1),
            },
          ]
        : [],
    [enabled, latest],
  )
  useRegisterActions(actions)
}
