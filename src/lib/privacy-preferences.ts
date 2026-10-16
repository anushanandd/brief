import { useMemo } from 'react'

import { preference } from './preference'

export type PrivacyPreferences = {
  hideOnLaunch: boolean
  hideOnBlur: boolean
}

const flag = (raw: string | null) => raw === 'true'
const hideOnLaunch = preference('brief:hide-values-on-launch', flag)
const hideOnBlur = preference('brief:hide-values-on-blur', flag)

export function getPrivacyPreferences(): PrivacyPreferences {
  return { hideOnLaunch: hideOnLaunch.get(), hideOnBlur: hideOnBlur.get() }
}

export function savePrivacyPreferences(preferences: PrivacyPreferences) {
  hideOnLaunch.save(preferences.hideOnLaunch)
  hideOnBlur.save(preferences.hideOnBlur)
}

export function usePrivacyPreferences(): PrivacyPreferences {
  const launch = hideOnLaunch.use()
  const blur = hideOnBlur.use()
  return useMemo(() => ({ hideOnLaunch: launch, hideOnBlur: blur }), [blur, launch])
}

export const privacyPreferenceKeys = [hideOnLaunch.key, hideOnBlur.key] as const
