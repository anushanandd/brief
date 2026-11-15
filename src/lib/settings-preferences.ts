import { rendererStorage } from './preference'
import { privacyPreferenceKeys } from './privacy-preferences'

export const navigationPreferenceKeys = [
  'brief.pageOrder',
  'brief.numberKeys',
  'brief.activitySavedViews',
] as const

export const dataPreferenceKeys = ['brief.externalLogosEnabled', ...privacyPreferenceKeys] as const

export function clearSettingsPreferences(keys: readonly string[]) {
  for (const key of keys) rendererStorage.remove(key)
  window.location.reload()
}
