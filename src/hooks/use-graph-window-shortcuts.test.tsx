// @vitest-environment jsdom

import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'

import { ActionProvider, useRegisteredActions, type AppAction } from '../lib/action-system'
import { addDays } from '../lib/dates'
import {
  scrollPeriodDays,
  sessionGraphEnd,
  sessionGraphWindow,
  setGraphEnd,
  type GraphScroll,
} from '../lib/graph-preferences'
import { useGraphAccountShortcuts, useGraphScrollShortcuts } from './use-graph-window-shortcuts'

let registered: readonly AppAction[] = []
const lists = new Set<readonly AppAction[]>()

function Reader() {
  registered = useRegisteredActions()
  lists.add(registered)
  return null
}

function Page({ scroll, onDirection }: { scroll: GraphScroll; onDirection: (d: -1 | 1) => void }) {
  useGraphScrollShortcuts(scroll)
  // Inline, as most pages pass it.
  useGraphAccountShortcuts((direction) => onDirection(direction))
  return null
}

afterEach(() => {
  setGraphEnd(undefined)
  lists.clear()
  document.body.replaceChildren()
})

const run = (id: string) => registered.find((action) => action.id === id)?.run()

it('keeps the page keys through live redraws and days scrolled, reading the end day when run', async () => {
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  const render = (onDirection: (d: -1 | 1) => void) =>
    act(async () =>
      root.render(
        <ActionProvider>
          <Page scroll={{ today: '2026-06-30', first: '2025-01-01' }} onDirection={onDirection} />
          <Reader />
        </ActionProvider>,
      ),
    )
  await render(() => undefined)
  const settled = registered
  // A live tick: new bounds of the same days and a new inline handler.
  const stepped = vi.fn()
  await render(stepped)
  expect(registered).toBe(settled)
  run('graph.selection.next')
  expect(stepped).toHaveBeenCalledWith(1)

  // Leaving today enables the way back; each later day keeps the list.
  await act(async () => setGraphEnd('2026-06-20'))
  const scrolled = registered
  expect(scrolled).not.toBe(settled)
  await act(async () => setGraphEnd('2026-06-19'))
  expect(registered).toBe(scrolled)

  const days = scrollPeriodDays(sessionGraphWindow())
  if (!days) throw new Error('The default range scrolls')
  await act(async () => run('graph.end.earlier'))
  expect(sessionGraphEnd()).toBe(addDays('2026-06-19', -days))
  await act(async () => root.unmount())
})
