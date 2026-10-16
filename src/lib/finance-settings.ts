import { invoke } from '@tauri-apps/api/core'

import { parseAccountDisplayNames } from './account-names'
import { parseAccountStartDates } from './account-start-dates'
import { getFinanceSnapshot, isTauri } from './api'
import {
  financeSettingsSchema,
  financeSnapshotSchema,
  type FinanceSettings,
  type FinanceSnapshot,
} from './schema'

/** Renderer keys that older builds used before these settings moved into the finance store. */
export const legacyFinanceSettingKeys = {
  spendingAccountId: 'brief:spending-account-id',
  hiddenBenefitIds: 'brief:hidden-platinum-benefits',
  accountNames: 'brief.accountDisplayNames',
  accountStartDates: 'brief.accountStartDates',
} as const

// The synthetic browser preview has no native store; it keeps its settings locally.
const previewSettingsKey = 'brief.preview.financeSettings'

export const defaultFinanceSettings: FinanceSettings = financeSettingsSchema.parse({})

export function financeSettings(data: Pick<FinanceSnapshot, 'settings'> | undefined) {
  return data?.settings ?? defaultFinanceSettings
}

/** Every card the user spends from: the saved spending card, then the others. */
export function spendingAccountIds(settings: FinanceSettings) {
  return [
    ...(settings.spendingAccountId ? [settings.spendingAccountId] : []),
    ...(settings.spendingCards ?? []),
  ]
}

function readLegacySettings(): Partial<FinanceSettings> | undefined {
  const raw = Object.fromEntries(
    Object.entries(legacyFinanceSettingKeys).map(([field, key]) => [
      field,
      window.localStorage.getItem(key),
    ]),
  )
  if (Object.values(raw).every((value) => value == null)) return undefined
  return {
    ...(raw.spendingAccountId ? { spendingAccountId: raw.spendingAccountId } : {}),
    hiddenBenefitIds: raw.hiddenBenefitIds?.split(',').filter(Boolean) ?? [],
    accountNames: parseAccountDisplayNames(raw.accountNames),
    accountStartDates: parseAccountStartDates(raw.accountStartDates),
  }
}

function removeLegacySettings() {
  for (const key of Object.values(legacyFinanceSettingKeys)) window.localStorage.removeItem(key)
}

export function previewFinanceSettings(): FinanceSettings {
  try {
    return financeSettingsSchema.parse(
      JSON.parse(window.localStorage.getItem(previewSettingsKey) ?? '{}'),
    )
  } catch {
    return defaultFinanceSettings
  }
}

/**
 * Moves settings saved by older renderer builds into the native store once. Existing native
 * choices win; legacy keys are removed only after the native store accepted them.
 */
export async function importLegacyFinanceSettings(): Promise<string | undefined> {
  let legacy: Partial<FinanceSettings> | undefined
  try {
    legacy = readLegacySettings()
  } catch {
    return 'Couldn’t read account preferences; originals kept'
  }
  if (!legacy) return undefined
  const imported = financeSettingsSchema.parse(legacy)
  if (!isTauri()) {
    const current = previewFinanceSettings()
    window.localStorage.setItem(
      previewSettingsKey,
      JSON.stringify({
        ...imported,
        ...current,
        accountNames: { ...imported.accountNames, ...current.accountNames },
        accountStartDates: { ...imported.accountStartDates, ...current.accountStartDates },
      }),
    )
    removeLegacySettings()
    return undefined
  }
  try {
    await invoke('save_finance_settings', { settings: imported, importing: true })
    removeLegacySettings()
    return undefined
  } catch {
    return 'Couldn’t import account preferences; originals kept'
  }
}

export async function saveFinanceSettings(settings: FinanceSettings): Promise<FinanceSnapshot> {
  const parsed = financeSettingsSchema.parse(settings)
  if (!isTauri()) {
    window.localStorage.setItem(previewSettingsKey, JSON.stringify(parsed))
    return getFinanceSnapshot()
  }
  return financeSnapshotSchema.parse(
    await invoke('save_finance_settings', { settings: parsed, importing: false }),
  )
}
