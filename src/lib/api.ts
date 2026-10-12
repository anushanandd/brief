import { getVersion } from '@tauri-apps/api/app'
import { Channel, invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { z } from 'zod'

import seedInsights from '../data/seed-insights.json'
import seed from '../data/seed.json'
import { previewFinanceSettings } from './finance-settings'
import { ledgerCallSchema } from './ledger-view'
import {
  backupInfoSchema,
  calibrationSchema,
  earningsEventSchema,
  foundationModelStatusSchema,
  integrationStatusSchema,
  loginItemStateSchema,
  providerConnectionSchema,
  providerLinkSessionSchema,
  providerLinkStatusSchema,
  financeSnapshotSchema,
  healthReportSchema,
  investmentSummarySchema,
  portfolioNewsSchema,
  portfolioTodaySchema,
  sectorExposureSchema,
  statementPreviewSchema,
  statementSummarySchema,
  marketNewsResultSchema,
  marketSnapshotsSchema,
  summaryRunSchema,
  syncRunSchema,
  type Calibration,
  type FinanceSnapshot,
  type EarningsEvent,
  type HealthReport,
  type InvestmentSummary,
  type PortfolioToday,
  type SectorExposure,
  type StatementPreview,
  type StatementSummary,
  type MarketNewsResult,
  type MarketSnapshots,
  type SummaryRun,
  type SyncRun,
  type BackupInfo,
  type FoundationModelStatus,
  type IntegrationStatus,
  type LoginItemState,
  type ProviderConnection,
  type ProviderLinkSession,
  type ProviderLinkStatus,
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

export type CredentialProvider = 'plaid' | 'snaptrade' | 'alpaca' | 'alphavantage' | 'sec'

export async function getSyncRuns(): Promise<SyncRun[]> {
  if (!isTauri()) return []
  return syncRunSchema.array().parse(await invoke<unknown>('get_sync_runs'))
}

/** How often the layered model's thresholds fired over saved sessions. */
export async function getCalibration(): Promise<Calibration | null> {
  if (!isTauri()) return null
  return calibrationSchema.parse(await invoke<unknown>('get_calibration'))
}

/** This launch's investment summary runs, newest first. */
export async function getSummaryRuns(): Promise<SummaryRun[]> {
  if (!isTauri()) return []
  return summaryRunSchema.array().parse(await invoke<unknown>('get_summary_runs'))
}

// The preview has no native runtime; its insights were computed by Rust from the same seed.
const browserSnapshot = (): FinanceSnapshot => {
  const settings = previewFinanceSettings()
  const sameCard = settings.spendingAccountId === seedInsights.spendingAccountId
  return financeSnapshotSchema.parse({
    ...seed,
    settings,
    transactions: seed.transactions.map((transaction) => ({
      ...transaction,
      recurringId: (seedInsights.recurringIds as Record<string, string>)[transaction.id],
    })),
    insights: {
      ...seedInsights.insights,
      ...(sameCard ? {} : { benefits: undefined, statement: undefined }),
    },
  })
}

export async function getFinanceSnapshot(): Promise<FinanceSnapshot> {
  if (!isTauri()) return browserSnapshot()

  const result = await invoke<unknown>('get_finance_snapshot')
  return financeSnapshotSchema.parse(result)
}

export async function recoverFinanceState(restore: boolean): Promise<FinanceSnapshot> {
  return financeSnapshotSchema.parse(await invoke('recover_finance_state', { restore }))
}

export async function exportFinanceBackup(path: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('export_finance_backup', { path })
}

export async function inspectFinanceBackup(path: string): Promise<BackupInfo> {
  if (!isTauri()) throw new Error('Desktop app only')
  return backupInfoSchema.parse(await invoke('inspect_finance_backup', { path }))
}

export async function restoreFinanceBackup(path: string): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return financeSnapshotSchema.parse(await invoke('restore_finance_backup', { path }))
}

/** Reads and checks a statement file. Rust keeps the bytes until `importStatement`. */
export async function previewStatementImport(path: string): Promise<StatementPreview> {
  if (!isTauri()) throw new Error('Desktop app only')
  return statementPreviewSchema.parse(await invoke('preview_statement_import', { path }))
}

export async function importStatement(
  draftId: string,
  accountId: string,
): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return financeSnapshotSchema.parse(await invoke('import_statement', { draftId, accountId }))
}

