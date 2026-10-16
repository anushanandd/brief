// @vitest-environment jsdom

import { beforeEach, expect, it, vi } from 'vitest'

import { getPrivacyPreferences, savePrivacyPreferences } from './privacy-preferences'

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

it('defaults to visible values and persists explicit privacy choices', () => {
  expect(getPrivacyPreferences()).toEqual({ hideOnLaunch: false, hideOnBlur: false })
  savePrivacyPreferences({ hideOnLaunch: true, hideOnBlur: true })
  expect(getPrivacyPreferences()).toEqual({ hideOnLaunch: true, hideOnBlur: true })
})
