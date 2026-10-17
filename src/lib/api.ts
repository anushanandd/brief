import { getVersion } from '@tauri-apps/api/app'
import { Channel, invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

import seedView from '../data/seed-view.json'
import type { AssistantAnswer } from './bindings/AssistantAnswer'
import type { DocumentImport as NativeDocumentImport } from './bindings/DocumentImport'
import type { PortfolioNews } from './bindings/PortfolioNews'
import { previewFinanceSettings } from './finance-settings'
import { ledgerCallSchema, type LedgerCall } from './ledger-view'
import { fromRust } from './rust-json'
import {
  marketHistorySchema,
  marketSnapshotsSchema,
  type Calibration,
  type ChangeLog,
  type FinanceSnapshot,
  type EarningsEvent,
  type HealthReport,
  type InvestmentSummary,
  type PortfolioToday,
  type SectorExposure,
  type StatementPreview,
  type StatementSummary,
  type StatementSync,
  type BankStatement,
  type MarketHistory,
  type MarketNewsResult,
  type MarketSnapshots,
  type ChartChanges,
  type Reports,
  type SpendingOwner,
  type SpendingPeriodDetail,
  type SpendingPeriodJudgment,
  type SummaryRun,
  type SyncRun,
  type BackupInfo,
  type BriefDocument,
  type DocumentField,
  type FoundationModelStatus,
  type IntegrationStatus,
  type LoginItemState,
  type ProviderConnection,
  type ProviderLinkSession,
  type ProviderLinkStatus,
} from './schema'

// Native results are typed by Rust's own types (src/lib/bindings), so they are read as they
// arrive; only what Rust leaves as raw JSON (the market stream, an answer's ledger evidence) is
// checked at runtime.

export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

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

export type CredentialProvider = 'plaid' | 'snaptrade' | 'alpaca' | 'alphavantage' | 'sec' | 'exa'

export async function getSyncRuns(): Promise<SyncRun[]> {
  if (!isTauri()) return []
  return invoke<SyncRun[]>('get_sync_runs')
}

/** The change log's size and the latest check that it replays to the saved state. */
export async function getChangeLog(): Promise<ChangeLog | null> {
  if (!isTauri()) return null
  return invoke<ChangeLog>('get_change_log')
}

/** How often the layered model's thresholds fired over saved sessions. */
export async function getCalibration(): Promise<Calibration | null> {
  if (!isTauri()) return null
  return invoke<Calibration>('get_calibration')
}

/** This launch's investment summary runs, newest first. */
export async function getSummaryRuns(): Promise<SummaryRun[]> {
  if (!isTauri()) return []
  return invoke<SummaryRun[]>('get_summary_runs')
}

// The preview has no native runtime; its view was made by Rust from the synthetic seed. A card
// other than the seed's own leaves the benefits and statement it was computed for.
const browserSnapshot = (): FinanceSnapshot => {
  const view = fromRust<FinanceSnapshot>(structuredClone(seedView.view))
  const settings = previewFinanceSettings()
  const sameCard = settings.spendingAccountId === seedView.spendingAccountId
  return {
    ...view,
    settings,
    insights:
      view.insights && !sameCard
        ? { ...view.insights, benefits: undefined, statement: undefined }
        : view.insights,
  }
}

export async function getFinanceSnapshot(): Promise<FinanceSnapshot> {
  if (!isTauri()) return browserSnapshot()
  return invoke<FinanceSnapshot>('get_finance_snapshot')
}

/** Which Spending period to read: whose spending, over which scope, and how many back. */
export type SpendingPeriodRequest = {
  scope: string
  owner: SpendingOwner
  kind: SpendingPeriodJudgment['kind']
  offset: number
}

/**
 * One Spending period in full, computed by Rust; null when the scope or period doesn't exist.
 * The preview reads the seed's periods, computed by the same code.
 */
export async function getSpendingPeriod(
  request: SpendingPeriodRequest,
): Promise<SpendingPeriodDetail | null> {
  if (!isTauri()) {
    const { default: periods } = await import('../data/seed-spending.json')
    const key = `${request.scope}|${request.owner}|${request.kind}|${request.offset}`
    return fromRust<Record<string, SpendingPeriodDetail>>(periods)[key] ?? null
  }
  return invoke<SpendingPeriodDetail | null>('get_spending_period', { request })
}

/** Which reports to read: over which window and accounts, and the one to read in full. */
export type ReportRequest = {
  chart?: Reports['summaries'][number]['chart']
  range?: Reports['window']['range']
  from?: string
  to?: string
  /** Every account when absent; none when empty. */
  accounts?: string[]
}

/**
 * Every report over a window and accounts, and the requested one in full, computed by Rust.
 * The preview reads the seed's reports for each chart period and Investments' periods; any other
 * window is null there.
 */
export async function getReports(request: ReportRequest): Promise<Reports | null> {
  if (!isTauri()) {
    const { default: reports } = await import('../data/seed-reports.json')
    const key = [
      request.chart ?? '',
      request.range ?? '',
      request.from ?? '',
      request.to ?? '',
      request.accounts ? request.accounts.join(',') : '*',
    ].join('|')
    return fromRust<Record<string, Reports>>(reports)[key] ?? null
  }
  return invoke<Reports | null>('get_reports', { request })
}

export async function recoverFinanceState(restore: boolean): Promise<FinanceSnapshot> {
  return invoke<FinanceSnapshot>('recover_finance_state', { restore })
}

/** A passphrase seals the documents' key into the backup, so another Mac can open them. */
export async function exportFinanceBackup(path: string, passphrase?: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('export_finance_backup', { path, passphrase: passphrase || null })
}

export async function inspectFinanceBackup(path: string): Promise<BackupInfo> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<BackupInfo>('inspect_finance_backup', { path })
}

