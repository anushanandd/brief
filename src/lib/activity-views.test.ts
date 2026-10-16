import { expect, it } from 'vitest'

import { activityViewSearch, withActivityView } from './activity-views'

it('keeps saved views unique by name, newest first, and drops non-filter state', () => {
  const first = { name: 'Dining', search: { category: 'Dining' } }
  const second = { name: 'Card', search: { account: 'card' } }
  expect(withActivityView([first, second], { name: 'Dining', search: { q: 'coffee' } })).toEqual([
    { name: 'Dining', search: { q: 'coffee' } },
    second,
  ])
  expect(
    activityViewSearch({ q: 'coffee', category: '', account: 'card', sort: 'date', page: 2 }),
  ).toEqual({ q: 'coffee', account: 'card' })
})
