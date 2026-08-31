import { invoke } from '@tauri-apps/api/core'

import seed from '../data/seed.json'
import { financeSnapshotSchema, type FinanceSnapshot } from './schema'

const isTauri = () => '__TAURI_INTERNALS__' in window

const browserSnapshot = (): FinanceSnapshot => financeSnapshotSchema.parse(seed)

export async function getFinanceSnapshot(): Promise<FinanceSnapshot> {
  if (!isTauri()) return browserSnapshot()

  const result = await invoke<unknown>('get_finance_snapshot')
  return financeSnapshotSchema.parse(result)
}

export async function refreshFinanceSnapshot(): Promise<FinanceSnapshot> {
  if (!isTauri()) {
    return {
      ...browserSnapshot(),
      updatedAt: new Date().toISOString(),
    }
  }

  const result = await invoke<unknown>('refresh_finance_snapshot')
  return financeSnapshotSchema.parse(result)
}
