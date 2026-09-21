import { Channel, invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import seed from '../data/seed.json'
import {
  earningsEventSchema,
  financeSnapshotSchema,
  marketNewsResultSchema,
  marketSnapshotsSchema,
  type FinanceSnapshot,
  type EarningsEvent,
  type MarketNewsResult,
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
  alphaVantage: boolean
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

export async function pollProviderLink(
  session: ProviderLinkSession,
  browserCompleted = false,
): Promise<ProviderLinkStatus> {
  return invoke<ProviderLinkStatus>('poll_provider_link', {
    browserCompleted,
    provider: session.provider,
    sessionId: session.sessionId,
  })
}

export async function getIntegrationStatus(): Promise<IntegrationStatus> {
  if (!isTauri()) return { plaid: false, snaptrade: false, alpaca: false, alphaVantage: false }
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
  provider: 'plaid' | 'snaptrade' | 'alpaca' | 'alphavantage',
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

export async function startMarketStream(
  symbols: string[],
  updateIntervalSeconds: number,
  requestId: string,
  onUpdate: (market: MarketSnapshots) => void,
  onError: (message: string) => void,
): Promise<void> {
  const updates = new Channel<unknown>((payload) => {
    if (!payload || typeof payload !== 'object') return
    if ('kind' in payload && payload.kind === 'update' && 'market' in payload) {
      const parsed = marketSnapshotsSchema.safeParse(payload.market)
      if (parsed.success) onUpdate(parsed.data)
      else onError('Invalid market update received')
    } else if (
      'kind' in payload &&
      payload.kind === 'error' &&
      'message' in payload &&
      typeof payload.message === 'string'
    ) {
      onError(payload.message)
    }
  })
  await invoke('start_market_stream', {
    symbols,
    updateIntervalMs: updateIntervalSeconds * 1_000,
    requestId,
    updates,
  })
}

export async function stopMarketStream(requestId: string): Promise<void> {
  await invoke('stop_market_stream', { requestId })
}

export async function startHoldingChart(
  symbol: string,
  range: number,
  requestId: string,
  warmSymbols: string[] = [],
) {
  if (!isTauri()) return
  await invoke('start_holding_chart', { symbol, range, requestId, warmSymbols })
}

export async function stopHoldingChart(requestId: string) {
  if (!isTauri()) return
  await invoke('stop_holding_chart', { requestId })
}

export async function getMarketNews(symbols: string[], force = false): Promise<MarketNewsResult> {
  if (!isTauri())
    return { articles: [], savedAt: null, warning: null, requestsRemaining: 0, canRefresh: false }
  return marketNewsResultSchema.parse(await invoke('get_market_news', { symbols, force }))
}

export async function getEarningsCalendar(symbols: string[]): Promise<EarningsEvent[]> {
  if (!isTauri()) return []
  return earningsEventSchema.array().parse(await invoke('get_earnings_calendar', { symbols }))
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
  signal: AbortSignal | undefined,
  purpose: 'news' | 'chat',
): Promise<string> {
  if (!isTauri()) throw new Error('Apple Intelligence requires the Brief desktop app')
  const result = explanationQueue.then(async () => {
    signal?.throwIfAborted()
    const requestId = crypto.randomUUID()
    let registered = false
    const cancel = () => {
      if (registered)
        void invoke('cancel_foundation_explanation', { requestId }).catch(() => undefined)
    }
    const started = new Channel<null>(() => {
      registered = true
      if (signal?.aborted) cancel()
    })
    signal?.addEventListener('abort', cancel, { once: true })
    try {
      const response = await invoke<string>('generate_foundation_explanation', {
        evidence: evidence.slice(0, 12_000),
        purpose,
        requestId,
        started,
      })
      signal?.throwIfAborted()
      return response
    } finally {
      signal?.removeEventListener('abort', cancel)
    }
  })
  explanationQueue = result.catch(() => undefined)
  return result
}

export async function getSavedMarket() {
  if (!isTauri()) return null
  return marketSnapshotsSchema.nullable().parse(await invoke('get_saved_market'))
}

export async function revealMainWindow() {
  if (isTauri()) await invoke('reveal_main_window')
}
