// @vitest-environment jsdom
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { act } from 'react'
import type { ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it } from 'vitest'

import { accountGroups } from '../lib/account-groups'
import { AccountGroups, AccountRows } from './account-groups'

const changes = [{ accountId: 'checking', change: 1 }]
const accounts = [
  { id: 'savings', name: 'Savings', institution: 'Example', type: 'cash', value: 20 },
  { id: 'checking', name: 'Checking', institution: 'Example', type: 'cash', value: 10 },
]
const accountNames = { savings: 'Reserve' }

async function render(view: ReactNode) {
  const router = createRouter({
    routeTree: createRootRoute({ component: () => view }),
    history: createMemoryHistory(),
  })
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () => root.render(<RouterProvider router={router} />))
  return {
    container,
    unmount: async () => {
      await act(async () => root.unmount())
      container.remove()
    },
  }
}

it('expands a category total to its accounts', async () => {
  const { container, unmount } = await render(
    <AccountGroups
      groups={accountGroups(accounts, [{ type: 'cash', value: 30 }], [{ type: 'cash', change: 1 }])}
      changes={changes}
      accountNames={accountNames}
    />,
  )
  const toggle = container.querySelector<HTMLButtonElement>('button[aria-expanded]')!
  expect(toggle.textContent).toContain('Cash')
  expect(toggle.textContent).toContain('2 accounts')
  expect(toggle.getAttribute('aria-expanded')).toBe('false')
  expect(toggle.hasAttribute('data-keyboard-open')).toBe(true)
  expect(container.querySelector('a.account-row')).toBeNull()

  await act(async () => toggle.click())
  expect(toggle.getAttribute('aria-expanded')).toBe('true')
  const rows = [...container.querySelectorAll('a.account-row')]
  expect(rows).toHaveLength(2)
  expect(rows[0].textContent).toContain('Reserve')
  expect(rows[0].querySelector('.account-mark')).not.toBeNull()
  expect(rows[0].textContent).toContain('Example')
  expect(rows[0].textContent).not.toContain('cash')
  expect(rows[1].textContent).toContain('+$1.00')

  await unmount()
})

it('shows a category of one account as that account, with nothing to expand', async () => {
  const { container, unmount } = await render(
    <AccountGroups
      groups={accountGroups(accounts.slice(0, 1), [{ type: 'cash', value: 20 }])}
      accountNames={accountNames}
    />,
  )
  expect(container.querySelector('button[aria-expanded]')).toBeNull()
  const rows = [...container.querySelectorAll('a.account-row')]
  expect(rows).toHaveLength(1)
  expect(rows[0].textContent).toContain('Reserve')
  expect(rows[0].querySelector('.account-mark')).not.toBeNull()

  await unmount()
})

it('lists every account without category rows when expanded', async () => {
  const { container, unmount } = await render(
    <AccountRows accounts={accounts} changes={changes} accountNames={accountNames} />,
  )
  expect(container.querySelector('button[aria-expanded]')).toBeNull()
  const rows = [...container.querySelectorAll('a.account-row')]
  expect(rows).toHaveLength(2)
  expect(rows[0].textContent).toContain('Reserve')
  expect(rows[0].querySelector('.account-mark')).not.toBeNull()
  expect(rows[1].textContent).toContain('+$1.00')

  await unmount()
})
