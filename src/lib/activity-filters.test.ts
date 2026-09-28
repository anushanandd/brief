import { expect, it } from 'vitest'

import type { ActivityItem } from './activity'
import {
  activityDateBounds,
  activityDatePreset,
  activityDateRangeValue,
  activityFilterCandidates,
  activityMatchesDateRange,
} from './activity-filters'

const activity = (id: string, date: string): ActivityItem => ({
  id,
  kind: 'transaction',
  category: 'Other',
  title: id,
  detail: 'Test activity',
  date,
  amount: 1,
})

it('applies inclusive activity date filters', () => {
  const reference = '2026-09-18T12:00:00Z'
  expect(
    activityDateBounds(
      [activity('latest', '2026-09-18'), activity('earliest', '2026-09-11')],
      reference,
    ),
  ).toEqual(['2026-09-11', '2026-09-18'])
  expect(
    activityMatchesDateRange(
      activity('included', '2026-09-12'),
      reference,
      '2026-09-12',
      '2026-09-18',
    ),
  ).toBe(true)
  expect(
    activityMatchesDateRange(
      activity('excluded', '2026-09-11'),
      reference,
      '2026-09-12',
      '2026-09-18',
    ),
  ).toBe(false)
})

it('uses the shared rolling windows as inclusive activity date presets', () => {
  expect(activityDatePreset('a', '2026-09-23')).toEqual({ from: undefined, to: undefined })
  expect(activityDatePreset('w', '2026-09-23')).toEqual({
    from: '2026-09-17',
    to: '2026-09-23',
  })
  expect(activityDatePreset('m', '2026-09-23')).toEqual({
    from: '2026-08-25',
    to: '2026-09-23',
  })
  expect(activityDatePreset('m', '2024-03-01')).toEqual({
    from: '2024-02-01',
    to: '2024-03-01',
  })
  expect(activityDatePreset('q', '2026-09-23')).toEqual({
    from: '2026-06-26',
    to: '2026-09-23',
  })
  expect(activityDatePreset('y', '2026-09-23')).toEqual({
    from: '2025-09-24',
    to: '2026-09-23',
  })
  expect(activityDateRangeValue(undefined, undefined, '2026-09-23')).toBe('a')
  expect(activityDateRangeValue('2026-08-25', '2026-09-23', '2026-09-23')).toBe('m')
  expect(activityDateRangeValue('2026-08-25', '2026-09-22', '2026-09-23')).toBe('custom')
})

it('limits each filter choice to matching activity while leaving its own facet open', () => {
  const entries = [
    {
      ...activity('chipotle checking', '2026-09-10'),
      accountId: 'checking',
      category: 'Dining',
      paymentChannel: 'in store',
    },
    {
      ...activity('chipotle credit', '2026-09-10'),
      accountId: 'credit',
      category: 'Dining',
      paymentChannel: 'online',
    },
    {
      ...activity('chipotle grocery', '2026-09-10'),
      accountId: 'checking',
      category: 'Groceries',
      paymentChannel: 'online',
    },
    {
      ...activity('coffee', '2026-09-10'),
      accountId: 'checking',
      category: 'Dining',
      paymentChannel: 'in store',
    },
    {
      ...activity('chipotle old', '2026-08-10'),
      accountId: 'checking',
      category: 'Dining',
      paymentChannel: 'in store',
    },
  ]
  const candidates = activityFilterCandidates(
    entries,
    '2026-09-18T12:00:00Z',
    'chipotle',
    '2026-09-01',
    undefined,
    new Set(['checking']),
    ['Dining'],
    ['In store'],
  )
  expect(candidates.accounts.map(({ id }) => id)).toEqual(['chipotle checking'])
  expect(candidates.categories.map(({ id }) => id)).toEqual(['chipotle checking'])
  expect(candidates.methods.map(({ id }) => id)).toEqual(['chipotle checking'])
  expect(candidates.results.map(({ id }) => id)).toEqual(['chipotle checking'])

  const wider = activityFilterCandidates(
    entries,
    '2026-09-18T12:00:00Z',
    'chipotle',
    '2026-09-01',
    undefined,
    new Set(['checking']),
    ['Dining'],
    [],
  )
  expect(wider.accounts.map(({ accountId }) => accountId)).toEqual(['checking', 'credit'])
  expect(wider.categories.map(({ category }) => category)).toEqual(['Dining', 'Groceries'])
  expect(wider.methods.map(({ paymentChannel }) => paymentChannel)).toEqual(['in store'])
})
