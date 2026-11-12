// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { beforeAll, expect, it, vi } from 'vitest'

import { snapshotKey } from '../hooks/use-finance'
import {
  contextualListActions,
  currentKeyboardRegion,
  openFocusedKeyboardRow,
} from '../lib/keyboard-regions'
import type { HealthReport } from '../lib/schema'
import { HealthPage } from './health'

vi.mock('../components/workspace-header', () => ({ WorkspaceHeader: () => <h1>Data health</h1> }))
vi.mock('../hooks/use-finance', async (original) => ({
  ...(await original<typeof import('../hooks/use-finance')>()),
  useFinance: () => ({ data: snapshot, isLoading: false, isError: false }),
}))
const snapshot = { revision: 1, updatedAt: '2026-09-18T20:00:00Z' }

// jsdom lays nothing out; list motions skip rows without boxes, so every row gets one.
beforeAll(() => {
  const box: DOMRectList = Object.assign([new DOMRect()], { item: () => new DOMRect() })
  HTMLElement.prototype.getClientRects = () => box
  HTMLElement.prototype.scrollIntoView = () => undefined
})

const issue = (
  id: string,
  severity: HealthReport['issues'][number]['severity'],
  route: string,
): HealthReport['issues'][number] => ({
  id,
  severity,
  category: 'storage',
  title: `Synthetic ${id}`,
  explanation: 'A synthetic check',
  evidence: [],
  affectedItems: [],
  action: { label: `Fix ${id}`, route },
})

const report: HealthReport = {
  computedAt: '2026-09-18T20:00:00Z',
  revision: 1,
  overallStatus: 'warning',
  counts: { critical: 0, error: 0, warning: 2, info: 1 },
  issues: [
    issue('stale', 'warning', '/accounts'),
    issue('conflict', 'warning', '/activities?evidence=conflict'),
    issue('backup', 'info', '/settings#connections'),
  ],
}

async function render() {
  // Never stale, so the page shows this report instead of fetching one.
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } })
  client.setQueryData(['data-health', ...snapshotKey(snapshot)], report)
  const history = createMemoryHistory({ initialEntries: ['/health'] })
  const root = createRootRoute()
  const routeTree = root.addChildren([
    createRoute({ getParentRoute: () => root, path: '/health', component: HealthPage }),
  ])
  const router = createRouter({ routeTree, history })
  await router.load()
  const container = document.createElement('div')
  document.body.append(container)
  const view = createRoot(container)
  await act(async () =>
    view.render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    ),
  )
  return {
    container,
    history,
    unmount: async () => {
      await act(async () => view.unmount())
      container.remove()
      client.clear()
    },
  }
}

it('steps through each severity’s issues with J and K and opens the fix with Return', async () => {
  const { container, history, unmount } = await render()
  const regions = [...container.querySelectorAll<HTMLElement>('[data-keyboard-region]')]
  expect(regions.map((region) => region.getAttribute('aria-label'))).toEqual([
    'Limited data',
    'Information',
  ])
  const [limited] = regions
  const rows = limited?.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  expect(rows).toHaveLength(2)

  rows?.[0]?.focus()
  const region = currentKeyboardRegion()
  expect(region).toBe(limited)
  const next = contextualListActions(region).find(({ id }) => id.startsWith('motion.next'))
  await act(async () => next?.run())
  expect(document.activeElement).toBe(rows?.[1])

  await act(async () => openFocusedKeyboardRow())
  expect(history.location.pathname).toBe('/activities')
  expect(history.location.search).toContain('evidence=conflict')
  await unmount()
})

it('opens a Settings fix over the page through the URL hash', async () => {
  const { container, history, unmount } = await render()
  const row = container.querySelector<HTMLElement>('[aria-label="Information"] [data-keyboard-row]')
  row?.focus()
  await act(async () => openFocusedKeyboardRow())
  expect(history.location.pathname).toBe('/health')
  expect(history.location.hash).toBe('#settings/connections')
  await unmount()
})
