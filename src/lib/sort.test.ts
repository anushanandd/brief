import { expect, it } from 'vitest'

import { nextSort } from './sort'

it('reverses the sorted column and starts another in its own direction', () => {
  const byDate = { column: 'date', direction: 'desc' } as const
  expect(nextSort(byDate, 'date', 'desc')).toEqual({ column: 'date', direction: 'asc' })
  expect(nextSort(byDate, 'name', 'asc')).toEqual({ column: 'name', direction: 'asc' })
})
