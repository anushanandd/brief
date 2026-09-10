import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import seed from '../data/seed.json'
import {
  financeSnapshotSchema,
  marketNewsSchema,
  marketSnapshotsSchema,
  type FinanceSnapshot,
  type MarketNewsArticle,
  type MarketSnapshots,
} from './schema'

export const isTauri = () => '__TAURI_INTERNALS__' in window

export function safeExternalUrl(url: string): string | undefined {
  try {
    const parsed = new URL(url.includes('://') ? url : `https://${url}`)
    return parsed.protocol === 'https:' ? parsed.toString() : undefined
  } catch {
    return undefined
  }
}

export async function openExternalUrl(url: string): Promise<void> {
  const safeUrl = safeExternalUrl(url)
  if (!safeUrl) throw new Error('Brief only opens secure external links')
  if (isTauri()) {
    await openUrl(safeUrl)
    return
  }
  window.open(safeUrl, '_blank', 'noopener,noreferrer')
}

export type ProviderLinkSession = {
  provider: 'plaid' | 'plaid-investments' | 'snaptrade'
  sessionId: string
  url: string
}

export type ProviderLinkStatus = {
  status: 'pending' | 'connected'
}

export type IntegrationStatus = {
  plaid: boolean
  snaptrade: boolean
  alpaca: boolean
}

export type SyncRun = {
  id: string
  startedAt: string
  finishedAt: string
  outcome: 'committed' | 'failed'
  warnings: string[]
  errorCode?: string
}

export async function getSyncRuns(): Promise<SyncRun[]> {
  return isTauri() ? invoke<SyncRun[]>('get_sync_runs') : []
}

const browserSnapshot = (): FinanceSnapshot => financeSnapshotSchema.parse(seed)

export async function getFinanceSnapshot(): Promise<FinanceSnapshot> {
  if (!isTauri()) return browserSnapshot()

  const result = await invoke<unknown>('get_finance_snapshot')
  return financeSnapshotSchema.parse(result)
}

export async function recoverFinanceState(restore: boolean): Promise<FinanceSnapshot> {
  return financeSnapshotSchema.parse(await invoke('recover_finance_state', { restore }))
}

export async function refreshFinanceSnapshot(): Promise<{
  snapshot: FinanceSnapshot
  warnings: string[]
}> {
  if (!isTauri()) {
    return {
      snapshot: { ...browserSnapshot(), updatedAt: new Date().toISOString() },
      warnings: [],
    }
  }

  const snapshot = financeSnapshotSchema.parse(await invoke('refresh_finance_snapshot'))
  return { snapshot, warnings: snapshot.syncWarnings ?? [] }
}

export async function saveAccountLink(
  plaidAccountId: string,
  snaptradeAccountId?: string,
): Promise<void> {
  if (!isTauri()) return
  await invoke('save_account_link', { plaidAccountId, snaptradeAccountId })
}

export async function beginProviderLink(
  provider: 'plaid' | 'plaid-investments' | 'snaptrade',
  itemId?: string,
): Promise<ProviderLinkSession> {
  if (!isTauri()) throw new Error('Provider linking is available in the Brief desktop app')
  const session = await invoke<ProviderLinkSession>('begin_provider_link', { provider, itemId })
  try {
    await openExternalUrl(session.url)
  } catch (error) {
    await cancelProviderLink(session).catch(() => undefined)
    throw error
  }
  return session
}

export async function cancelProviderLink(session: ProviderLinkSession): Promise<void> {
  await invoke('cancel_provider_link', { sessionId: session.sessionId })
}

export async function pollProviderLink(session: ProviderLinkSession): Promise<ProviderLinkStatus> {
  return invoke<ProviderLinkStatus>('poll_provider_link', {
    provider: session.provider,
    sessionId: session.sessionId,
  })
}

export async function getIntegrationStatus(): Promise<IntegrationStatus> {
  if (!isTauri()) return { plaid: false, snaptrade: false, alpaca: false }
  return invoke<IntegrationStatus>('get_integration_status')
}

export type ProviderConnection = {
  itemId: string
  name: string
  provider: 'plaid' | 'plaid-investments'
  error: string | null
}
export async function getProviderConnections(): Promise<ProviderConnection[]> {
  return isTauri() ? invoke('get_provider_connections') : []
}
export async function forgetProviderConnection(itemId: string): Promise<void> {
  await invoke('forget_provider_connection', { itemId })
}

export async function authenticateSensitiveAction(): Promise<void> {
  if (!isTauri()) throw new Error('System authentication is available in the Brief desktop app')
  await invoke('authenticate_sensitive_action')
}

export async function saveIntegrationCredentials(
  provider: 'plaid' | 'snaptrade' | 'alpaca',
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

export async function getMarketSnapshots(symbols: string[]): Promise<MarketSnapshots> {
  if (!isTauri()) {
    return {
      snapshots: {},
      session: 'Market closed',
      feed: 'iex',
      delayMinutes: 0,
      asOf: null,
      nextTransitionAt: null,
      pollIntervalMs: null,
    }
  }
  return marketSnapshotsSchema.parse(await invoke('get_market_snapshots', { symbols }))
}

export async function getMarketNews(symbol: string): Promise<MarketNewsArticle[]> {
  if (!isTauri()) return []
  return marketNewsSchema.parse(await invoke('get_market_news', { symbol }))
}

export type FoundationModelStatus = {
  state: 'available' | 'unavailable' | 'disabled' | 'notReady'
  message: string
}

export async function getFoundationModelStatus(): Promise<FoundationModelStatus> {
  if (!isTauri()) {
    return { state: 'unavailable', message: 'Apple Intelligence requires the Brief desktop app' }
  }
  return invoke<FoundationModelStatus>('get_foundation_model_status')
}

let explanationQueue: Promise<unknown> = Promise.resolve()
export async function generateFoundationExplanation(
  evidence: string,
  signal?: AbortSignal,
): Promise<string> {
  if (!isTauri()) throw new Error('Apple Intelligence requires the Brief desktop app')
  const result = explanationQueue.then(() => {
    signal?.throwIfAborted()
    return invoke<string>('generate_foundation_explanation', {
      evidence: evidence.slice(0, 12_000),
    })
  })
  explanationQueue = result.catch(() => undefined)
  return result
}
