import { expect, it } from 'vitest'

import { searchShortcutAction } from '../hooks/use-search-shortcuts'
import { activityDateGroup, groupActivitiesByDate, type ActivityItem } from '../lib/activity'
import { activityClearShortcut, activityFilterShortcut } from './activities'

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

it('focuses, clears, and blurs ledger search without stealing input', () => {
  const slash = { key: '/', metaKey: false, ctrlKey: false, altKey: false }
  const escape = { ...slash, key: 'Escape' }
  expect(searchShortcutAction(slash, false, false, false)).toBe('focus')
  expect(searchShortcutAction(slash, true, false, false)).toBeUndefined()
  expect(searchShortcutAction({ ...slash, metaKey: true }, false, false, false)).toBeUndefined()
  expect(searchShortcutAction(escape, true, true, true)).toBe('clear')
  expect(searchShortcutAction(escape, true, true, false)).toBe('blur')
})

it('focuses filters with F and clears them with C outside editing and overlays', () => {
  const event = {
    key: 'f',
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    repeat: false,
  }
  expect(activityFilterShortcut(event, false)).toBe(true)
  expect(activityFilterShortcut({ ...event, key: 'F' }, false)).toBe(true)
  expect(activityFilterShortcut({ ...event, key: '1' }, false)).toBe(false)
  expect(activityFilterShortcut({ ...event, metaKey: true }, false)).toBe(false)
  expect(activityFilterShortcut(event, true)).toBe(false)
  expect(activityClearShortcut({ ...event, key: 'c' }, false)).toBe(true)
  expect(activityClearShortcut({ ...event, key: 'C' }, false)).toBe(true)
  expect(activityClearShortcut({ ...event, key: 'c', repeat: true }, false)).toBe(false)
  expect(activityClearShortcut({ ...event, key: 'c', metaKey: true }, false)).toBe(false)
  expect(activityClearShortcut({ ...event, key: 'c' }, true)).toBe(false)
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
