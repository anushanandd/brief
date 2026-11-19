// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { MerchantGroups, merchantDates } from './merchant-groups'

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

/** A merchant's activity with the total Rust summed for it. */
const group = (activities: ActivityItem[], total: number) => ({
  key: 'example bistro',
  name: 'Example bistro',
  total,
  activities,
})

it('shows a merchant with one purchase as that purchase', async () => {
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <MerchantGroups
        groups={[group([purchase('one', -10)], -10)]}
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

it('dates a merchant by its days, or their range past three', () => {
  const on = (...dates: string[]) =>
    dates.map((date, index) => ({ ...purchase(`${index}`, -10), date }))
  const reference = '2026-09-30T12:00:00Z'
  expect(merchantDates(on('2026-09-14', '2026-09-14'), reference)).toBe('Sep 14')
  expect(merchantDates(on('2026-09-14', '2026-08-30', '2026-09-03'), reference)).toBe(
    'Aug 30, Sep 3, 14',
  )
  expect(merchantDates(on('2026-09-01', '2026-09-02', '2026-09-03', '2026-09-20'), reference)).toBe(
    'Sep 1 – Sep 20',
  )
})

it('collapses a merchant at once', async () => {
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <MerchantGroups
        groups={[group([purchase('one', -10), purchase('two', -20)], -30)]}
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
