import { describe, expect, it } from 'vitest'

import {
  browserHistoryDirection,
  listNavigationAction,
  navigationShortcutIndex,
  sensitiveValuesShortcut,
  shortcutHelpShortcut,
  sidebarShortcutDirection,
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
  it('uses bare numbers for pages without stealing typing', () => {
    expect(navigationShortcutIndex(key(), false)).toBe(0)
    expect(navigationShortcutIndex(key({ key: '7' }), false)).toBe(6)
    expect(navigationShortcutIndex(key(), true)).toBeUndefined()
    expect(navigationShortcutIndex(key({ key: '1', metaKey: true }), false)).toBeUndefined()
    expect(navigationShortcutIndex(key({ key: '9' }), false)).toBeUndefined()
  })

  it('maps list navigation and shortcut help', () => {
    expect(listNavigationAction(key({ key: 'j' }), false)).toBe('next')
    expect(listNavigationAction(key({ key: 'k' }), false)).toBe('previous')
    expect(listNavigationAction(key({ key: 'Enter' }), false)).toBe('open')
    expect(listNavigationAction(key({ key: 'j' }), true)).toBeUndefined()
    expect(shortcutHelpShortcut(key({ key: '?', shiftKey: true }), false)).toBe(true)
    expect(shortcutHelpShortcut(key({ key: '?', shiftKey: true }), true)).toBe(false)
  })

  it('ignores removed motion keys', () => {
    expect(listNavigationAction(key({ key: 'h' }), false)).toBeUndefined()
    expect(listNavigationAction(key({ key: 'l' }), false)).toBeUndefined()
    expect(listNavigationAction(key({ key: 'g' }), false)).toBeUndefined()
    expect(listNavigationAction(key({ key: 'G', shiftKey: true }), false)).toBeUndefined()
  })
})

it('cycles sidebar pages only for unmodified Command-Up/Down outside editing', () => {
  expect(sidebarShortcutDirection(key({ key: 'ArrowUp', metaKey: true }), false)).toBe(-1)
  expect(sidebarShortcutDirection(key({ key: 'ArrowDown', metaKey: true }), false)).toBe(1)
  expect(sidebarShortcutDirection(key({ key: 'ArrowUp' }), false)).toBeUndefined()
  expect(sidebarShortcutDirection(key({ key: 'ArrowUp', metaKey: true }), true)).toBeUndefined()
  expect(
    sidebarShortcutDirection(key({ key: 'ArrowUp', metaKey: true, ctrlKey: true }), false),
  ).toBeUndefined()
})

it('toggles value masking only for an initial Command-H press', () => {
  expect(sensitiveValuesShortcut({ ...key({ key: 'h', metaKey: true }), repeat: false })).toBe(true)
  expect(sensitiveValuesShortcut({ ...key({ key: 'H', metaKey: true }), repeat: false })).toBe(true)
  expect(sensitiveValuesShortcut({ ...key({ key: 'h', metaKey: true }), repeat: true })).toBe(false)
  expect(sensitiveValuesShortcut({ ...key({ key: 'h' }), repeat: false })).toBe(false)
  expect(
    sensitiveValuesShortcut({ ...key({ key: 'h', metaKey: true, shiftKey: true }), repeat: false }),
  ).toBe(false)
})
