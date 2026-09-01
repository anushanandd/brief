import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import seed from '../data/seed.json'
import { normalizeSnapshot, type ProviderSyncPayload } from './normalize'
import { financeSnapshotSchema, type FinanceSnapshot } from './schema'

export const isTauri = () => '__TAURI_INTERNALS__' in window

function secureExternalUrl(url: string): string {
  const parsed = new URL(url)
  if (parsed.protocol !== 'https:') throw new Error('Brief only opens secure external links')
  return parsed.toString()
}

export async function openExternalUrl(url: string): Promise<void> {
  const safeUrl = secureExternalUrl(url)
  if (isTauri()) {
    await openUrl(safeUrl)
    return
  }
  window.open(safeUrl, '_blank', 'noopener,noreferrer')
}

export type ProviderLinkSession = {
  provider: 'plaid' | 'snaptrade'
  sessionId: string
  url: string
}

export type ProviderLinkStatus = {
  status: 'pending' | 'connected'
  accounts: string[]
}

export type IntegrationStatus = {
  plaid: boolean
  snaptrade: boolean
}

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

  const data = await invoke<ProviderSyncPayload>('refresh_provider_data')
  const snapshot = financeSnapshotSchema.parse(
    normalizeSnapshot(
      data.plaid,
      data.snaptrade,
      data.netWorthHistory,
      new Date(),
      data.netWorthHistoryEstimated,
      data.benchmarkHistory,
    ),
  )
  await invoke('save_finance_snapshot', { snapshot })
  return snapshot
}

export async function beginProviderLink(
  provider: 'plaid' | 'snaptrade',
): Promise<ProviderLinkSession> {
  if (!isTauri()) throw new Error('Provider linking is available in the Brief desktop app')
  const session = await invoke<ProviderLinkSession>('begin_provider_link', { provider })
  await openUrl(secureExternalUrl(session.url))
  return session
}

export async function pollProviderLink(session: ProviderLinkSession): Promise<ProviderLinkStatus> {
  return invoke<ProviderLinkStatus>('poll_provider_link', {
    provider: session.provider,
    sessionId: session.sessionId,
  })
}

export async function getIntegrationStatus(): Promise<IntegrationStatus> {
  if (!isTauri()) return { plaid: false, snaptrade: false }
  return invoke<IntegrationStatus>('get_integration_status')
}

export async function authenticateSensitiveAction(): Promise<void> {
  if (!isTauri()) throw new Error('System authentication is available in the Brief desktop app')
  await invoke('authenticate_sensitive_action')
}

export async function saveIntegrationCredentials(
  provider: 'plaid' | 'snaptrade',
  credentials: { clientId: string; secret?: string; consumerKey?: string },
): Promise<IntegrationStatus> {
  if (!isTauri()) throw new Error('Integration settings are available in the Brief desktop app')
  return invoke<IntegrationStatus>('save_integration_credentials', {
    provider,
    clientId: credentials.clientId,
    secret: credentials.secret,
    consumerKey: credentials.consumerKey,
  })
}