export async function getStatementImports(): Promise<StatementSummary[]> {
  if (!isTauri()) return []
  return statementSummarySchema.array().parse(await invoke('get_statement_imports'))
}

export async function removeStatementImport(id: string): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return financeSnapshotSchema.parse(await invoke('remove_statement_import', { id }))
}

export async function eraseFinanceState(): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return financeSnapshotSchema.parse(await invoke('erase_finance_state'))
}

export async function exportSettingsFile(path: string, contents: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('export_settings_file', { path, contents })
}

export async function readSettingsFile(path: string): Promise<string> {
  if (!isTauri()) throw new Error('Desktop app only')
  return z.string().parse(await invoke('read_settings_file', { path }))
}

export async function getAppVersion(): Promise<string> {
  return isTauri() ? getVersion() : 'Browser preview'
}

export async function getDataHealth(): Promise<HealthReport> {
  if (!isTauri()) {
    return healthReportSchema.parse({
      computedAt: new Date().toISOString(),
      revision: browserSnapshot().revision ?? 0,
      overallStatus: 'info',
      counts: { critical: 0, error: 0, warning: 0, info: 1 },
      issues: [
        {
          id: 'browser-preview',
          severity: 'info',
          category: 'environment',
          title: 'Synthetic browser preview',
          explanation: 'Storage and provider checks run in the desktop app',
          evidence: [],
          affectedItems: [],
          action: { label: 'Open settings', route: '/settings' },
        },
      ],
    })
  }
  return healthReportSchema.parse(await invoke('get_data_health'))
}

/**
 * Ordinary refreshes use Plaid's cached balances. `liveBalances` requests real-time bank
 * balances, which Plaid bills per request.
 */
export async function refreshFinanceSnapshot({
  liveBalances = false,
}: { liveBalances?: boolean } = {}): Promise<{
  snapshot: FinanceSnapshot
  warnings: string[]
}> {
  if (!isTauri()) {
    return {
      snapshot: { ...browserSnapshot(), updatedAt: new Date().toISOString() },
      warnings: [],
    }
  }

  const snapshot = financeSnapshotSchema.parse(
    await invoke('refresh_finance_snapshot', { liveBalances }),
  )
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
  if (!isTauri()) throw new Error('Desktop app only')
  const session = providerLinkSessionSchema.parse(
    await invoke('begin_provider_link', { provider, itemId }),
  )
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
  return providerLinkStatusSchema.parse(
    await invoke('poll_provider_link', {
      browserCompleted,
      provider: session.provider,
      sessionId: session.sessionId,
    }),
  )
}

export async function getIntegrationStatus(): Promise<IntegrationStatus> {
  if (!isTauri())
    return { plaid: false, snaptrade: false, alpaca: false, alphaVantage: false, sec: false }
  return integrationStatusSchema.parse(await invoke('get_integration_status'))
}

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  return isTauri()
    ? providerConnectionSchema.array().parse(await invoke('get_provider_connections'))
    : []
}
/** Asks SnapTrade, so it can be slow; `getProviderConnections` has the saved list. */
export async function getSnapTradeConnections(): Promise<ProviderConnection[]> {
  return isTauri()
    ? providerConnectionSchema.array().parse(await invoke('get_snaptrade_connections'))
    : []
}
export async function forgetProviderConnection(itemId: string): Promise<void> {
  await invoke('forget_provider_connection', { itemId })
}

export async function authenticateSensitiveAction(): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('authenticate_sensitive_action')
}

export async function removeIntegrationCredentials(
  provider: CredentialProvider,
): Promise<IntegrationStatus> {
  if (!isTauri()) throw new Error('Desktop app only')
  return integrationStatusSchema.parse(await invoke('remove_integration_credentials', { provider }))
}

