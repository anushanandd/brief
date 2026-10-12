import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { groupByMerchant } from './category-merchants'

const activity = (id: string, title: string, amount: number): ActivityItem => ({
  id,
  kind: 'spending',
  category: 'Dining',
  title,
  detail: '',
  date: '2026-09-01',
  amount,
})

it('groups a category by merchant in cents, largest first, and marks recurring merchants', () => {
  const groups = groupByMerchant(
    [
      activity('a', 'Corner Cafe', -0.1),
      activity('b', 'corner cafe ', -0.2),
      activity('c', 'Synthetic Grill', -0.3),
      activity('d', 'Noodle Bar', -0.3),
    ],
    new Set(['c']),
  )
  expect(
    groups.map(({ name, total, recurring, activities }) => [
      name,
      total,
      recurring,
      activities.length,
    ]),
  ).toEqual([
    ['Corner Cafe', 0.3, false, 2],
    ['Noodle Bar', 0.3, false, 1],
    ['Synthetic Grill', 0.3, true, 1],
  ])
})
