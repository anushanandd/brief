// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { MerchantGroups } from './merchant-groups'

const purchase = (id: string, amount: number): ActivityItem => ({
  id: `spending:${id}`,
  kind: 'spending',
  mark: 'initial',
  account: 'Example card',
  category: 'Dining',
  title: 'Example bistro',
  detail: 'Example card · Dining',
  date: '2026-09-02',
  amount,
  pending: false,
  other: false,
})

it('shows a merchant with one purchase as that purchase', async () => {
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <MerchantGroups
        activities={[purchase('one', -10)]}
        referenceIso="2026-09-10T12:00:00Z"
        emptyMessage="None"
      />,
    ),
  )
  expect(container.querySelector('button[aria-expanded]')).toBeNull()
  expect(container.querySelector('[data-activity-id="spending:one"]')).not.toBeNull()
  expect(container.textContent).toContain('Example card · Dining')
  expect(container.textContent).not.toContain('1 purchase')
  await act(async () => root.unmount())
  container.remove()
})

it('collapses a merchant at once', async () => {
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <MerchantGroups
        activities={[purchase('one', -10), purchase('two', -20)]}
        referenceIso="2026-09-10T12:00:00Z"
        emptyMessage="None"
      />,
    ),
  )
  const parent = container.querySelector<HTMLButtonElement>('button[aria-expanded]')!
  await act(async () => parent.click())
  expect(parent.getAttribute('aria-expanded')).toBe('true')
  expect(container.querySelectorAll('[data-activity-id]')).toHaveLength(2)

  // Closing never waits on a fade: the rows and their space go in the same render.
  await act(async () => parent.click())
  expect(parent.getAttribute('aria-expanded')).toBe('false')
  expect(container.querySelectorAll('[data-activity-id]')).toHaveLength(0)

  await act(async () => root.unmount())
  container.remove()
})