export async function restoreFinanceBackup(
  path: string,
  passphrase?: string,
): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('restore_finance_backup', { path, passphrase: passphrase || null })
}

/** What a document import added, already had, and couldn't keep, with the view after it. */
export type DocumentImport = Omit<NativeDocumentImport, 'snapshot'> & { snapshot: FinanceSnapshot }

/** Reads files and folders into documents on this Mac, reporting `documents-progress`. */
export async function importDocuments(paths: string[]): Promise<DocumentImport> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<DocumentImport>('import_documents', { paths })
}

export type DocumentChange = {
  kind?: BriefDocument['kind']
  form?: string
  issuer?: string
  taxYear?: number
  /** An empty string clears it. */
  accountId?: string
  person?: string
  corrected?: boolean
  reviewed?: boolean
  fields?: DocumentField[]
}

export async function updateDocument(id: string, change: DocumentChange): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('update_document', { id, change })
}

export async function deleteDocuments(ids: string[]): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('delete_documents', { ids })
}

/** Opens the original in the Mac's own viewer, from a private decrypted copy. */
export async function openDocument(id: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('open_document', { id })
}

/** A document's original, decrypted in memory for Brief's viewer. */
export async function documentFile(id: string): Promise<ArrayBuffer> {
  if (!isTauri()) throw new Error('Desktop app only')
  return await invoke<ArrayBuffer>('document_file', { id })
}

export async function rereadDocument(id: string): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('reread_document', { id })
}

export async function addDocumentHistory(
  ids: string[],
  accountId: string,
): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('add_document_history', { ids, accountId })
}

/**
 * Finds the businesses in-person purchases were made at: Rust downloads Overture's places for
 * the cities not searched yet, reporting `places-progress`, and the snapshot updates per city.
 */
export async function findPlaces(): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('find_places')
}

/** Reads and checks a statement file. Rust keeps the bytes until `importStatement`. */
export async function previewStatementImport(path: string): Promise<StatementPreview> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<StatementPreview>('preview_statement_import', { path })
}

export async function importStatement(
  draftId: string,
  accountId: string,
): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('import_statement', { draftId, accountId })
}

export async function getStatementImports(): Promise<StatementSummary[]> {
  if (!isTauri()) return []
  return invoke<StatementSummary[]>('get_statement_imports')
}

export async function removeStatementImport(id: string): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('remove_statement_import', { id })
}

