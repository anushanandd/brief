import { describe, expect, it } from 'vitest'

import { navigation } from '../lib/navigation'
import {
  browserHistoryDirection,
  listNavigationAction,
  navigationShortcutIndex,
  shortcutHelpShortcut,
} from './app-shell'

const key = (overrides: Partial<Parameters<typeof navigationShortcutIndex>[0]> = {}) => ({
  altKey: false,
  ctrlKey: false,
  key: '1',
  metaKey: false,
  shiftKey: false,
  ...overrides,
})

describe('browser history shortcuts', () => {
  it.each([
    [{ metaKey: true, key: '[' }, 'back'],
    [{ metaKey: true, key: ']' }, 'forward'],
    [{ metaKey: false, key: '[' }, undefined],
  ])('maps %o to %s', (event, direction) => {
    expect(browserHistoryDirection(event)).toBe(direction)
  })
})

describe('navigation shortcuts', () => {
  it('places Spending directly after Accounts', () => {
    expect(navigation.map(({ label, shortcut }) => [label, shortcut])).toEqual([
      ['Home', '1'],
      ['Accounts', '2'],
      ['Spending', '3'],
      ['Holdings', '4'],
      ['Activity', '5'],
      ['Settings', '6'],
    ])
  })

  it('uses bare number keys without stealing typing or modified shortcuts', () => {
    expect(navigationShortcutIndex(key(), false)).toBe(0)
    expect(navigationShortcutIndex(key({ key: '6' }), false)).toBe(5)
    expect(navigationShortcutIndex(key({ metaKey: true }), false)).toBeUndefined()
    expect(navigationShortcutIndex(key(), true)).toBeUndefined()
    expect(navigationShortcutIndex(key({ key: '7' }), false)).toBeUndefined()
  })

  it('maps list navigation and shortcut help', () => {
    expect(listNavigationAction(key({ key: 'j' }), false)).toBe('next')
    expect(listNavigationAction(key({ key: 'k' }), false)).toBe('previous')
    expect(listNavigationAction(key({ key: 'Enter' }), false)).toBe('open')
    expect(listNavigationAction(key({ key: 'j' }), true)).toBeUndefined()
    expect(shortcutHelpShortcut(key({ key: '?', shiftKey: true }), false)).toBe(true)
    expect(shortcutHelpShortcut(key({ key: '?', shiftKey: true }), true)).toBe(false)
  })
})
