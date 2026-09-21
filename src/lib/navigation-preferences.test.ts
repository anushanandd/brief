import { expect, it } from 'vitest'

import { navigation } from './navigation'
import { adjacentPage, movePage, parsePageOrder } from './navigation-preferences'

it('restores valid ordering, discards unknowns and duplicates, and appends missing pages', () => {
  const defaults = navigation.map(({ to }) => to)
  expect(parsePageOrder('broken')).toEqual(defaults)
  expect(parsePageOrder('{"to":"/holdings"}')).toEqual(defaults)
  const order = parsePageOrder('["/holdings","/settings","/holdings","/unknown"]')
  expect(order.slice(0, 2)).toEqual(['/holdings', '/settings'])
  expect(new Set(order)).toEqual(new Set(defaults))
  expect(order).toHaveLength(defaults.length)
  expect(movePage(order, 0, -1)).toEqual(order)
  expect(movePage(order, 0, 1).slice(0, 2)).toEqual(['/settings', '/holdings'])
  expect(order[0]).toBe('/holdings')
})

it('cycles the saved order including nested routes and wraps at either end', () => {
  const order = parsePageOrder('["/holdings","/settings"]')
  expect(adjacentPage(order, '/holdings', 1)).toBe('/settings')
  expect(adjacentPage(order, '/settings/design', -1)).toBe('/holdings')
  expect(adjacentPage(order, '/accounts/investments', -1)).toBe('/')
  expect(adjacentPage(order, '/holdings', -1)).toBe(order.at(-1))
  expect(adjacentPage(order, order.at(-1)!, 1)).toBe('/holdings')
})
