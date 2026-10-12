// @vitest-environment jsdom

import { createMemoryHistory, createRouter } from '@tanstack/react-router'
import { expect, it } from 'vitest'

import { router } from './router'

it('redirects Settings paths to the dialog at their sections', async () => {
  for (const [path, hash] of [
    ['/settings', 'settings'],
    ['/settings/accounts', 'settings/accounts'],
    ['/settings/connections', 'settings/connections'],
    ['/settings/data', 'settings/privacy'],
    ['/settings/accounts#platinum-benefits', 'settings/platinum-benefits'],
    ['/settings#data-sources', 'settings/connections'],
  ] as const) {
    const testRouter = createRouter({
      routeTree: router.routeTree,
      history: createMemoryHistory({ initialEntries: [path] }),
    })
    await testRouter.load()
    expect(testRouter.state.location.pathname).toBe('/')
    expect(testRouter.state.location.hash).toBe(hash)
  }
})
