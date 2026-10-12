import { preference } from './preference'

export type AccountGroupsMode = 'collapsed' | 'expanded'

export const accountGroupsModes = [
  { value: 'collapsed', label: 'Collapsed', accessibleLabel: 'Collapsed' },
  { value: 'expanded', label: 'Expanded', accessibleLabel: 'Expanded' },
] as const

export const parseAccountGroupsMode = (value: unknown): AccountGroupsMode =>
  value === 'expanded' ? 'expanded' : 'collapsed'

const accountGroups = preference('brief.homeAccountGroups', parseAccountGroupsMode)

export const getAccountGroupsMode = accountGroups.get
export const saveAccountGroupsMode = accountGroups.save
export const useAccountGroupsMode = accountGroups.use

export const homePreferenceKeys = [accountGroups.key] as const
