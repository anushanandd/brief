import { expect, it } from 'vitest'

import { settingsHash, settingsTarget } from './settings-dialog'
import {
  searchSettings,
  settingsHashForPath,
  settingsMatchRanges,
  settingsSections,
} from './settings-search'

it('returns no filter for an empty query', () => {
  expect(searchSettings('')).toBeNull()
  expect(searchSettings('   ')).toBeNull()
})

it('requires every word and matches titles, keywords, and section names', () => {
  const keychain = searchSettings('alpaca key')!
  expect([...keychain.entryIds]).toEqual(['connection-alpaca'])
  expect([...keychain.sectionIds]).toEqual(['connections'])

  const privacy = searchSettings('PRIVACY')!
  expect(privacy.counts.get('privacy')).toBe(4)
  expect(privacy.total).toBe(4)

  expect(searchSettings('dark')!.entryIds.has('theme')).toBe(true)
  expect(searchSettings('collapsed')!.entryIds.has('home-accounts')).toBe(true)
  expect(searchSettings('rename')!.entryIds.has('account-details')).toBe(true)
  expect(searchSettings('zzz-not-a-setting')!.total).toBe(0)
})

it('keeps entry ids unique so every row can be a deep link', () => {
  const ids = settingsSections.flatMap((section) => [
    section.id,
    ...section.entries.map(({ id }) => id),
  ])
  expect(new Set(ids).size).toBe(ids.length)
})

it('merges overlapping highlight ranges', () => {
  expect(settingsMatchRanges('Hide values when Brief opens', 'val values')).toEqual([[5, 11]])
  expect(settingsMatchRanges('Home range', 'e')).toEqual([
    [3, 4],
    [9, 10],
  ])
  expect(settingsMatchRanges('Theme', '')).toEqual([])
})

it('maps former detail routes and anchors onto the single Settings page', () => {
  expect(settingsHashForPath('/settings/connections')).toBe('connections')
  expect(settingsHashForPath('/settings/accounts')).toBe('accounts')
  expect(settingsHashForPath('/settings/accounts#platinum-benefits')).toBe('platinum-benefits')
  expect(settingsHashForPath('/settings#data-sources')).toBe('connections')
  expect(settingsHashForPath('/settings')).toBeUndefined()
})

it('addresses Settings through the URL hash', () => {
  expect(settingsTarget('')).toBeUndefined()
  expect(settingsTarget('privacy')).toBeUndefined()
  expect(settingsTarget('#settings')).toBe('')
  expect(settingsTarget('settings/platinum-benefits')).toBe('platinum-benefits')
  expect(settingsHash()).toBe('settings')
  expect(settingsHash('privacy')).toBe('settings/privacy')
})
