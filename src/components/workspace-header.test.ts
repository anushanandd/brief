import { expect, it } from 'vitest'

import { formatUpdatedAt } from '../lib/format'
import { marketSessionLabel } from './workspace-header'

it('names closed weekend sessions without relabeling other sessions or weekday closures', () => {
  expect(marketSessionLabel('Market closed', new Date('2026-09-26T16:00:00Z'))).toBe('Weekend')
  expect(marketSessionLabel('Market closed', new Date('2026-09-28T16:00:00Z'))).toBe(
    'Market closed',
  )
  expect(marketSessionLabel('After hours', new Date('2026-09-26T16:00:00Z'))).toBe('After hours')
})

it('omits the current year from saved timestamps', () => {
  const year = new Date().getFullYear()
  expect(formatUpdatedAt(`${year}-06-15T12:34:00`)).not.toContain(String(year))
  expect(formatUpdatedAt(`${year - 1}-06-15T12:34:00`)).toContain(String(year - 1))
})
