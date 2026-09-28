import { expect, it } from 'vitest'

import { analyticsCharts } from './analytics'
import { moveAnalyticsChart, parseAnalyticsOrder } from './analytics-order-preferences'

it('starts with Cash flow and restores a valid saved chart order', () => {
  const defaults = analyticsCharts.map(({ id }) => id)
  expect(parseAnalyticsOrder(null)).toEqual(defaults)
  expect(defaults[0]).toBe('cash-flow')
  expect(parseAnalyticsOrder('["fees","fees","unknown","cash-flow"]')).toEqual([
    'fees',
    'cash-flow',
    ...defaults.filter((id) => id !== 'fees' && id !== 'cash-flow'),
  ])
  expect(parseAnalyticsOrder('broken')).toEqual(defaults)
  expect(moveAnalyticsChart(defaults, 0, 1).slice(0, 2)).toEqual(['income', 'cash-flow'])
})
