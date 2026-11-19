// @vitest-environment jsdom
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { holding } from '../data/fixtures/records'
import { PositionTable } from './position-table'

it('opens the security from anywhere on its row, once', async () => {
  const router = createRouter({
    routeTree: createRootRoute({
      component: () => (
        <PositionTable
          positions={[holding({ accountId: 'synthetic', ticker: 'TEST', price: 10, value: 20 })]}
          view="market"
        />
      ),
    }),
    history: createMemoryHistory(),
  })
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () => root.render(<RouterProvider router={router} />))
  const link = container.querySelector<HTMLAnchorElement>('tbody a')!
  const opened = vi.spyOn(link, 'click')

  container.querySelector<HTMLElement>('tbody td:last-child')!.click()
  expect(opened).toHaveBeenCalledTimes(1)

  // A click on the link itself is the link's own, so the row doesn't open it again.
  link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
  expect(opened).toHaveBeenCalledTimes(1)

  await act(async () => root.unmount())
  container.remove()
})
