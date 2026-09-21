import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { GroupedActivityList } from './activity-list'

it('renders an initial batch and keeps a keyboard-accessible way to load the rest', () => {
  const activities: ActivityItem[] = Array.from({ length: 1000 }, (_, i) => ({
    id: `synthetic-${i}`,
    kind: 'income',
    category: 'Income',
    title: `Synthetic ${i}`,
    detail: 'Synthetic account',
    date: '2026-09-20',
    amount: 10,
  }))
  const html = renderToStaticMarkup(
    <GroupedActivityList activities={activities} referenceIso="2026-09-20" />,
  )
  expect(html.match(/data-keyboard-row/g)).toHaveLength(60)
  expect(html).toContain('Load more activities')
  const filtered = renderToStaticMarkup(
    <GroupedActivityList activities={activities.slice(-2)} referenceIso="2026-09-20" />,
  )
  expect(filtered).toContain('Synthetic 999')
  expect(filtered).not.toContain('Load more activities')
  expect(activities).toHaveLength(1000)
})