export async function saveIntegrationCredentials(
  provider: CredentialProvider,
  credentials: { clientId: string; secret?: string; consumerKey?: string },
): Promise<IntegrationStatus> {
  if (!isTauri()) throw new Error('Desktop app only')
  return integrationStatusSchema.parse(
    await invoke('save_integration_credentials', {
      provider,
      clientId: credentials.clientId,
      secret: credentials.secret,
      consumerKey: credentials.consumerKey,
    }),
  )
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

const emptyNews: MarketNewsResult = {
  articles: [],
  savedAt: null,
  warning: null,
  canRefresh: false,
  company: null,
  filings: [],
  filingsSavedAt: null,
  filingsWarning: null,
}

export async function getMarketNews(symbols: string[], refresh = false): Promise<MarketNewsResult> {
  if (!isTauri()) return emptyNews
  return marketNewsResultSchema.parse(await invoke('get_market_news', { symbols, refresh }))
}

/** The portfolio's move today at live prices, with each position's dollar effect. */
export async function getPortfolioToday(): Promise<PortfolioToday> {
  if (!isTauri()) throw new Error('Desktop app only')
  return portfolioTodaySchema.parse(await invoke('get_portfolio_today'))
}

/** Each holding's market gain since `start` (a returns window's first day). */

/**
 * The portfolio's, or one holding's, period since `start`: moves against benchmarks, your
 * activity, ranked company events, the latest reported quarter, and any saved context.
 */
export async function getInvestmentSummary(
  start: string,
  ticker?: string,
): Promise<InvestmentSummary> {
  // The preview shows the portfolio's summary Rust made from the synthetic seed.
  if (!isTauri() && !ticker) return investmentSummarySchema.parse(seedInsights.summary)
  if (!isTauri()) throw new Error('Desktop app only')
  return investmentSummarySchema.parse(
    await invoke('get_investment_summary', { start, ticker: ticker ?? null }),
  )
}

/**
 * Saves the past week's news for every held stock in one request and checks EDGAR for the
 * stalest holdings, so summaries and the holdings list are ready before they're opened. Runs at
 * most every ten minutes; `refreshed` is false when it was skipped.
 */
export async function refreshPortfolioNews(): Promise<z.infer<typeof portfolioNewsSchema>> {
  if (!isTauri()) throw new Error('Desktop app only')
  return portfolioNewsSchema.parse(await invoke('refresh_portfolio_news'))
}

/** Held stocks by sector; looks up unclassified stocks in EDGAR once. */
export async function getPortfolioSectors(): Promise<SectorExposure> {
  if (!isTauri()) throw new Error('Desktop app only')
  return sectorExposureSchema.parse(await invoke('get_portfolio_sectors'))
}

export async function getEarningsCalendar(symbols: string[]): Promise<EarningsEvent[]> {
  if (!isTauri()) return []
  return earningsEventSchema.array().parse(await invoke('get_earnings_calendar', { symbols }))
}

export async function getFoundationModelStatus(): Promise<FoundationModelStatus> {
  if (!isTauri()) {
    return { state: 'unavailable', message: 'Apple Intelligence requires the desktop app' }
  }
  return foundationModelStatusSchema.parse(await invoke('get_foundation_model_status'))
}

const foundationAnswerSchema = z.object({
  text: z.string(),
  evidence: z.array(ledgerCallSchema),
})
/** An answer and the native ledger queries whose figures it may cite. */
export type FoundationAnswer = z.infer<typeof foundationAnswerSchema>

/** Reruns one read-only ledger query over the saved snapshot, without the language model. */
export async function ledgerQuery(name: string, args: Record<string, unknown>) {
  if (!isTauri()) throw new Error('Saved queries need the Brief desktop app')
  return z
    .record(z.string(), z.unknown())
    .parse(await invoke('ledger_query', { name, arguments: args }))
}

let explanationQueue: Promise<unknown> = Promise.resolve()
/**
 * Asks Apple's on-device model a question. The model looks up figures only through native,
 * read-only ledger tools; it never receives the snapshot itself or computes amounts.
 */
export async function generateFoundationExplanation(
  question: string,
  signal: AbortSignal | undefined,
): Promise<FoundationAnswer> {
  if (!isTauri()) throw new Error('Apple Intelligence requires the desktop app')
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
      const response = await invoke('generate_foundation_explanation', {
        question: question.slice(0, 1_000),
        requestId,
        started,
      })
      // An abandoned answer is discarded before it is read.
      signal?.throwIfAborted()
      return foundationAnswerSchema.parse(response)
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

export async function getLoginItem(): Promise<LoginItemState> {
  return isTauri() ? loginItemStateSchema.parse(await invoke('get_login_item')) : 'unavailable'
}

export async function setLoginItem(enabled: boolean): Promise<LoginItemState> {
  if (!isTauri()) throw new Error('Desktop app only')
  return loginItemStateSchema.parse(await invoke('set_login_item', { enabled }))
}

export async function revealMainWindow() {
  if (isTauri()) await invoke('reveal_main_window')
}
