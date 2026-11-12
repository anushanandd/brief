// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeAll, expect, it, vi } from 'vitest'

import { FinanceProvider } from '../hooks/finance-provider'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { ActionProvider } from '../lib/action-system'
import { router } from '../router'

// The snapshot read is the IPC call every page waits on; here it fails as a broken store would.
vi.mock('../lib/api', async (original) => ({
  ...(await original<typeof import('../lib/api')>()),
  getFinanceSnapshot: () => Promise.reject(new Error('Synthetic read failure')),
}))

beforeAll(() => {
  // jsdom has no layout or media queries; the shell asks for both.
  window.matchMedia ??= (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) satisfies MediaQueryList
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  HTMLElement.prototype.scrollIntoView = () => undefined
})

afterEach(() => document.body.replaceChildren())

/** Mounts the app at `path` with the real providers and waits for the read to settle. */
async function mount(path: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const testRouter = createRouter({
    routeTree: router.routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
    context: { queryClient: client },
  })
  await testRouter.load()
  const container = document.body.appendChild(document.createElement('div'))
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <ActionProvider>
          <FinanceProvider>
            <LiveMarketProvider>
              <RouterProvider router={testRouter} />
            </LiveMarketProvider>
          </FinanceProvider>
        </ActionProvider>
      </QueryClientProvider>,
    ),
  )
  // Lazy pages load, then the rejected read settles.
  for (let attempt = 0; attempt < 20 && !container.querySelector('h1'); attempt += 1)
    await act(async () => new Promise((resolve) => setTimeout(resolve, 10)))
  return {
    container,
    unmount: async () => {
      await act(async () => root.unmount())
      client.clear()
    },
  }
}

const mainPages = [
  '/',
  '/accounts',
  '/holdings',
  '/activities',
  '/spending',
  '/analytics',
  '/documents',
]

it.each(mainPages)('%s shows the read error, not a blank page or zeros', async (path) => {
  const { container, unmount } = await mount(path)
  const page = container.textContent ?? ''
  expect(container.querySelector('h1')?.textContent).toBe('We couldn’t load your snapshot.')
  expect(page).toContain('Synthetic read failure')
  expect(page).toContain('Try again')
  expect(page).not.toMatch(/\$0(\.00)?\b/)
  await unmount()
})
