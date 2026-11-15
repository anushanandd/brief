import { expect, it } from 'vitest'

import { pageSections, sectionHome, sectionTabActive } from './navigation'

/** The tabs a location marks as shown. */
const shown = (pathname: string, view?: string) =>
  Object.values(pageSections)
    .flat()
    .filter((tab) => sectionTabActive(tab, pathname, view))
    .map(({ label }) => label)

it('keeps each page under its sidebar page and marks only the one shown', () => {
  expect(sectionHome('/insights')).toBe('/insights')
  expect(sectionHome('/taxes')).toBe('/insights')
  expect(sectionHome('/documents')).toBe('/insights')
  // Activity and Calendar are pages of their own.
  expect(sectionHome('/activities')).toBeUndefined()
  expect(sectionHome('/calendar')).toBeUndefined()
  // Trips is Insights without the people view, even on one trip or another year.
  expect(shown('/insights')).toEqual(['Trips'])
  expect(shown('/insights', 'people')).toEqual(['People'])
  expect(shown('/taxes')).toEqual(['Taxes'])
  expect(shown('/documents')).toEqual(['Documents'])
})
