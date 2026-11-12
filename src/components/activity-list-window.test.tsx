// @vitest-environment jsdom

import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterAll, beforeAll, expect, it, vi } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { contextualListActions } from '../lib/keyboard-regions'
import { GroupedActivityList } from './activity-list'

const activities: ActivityItem[] = Array.from({ length: 5_000 }, (_, i) => ({
  id: `synthetic-${i}`,
  kind: 'spending',
  category: 'Dining',
  title: `Synthetic ${i}`,
  detail: 'Synthetic account',
  date: '2026-09-20',
  amount: -5,
}))

const rows = () => document.querySelectorAll<HTMLElement>('[data-keyboard-row]')
const focusedId = () =>
  document.activeElement instanceof HTMLElement
    ? document.activeElement.dataset.activityId
    : undefined

beforeAll(() => {
  // jsdom lays nothing out: rows count as visible, nothing scrolls, and there's no canvas.
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue(
    Object.assign([new DOMRect(0, 0, 10, 10)], { item: () => null }),
  )
  HTMLElement.prototype.scrollIntoView = () => undefined
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
})

afterAll(() => {
  vi.restoreAllMocks()
  document.body.replaceChildren()
})

it('draws a bounded window of a long ledger, and J still walks past it', async () => {
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <GroupedActivityList activities={activities} referenceIso="2026-09-20" showColumns />,
    ),
  )
  expect(rows().length).toBeLessThanOrEqual(120)
  const [first] = rows()
  if (!first) throw new Error('The ledger draws rows')
  await act(async () => first.focus())
  const region = first.closest<HTMLElement>('[data-keyboard-region]') ?? undefined
  const actions = contextualListActions(region)
  const motion = (name: string) => actions.find(({ id }) => id.startsWith(`motion.${name}`))
  await act(async () => motion('next')?.run())
  expect(focusedId()).toBe('synthetic-1')
  // ⇧G reaches the last loaded row, and J the next page, which loads as focus nears it.
  const index = () => Number(focusedId()?.replace('synthetic-', ''))
  while (index() < 300) {
    await act(async () => motion('last')?.run())
    const last = index()
    expect(last % 60).toBe(59)
    await act(async () => motion('next')?.run())
    expect(index()).toBe(last + 1)
  }
  expect(focusedId()).toBe('synthetic-360')
  // Eight pages have loaded; those far from focus are gaps. The first page, the focused one and
  // its neighbours stay drawn.
  expect(document.querySelectorAll('[data-activity-page]')).toHaveLength(8)
  expect(rows()).toHaveLength(4 * 60)
  expect(document.querySelector('[data-activity-id="synthetic-100"]')).toBeNull()
  await act(async () => root.unmount())
})
