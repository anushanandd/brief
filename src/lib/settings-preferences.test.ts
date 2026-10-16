// @vitest-environment jsdom

import { beforeEach, expect, it, vi } from 'vitest'

import {
  exportSettingsPreferences,
  importSettingsPreferences,
  parseSettingsPreferences,
} from './settings-preferences'

beforeEach(() => {
  const values = new Map<string, string>()
  const storage = {
    clear: () => values.clear(),
    getItem: (key: string) => values.get(key) ?? null,
    removeItem: (key: string) => values.delete(key),
    setItem: (key: string, value: string) => values.set(key, value),
  }
  vi.stubGlobal('localStorage', storage)
  Object.defineProperty(window, 'localStorage', { configurable: true, value: storage })
})

it('exports and imports only bounded known preferences', () => {
  localStorage.setItem('brief:theme', 'dark')
  localStorage.setItem('brief.accountDisplayNames', '{"account":"Primary"}')
  localStorage.setItem('unrelated', 'keep')
  const exported = exportSettingsPreferences()
  // Account settings belong to the finance database and its backups, not preference files.
  expect(exported.preferences).toEqual({ 'brief:theme': 'dark' })

  const parsed = parseSettingsPreferences({
    ...exported,
    preferences: { ...exported.preferences, unknown: 'ignored' },
  })
  localStorage.setItem('brief:theme', 'custom')
  importSettingsPreferences(parsed)
  expect(localStorage.getItem('brief:theme')).toBe('dark')
  expect(localStorage.getItem('unknown')).toBeNull()
  expect(localStorage.getItem('unrelated')).toBe('keep')
})

it('accepts account settings from older preference files for the native import', () => {
  const parsed = parseSettingsPreferences({
    version: 1,
    exportedAt: '2026-09-01T00:00:00Z',
    preferences: { 'brief.accountDisplayNames': '{"account":"Primary"}' },
  })
  importSettingsPreferences(parsed)
  expect(localStorage.getItem('brief.accountDisplayNames')).toBe('{"account":"Primary"}')
})

it('rejects unsupported preference files', () => {
  expect(() => parseSettingsPreferences({ version: 2, exportedAt: '', preferences: {} })).toThrow(
    'not supported',
  )
  expect(() => parseSettingsPreferences({ version: 1, exportedAt: '', preferences: [] })).toThrow(
    'invalid',
  )
})
