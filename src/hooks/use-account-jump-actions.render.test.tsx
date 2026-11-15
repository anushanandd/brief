// @vitest-environment jsdom

import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { seedSnapshot } from '../data/fixtures/records'
import { ActionProvider } from '../lib/action-system'
import { useAccountJumpActions } from './use-account-jump-actions'

// The live overlay as a store that ticks on its own; the committed snapshot holds still.
const finance = vi.hoisted(() => {
  const listeners = new Set<() => void>()
  const store = {
    committed: undefined as unknown,
    live: undefined as unknown,
    tick(next: unknown) {
      store.live = next
      for (const notify of listeners) notify()
    },
    subscribe: (notify: () => void) => {
      listeners.add(notify)
      return () => listeners.delete(notify)
    },
  }
  return store
})
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => () => undefined }))
vi.mock('./use-finance', () => ({ useFinance: () => ({ data: finance.committed }) }))
vi.mock('./use-live-finance', async () => {
  const { useSyncExternalStore } = await import('react')
  return {
    useLiveFinance: () => ({ data: useSyncExternalStore(finance.subscribe, () => finance.live) }),
  }
})

let renders = 0
function Shell() {
  renders += 1
  useAccountJumpActions()
  return null
}

it('leaves the shell alone through a live tick', async () => {
  const data = seedSnapshot()
  finance.committed = data
  finance.live = data
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <ActionProvider>
        <Shell />
      </ActionProvider>,
    ),
  )
  const settled = renders
  await act(async () => finance.tick({ ...data, holdings: [...data.holdings] }))
  expect(renders).toBe(settled)
  await act(async () => root.unmount())
})
