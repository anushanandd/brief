import { homePreferenceKeys } from './home-preferences'
import { privacyPreferenceKeys } from './privacy-preferences'

export const themePreferenceKeys = ['brief:theme', ...homePreferenceKeys] as const
export const chartPreferenceKeys = ['brief.defaultGraphWindow'] as const
export const navigationPreferenceKeys = ['brief.pageOrder', 'brief.activitySavedViews'] as const

export const appearancePreferenceKeys = [
  ...themePreferenceKeys,
  ...navigationPreferenceKeys,
  ...chartPreferenceKeys,
] as const

// Account names, start dates, the spending account, and benefit choices now live in the
// finance database and its backups. Older preference files may still carry them.
const legacyAccountPreferenceKeys = [
  'brief.accountDisplayNames',
  'brief.accountStartDates',
  'brief:spending-account-id',
  'brief:hidden-platinum-benefits',
] as const

export const dataPreferenceKeys = ['brief.externalLogosEnabled', ...privacyPreferenceKeys] as const
export const advancedPreferenceKeys = ['brief.marketUpdateIntervalSeconds'] as const

export const settingsPreferenceKeys = [
  ...appearancePreferenceKeys,
  ...dataPreferenceKeys,
  ...advancedPreferenceKeys,
] as const

// Legacy account keys are accepted on import; the next launch moves them into the native store.
const allowedKeys = new Set<string>([...settingsPreferenceKeys, ...legacyAccountPreferenceKeys])

export type SettingsPreferencesFile = {
  version: 1
  exportedAt: string
  preferences: Record<string, string>
}

export function exportSettingsPreferences(): SettingsPreferencesFile {
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
    preferences: Object.fromEntries(
      settingsPreferenceKeys.flatMap((key) => {
        const value = window.localStorage.getItem(key)
        return value == null ? [] : [[key, value]]
      }),
    ),
  }
}

export function parseSettingsPreferences(value: unknown): SettingsPreferencesFile {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error('This is not a Brief preferences file')
  if (
    !('version' in value) ||
    value.version !== 1 ||
    !('exportedAt' in value) ||
    typeof value.exportedAt !== 'string'
  )
    throw new Error('This preferences file is not supported')
  if (
    !('preferences' in value) ||
    !value.preferences ||
    typeof value.preferences !== 'object' ||
    Array.isArray(value.preferences)
  )
    throw new Error('This preferences file is invalid')
  const preferences = Object.fromEntries(
    Object.entries(value.preferences).flatMap(([key, preference]) =>
      allowedKeys.has(key) && typeof preference === 'string' && preference.length <= 100_000
        ? [[key, preference]]
        : [],
    ),
  )
  return { version: 1, exportedAt: value.exportedAt, preferences }
}

export function importSettingsPreferences(file: SettingsPreferencesFile) {
  for (const key of allowedKeys) window.localStorage.removeItem(key)
  for (const [key, value] of Object.entries(file.preferences))
    window.localStorage.setItem(key, value)
}

export function clearSettingsPreferences(keys: readonly string[]) {
  for (const key of keys) window.localStorage.removeItem(key)
  window.location.reload()
}