/** Confirms, or withdraws, a statement row Plaid never reported; a confirmed row fills history. */
export async function confirmStatementRow(
  fileId: string,
  rowId: string,
  confirmed: boolean,
): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('confirm_statement_row', { fileId, rowId, confirmed })
}

export async function eraseFinanceState(): Promise<FinanceSnapshot> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<FinanceSnapshot>('erase_finance_state')
}

export async function exportSettingsFile(path: string, contents: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('export_settings_file', { path, contents })
}

export async function readSettingsFile(path: string): Promise<string> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<string>('read_settings_file', { path })
}

export async function getAppVersion(): Promise<string> {
  return isTauri() ? getVersion() : 'Browser preview'
}

export async function getDataHealth(): Promise<HealthReport> {
  if (!isTauri()) {
    return {
      computedAt: new Date().toISOString(),
      revision: browserSnapshot().revision ?? 0,
      overallStatus: 'info',
      counts: { critical: 0, error: 0, warning: 0, info: 1 },
      issues: [
        {
          id: 'browser-preview',
          severity: 'info',
          category: 'storage',
          title: 'Synthetic browser preview',
          explanation: 'Storage and provider checks run in the desktop app',
          evidence: [],
          affectedItems: [],
          action: { label: 'Open settings', route: '/settings' },
        },
      ],
    }
  }
  return invoke<HealthReport>('get_data_health')
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

  const snapshot = await invoke<FinanceSnapshot>('refresh_finance_snapshot', { liveBalances })
  return { snapshot, warnings: snapshot.syncWarnings }
}

export async function saveAccountLink(
  plaidAccountId: string,
  snaptradeAccountId?: string,
): Promise<void> {
  if (!isTauri()) return
  await invoke('save_account_link', { plaidAccountId, snaptradeAccountId })
}

/** A connection's provider, or adding statements to a saved Plaid bank connection. */
export type LinkProvider = 'plaid' | 'plaid-investments' | 'snaptrade' | 'plaid-statements'

export async function beginProviderLink(
  provider: LinkProvider,
  itemId?: string,
): Promise<ProviderLinkSession> {
  if (!isTauri()) throw new Error('Desktop app only')
  const session = await invoke<ProviderLinkSession>('begin_provider_link', { provider, itemId })
  try {
    await openExternalUrl(session.url)
  } catch (error) {
    await cancelProviderLink(session).catch(() => undefined)
    throw error
  }
  return session
}

/** Saved bank statements, newest first. The browser preview has none. */
export async function getBankStatements(): Promise<BankStatement[]> {
  return isTauri() ? invoke<BankStatement[]>('get_bank_statements') : []
}

/** Checks every connection with statements now and saves new ones. */
export async function syncBankStatements(): Promise<StatementSync> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<StatementSync>('sync_bank_statements')
}

/** Opens a saved statement in the Mac's PDF viewer. */
export async function openBankStatement(id: string): Promise<void> {
  if (!isTauri()) throw new Error('Desktop app only')
  await invoke('open_bank_statement', { id })
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
  if (!isTauri())
    return {
      plaid: false,
      snaptrade: false,
      alpaca: false,
      alphaVantage: false,
      sec: false,
      exa: false,
    }
  return invoke<IntegrationStatus>('get_integration_status')
}

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  return isTauri() ? invoke<ProviderConnection[]>('get_provider_connections') : []
}
/** Asks SnapTrade, so it can be slow; `getProviderConnections` has the saved list. */
export async function getSnapTradeConnections(): Promise<ProviderConnection[]> {
  return isTauri() ? invoke<ProviderConnection[]>('get_snaptrade_connections') : []
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
  return invoke<IntegrationStatus>('remove_integration_credentials', { provider })
}

