import { expect, it } from 'vitest'

import {
  frecency,
  matchPaletteItem,
  matchTier,
  moveFavorite,
  paletteTabPrefix,
  queryPicks,
  toggleFavorite,
} from './palette-ranking'

const score = (query: string, title: string, options?: Parameters<typeof matchPaletteItem>[2]) =>
  matchPaletteItem(query, title, options).score

it('ranks exact, alias, prefix and word-start title matches in tiers', () => {
  expect(score('accounts', 'Accounts')).toBe(matchTier.exact)
  expect(score('nvda', 'NVDA · NVIDIA', { aliases: ['NVDA'] })).toBe(matchTier.alias)
  expect(score('acc', 'Accounts')).toBe(matchTier.prefix)
  expect(score('acc', 'Open Accounts', { aliases: ['Accounts'] })).toBe(matchTier.aliasPrefix)
  expect(score('acc', 'Open Accounts')).toBe(matchTier.wordStart)
  expect(score('open acc', 'Open previous Accounts')).toBe(matchTier.allTermsInTitle)
  expect(score('sdt', 'Switch to Dark theme')).toBe(matchTier.acronym)
  expect(score('mode', 'Switch to Dark theme', { keywords: ['dark mode appearance'] })).toBe(
    matchTier.keyword,
  )
})

it('treats a single character as a title word start only', () => {
  expect(score('a', 'Open Accounts')).toBe(matchTier.wordStart)
  expect(score('a', 'Refresh snapshot', { keywords: ['refresh data finance'] })).toBe(0)
  expect(score('a', 'Show sensitive values')).toBe(0)
})

it('only accepts loose substrings once a query is specific', () => {
  expect(score('na', 'Refresh snapshot')).toBe(0)
  expect(score('nap', 'Refresh snapshot')).toBe(matchTier.substring)
})

it('returns highlight ranges for the matched title characters', () => {
  expect(matchPaletteItem('acc', 'Open Accounts').ranges).toEqual([[5, 8]])
  expect(matchPaletteItem('sdt', 'Switch to Dark theme').ranges).toEqual([
    [0, 1],
    [10, 11],
    [15, 16],
  ])
})

it('weights recent use above old use and learns picks for a typed prefix', () => {
  const now = Date.UTC(2026, 9, 1)
  const day = 86_400_000
  expect(frecency({ count: 2, lastUsed: now }, now)).toBeGreaterThan(
    frecency({ count: 5, lastUsed: now - 30 * day }, now),
  )
  const usage = { calendar: { 'open.calendar': { count: 3, lastUsed: now } } }
  expect(queryPicks(usage, 'cal', 'open.calendar', now)).toBe(3)
  expect(queryPicks(usage, 'cal', 'open.calculator', now)).toBe(0)
  expect(queryPicks(usage, 'calx', 'open.calendar', now)).toBe(0)
})

it('toggles and reorders favorites without leaving the list', () => {
  expect(toggleFavorite(['a'], 'b')).toEqual(['a', 'b'])
  expect(toggleFavorite(['a', 'b'], 'a')).toEqual(['b'])
  expect(moveFavorite(['a', 'b', 'c'], 'c', -1)).toEqual(['a', 'c', 'b'])
  expect(moveFavorite(['a', 'b'], 'a', -1)).toEqual(['a', 'b'])
})

it('reads a leading prefix as a jump to its tab', () => {
  expect(paletteTabPrefix('> dark')).toMatchObject({ tab: { id: 'commands' }, term: 'dark' })
  expect(paletteTabPrefix('@doc w-2')).toMatchObject({ tab: { id: 'objects' }, term: 'doc w-2' })
  expect(paletteTabPrefix('dark')).toBeUndefined()
})
