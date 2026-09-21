import { expect, it } from 'vitest'

import { searchShortcutAction } from '../hooks/use-search-shortcuts'
import { activityDateGroup, groupActivitiesByDate, type ActivityItem } from '../lib/activity'
import { accountActivityPreviewLimit } from './account-overview'
import { activityDateBounds, activityMatchesDateRange } from './activities'

it('bounds account previews by their companion content', () => {
  expect(accountActivityPreviewLimit(4, 0)).toBe(6)
  expect(accountActivityPreviewLimit(2, 1)).toBe(4)
  expect(accountActivityPreviewLimit(3, 3)).toBe(7)
})

const activity = (id: string, date: string): ActivityItem => ({
  id,
  kind: 'transaction',
  category: 'Other',
  title: id,
  detail: 'Test activity',
  date,
  amount: 1,
})

it('groups activity into relative date sections', () => {
  const reference = '2026-09-18T12:00:00Z'
  expect(activityDateGroup('2026-09-18', reference)).toBe('Today')
  expect(activityDateGroup('2026-09-17', reference)).toBe('Yesterday')
  expect(activityDateGroup('2026-09-12', reference)).toBe('Last week')
  expect(activityDateGroup('2026-08-25', reference)).toBe('August')
  expect(activityDateGroup('2026-07-01', reference)).toBe('July')

  expect(
    groupActivitiesByDate(
      [
        activity('today', '2026-09-18'),
        activity('yesterday', '2026-09-17'),
        activity('week', '2026-09-12'),
      ],
      reference,
    ).map(({ label, activities }) => [label, activities.map(({ id }) => id)]),
  ).toEqual([
    ['Today', ['today']],
    ['Yesterday', ['yesterday']],
    ['Last week', ['week']],
  ])
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

it('focuses, clears, and blurs ledger search without stealing input', () => {
  const slash = { key: '/', metaKey: false, ctrlKey: false, altKey: false }
  const escape = { ...slash, key: 'Escape' }
  expect(searchShortcutAction(slash, false, false, false)).toBe('focus')
  expect(searchShortcutAction(slash, true, false, false)).toBeUndefined()
  expect(searchShortcutAction({ ...slash, metaKey: true }, false, false, false)).toBeUndefined()
  expect(searchShortcutAction(escape, true, true, true)).toBe('clear')
  expect(searchShortcutAction(escape, true, true, false)).toBe('blur')
})

it('starts a named month group at each calendar boundary and distinguishes prior years', () => {
  const reference = '2026-09-03T12:00:00Z'
  const items = [
    activity('today', '2026-09-03'),
    activity('yesterday', '2026-09-02'),
    activity('week', '2026-09-01'),
    activity('august', '2026-08-31'),
    activity('july', '2026-07-15'),
    activity('old', '2025-07-15'),
  ]
  expect(groupActivitiesByDate(items, reference).map(({ label }) => label)).toEqual([
    'Today',
    'Yesterday',
    'Last week',
    'August',
    'July',
    'July 2025',
  ])
  expect(activityDateGroup('2026-09-01', '2026-09-18T12:00:00Z')).toBe('September')
})