export async function saveIntegrationCredentials(
  provider: CredentialProvider,
  credentials: { clientId: string; secret?: string; consumerKey?: string },
): Promise<IntegrationStatus> {
  if (!isTauri()) throw new Error('Desktop app only')
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
  onHistory: (series: MarketHistory, changes?: ChartChanges) => void = () => undefined,
): Promise<void> {
  const updates = new Channel<unknown>((payload) => {
    if (!payload || typeof payload !== 'object') return
    if ('kind' in payload && payload.kind === 'update' && 'market' in payload) {
      const parsed = marketSnapshotsSchema.safeParse(payload.market)
      if (parsed.success) onUpdate(parsed.data)
      else onError('Invalid market update received')
    } else if ('kind' in payload && payload.kind === 'history' && 'chartSeries' in payload) {
      // A malformed history only costs the detail; the live frame stays. Rust always sends
      // one, empty when it has no week, so its arrival settles the changes.
      const parsed = marketHistorySchema.safeParse(payload.chartSeries)
      const changes =
        'chartChanges' in payload &&
        payload.chartChanges &&
        typeof payload.chartChanges === 'object'
          ? fromRust<ChartChanges>(payload.chartChanges)
          : undefined
      onHistory(parsed.success ? parsed.data : {}, changes)
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
  return invoke<MarketNewsResult>('get_market_news', { symbols, refresh })
}

/** The portfolio's move today at live prices, with each position's dollar effect. */
export async function getPortfolioToday(): Promise<PortfolioToday> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<PortfolioToday>('get_portfolio_today')
}

/**
 * The portfolio's, or one holding's, period since `start`: moves against benchmarks, your
 * activity, ranked company events, the latest reported quarter, and any saved context.
 */
export async function getInvestmentSummary(
  start: string,
  ticker?: string,
): Promise<InvestmentSummary> {
  // The preview shows the portfolio's summary Rust made from the synthetic seed.
  if (!isTauri() && !ticker) return fromRust<InvestmentSummary>(seedView.summary)
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<InvestmentSummary>('get_investment_summary', { start, ticker: ticker ?? null })
}

/**
 * Saves the past week's news for every held stock in one request and checks EDGAR for the
 * stalest holdings, so summaries and the holdings list are ready before they're opened. Runs at
 * most every ten minutes; `refreshed` is false when it was skipped.
 */
export async function refreshPortfolioNews(): Promise<PortfolioNews> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<PortfolioNews>('refresh_portfolio_news')
}

/** Held stocks by sector; looks up unclassified stocks in EDGAR once. */
export async function getPortfolioSectors(): Promise<SectorExposure> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<SectorExposure>('get_portfolio_sectors')
}

export async function getEarningsCalendar(symbols: string[]): Promise<EarningsEvent[]> {
  if (!isTauri()) return []
  return invoke<EarningsEvent[]>('get_earnings_calendar', { symbols })
}

export async function getFoundationModelStatus(): Promise<FoundationModelStatus> {
  if (!isTauri()) {
    return { state: 'unavailable', message: 'Apple Intelligence requires the desktop app' }
  }
  return invoke<FoundationModelStatus>('get_foundation_model_status')
}

/** An answer, the native ledger queries whose figures it may cite, and any it couldn't. */
export type FoundationAnswer = Omit<AssistantAnswer, 'evidence'> & { evidence: LedgerCall[] }

/** Reruns one read-only ledger query over the saved snapshot, without the language model. */
export async function ledgerQuery(
  name: string,
  args: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  if (!isTauri()) throw new Error('Saved queries need the Brief desktop app')
  return invoke<Record<string, unknown>>('ledger_query', { name, arguments: args })
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
      const response = await invoke<AssistantAnswer>('generate_foundation_explanation', {
        question: question.slice(0, 1_000),
        requestId,
        started,
      })
      // An abandoned answer is discarded before it is read.
      signal?.throwIfAborted()
      // Evidence is a ledger query's arguments and result, which are JSON objects.
      const evidence = response.evidence.flatMap((call) => {
        const parsed = ledgerCallSchema.safeParse(call)
        return parsed.success ? [parsed.data] : []
      })
      return { ...response, evidence }
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
  return isTauri() ? invoke<LoginItemState>('get_login_item') : 'unavailable'
}

export async function setLoginItem(enabled: boolean): Promise<LoginItemState> {
  if (!isTauri()) throw new Error('Desktop app only')
  return invoke<LoginItemState>('set_login_item', { enabled })
}

export async function revealMainWindow() {
  if (isTauri()) await invoke('reveal_main_window')
}
