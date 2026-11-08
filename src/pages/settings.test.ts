import { expect, it } from 'vitest'

import { sectionInView, settingsSectionFor } from './settings'

const tops = [
  { id: 'connections', top: 0 },
  { id: 'accounts', top: 600 },
  { id: 'advanced', top: 1400 },
] as const

it('marks the section whose top has reached the upper part of the scroller', () => {
  const scroller = { clientHeight: 600, scrollHeight: 1800 }
  expect(sectionInView(tops, { ...scroller, scrollTop: 0 })).toBe('connections')
  expect(sectionInView(tops, { ...scroller, scrollTop: 479 })).toBe('connections')
  expect(sectionInView(tops, { ...scroller, scrollTop: 480 })).toBe('accounts')
  expect(sectionInView(tops, { ...scroller, scrollTop: 1100 })).toBe('accounts')
})

it('marks the last section once the scroller reaches the bottom', () => {
  // A short last section can't reach the top, so the bottom stands for it.
  expect(sectionInView(tops, { clientHeight: 600, scrollHeight: 1800, scrollTop: 1200 })).toBe(
    'advanced',
  )
  expect(sectionInView([], { clientHeight: 600, scrollHeight: 600, scrollTop: 0 })).toBeUndefined()
})

it('finds the section holding a row, and the first section otherwise', () => {
  expect(settingsSectionFor('platinum-benefits')).toBe('accounts')
  expect(settingsSectionFor('advanced')).toBe('advanced')
  expect(settingsSectionFor(undefined)).toBe('connections')
  expect(settingsSectionFor('no-such-row')).toBe('connections')
})
