import { expect, it } from 'vitest'

import { navigation } from './navigation'
import { movePage, parsePageOrder } from './navigation-preferences'

it('restores valid ordering, discards unknowns and duplicates, and appends missing pages', () => {
  const defaults = navigation.map(({ to }) => to)
  expect(parsePageOrder('broken')).toEqual(defaults)
  expect(parsePageOrder('{"to":"/holdings"}')).toEqual(defaults)
  const order = parsePageOrder('["/holdings","/spending","/holdings","/settings","/unknown"]')
  expect(order.slice(0, 2)).toEqual(['/holdings', '/spending'])
  expect(new Set(order)).toEqual(new Set(defaults))
  expect(order).toHaveLength(defaults.length)
  expect(movePage(order, 0, -1)).toEqual(order)
  expect(movePage(order, 0, 1).slice(0, 2)).toEqual(['/spending', '/holdings'])
  expect(order[0]).toBe('/holdings')
})
