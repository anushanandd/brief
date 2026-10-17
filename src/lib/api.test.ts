import { Channel, invoke } from '@tauri-apps/api/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import empty from '../data/empty.json'
import {
  forgetProviderConnection,
  getMarketNews,
  pollProviderLink,
  generateFoundationExplanation,
  refreshFinanceSnapshot,
  safeExternalUrl,
} from './api'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {
    constructor(public onmessage: (message: unknown) => void) {}
  },
}))
const mockedInvoke = vi.mocked(invoke)

describe('refresh process', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
    mockedInvoke.mockReset()
  })
  afterEach(() => vi.unstubAllGlobals())

  it('only requests provider news for an explicit refresh', async () => {
    const result = {
      articles: [],
      savedAt: null,
      warning: null,
      canRefresh: true,
      company: null,
      filings: [],
      filingsSavedAt: null,
      filingsWarning: null,
    }
    mockedInvoke.mockResolvedValue(result)
    expect(await getMarketNews(['TEST'])).toEqual(result)
    expect(mockedInvoke).toHaveBeenLastCalledWith('get_market_news', {
      symbols: ['TEST'],
      refresh: false,
    })
    await getMarketNews(['TEST'], true)
    expect(mockedInvoke).toHaveBeenLastCalledWith('get_market_news', {
      symbols: ['TEST'],
      refresh: true,
    })
  })

  it('only confirms browser completion when explicitly requested', async () => {
    const session = {
      provider: 'snaptrade' as const,
      sessionId: 'synthetic-session',
      url: 'https://example.com',
    }
    mockedInvoke.mockResolvedValue({ status: 'pending' })
    await pollProviderLink(session)
    expect(mockedInvoke).toHaveBeenLastCalledWith('poll_provider_link', {
      provider: 'snaptrade',
      sessionId: 'synthetic-session',
      browserCompleted: false,
    })
    mockedInvoke.mockResolvedValue({ status: 'connected' })
    expect(await pollProviderLink(session, true)).toEqual({ status: 'connected' })
    expect(mockedInvoke).toHaveBeenLastCalledWith('poll_provider_link', {
      provider: 'snaptrade',
      sessionId: 'synthetic-session',
      browserCompleted: true,
    })
    mockedInvoke.mockRejectedValueOnce(new Error('No active SnapTrade connection found'))
    await expect(pollProviderLink(session, true)).rejects.toThrow('No active SnapTrade connection')
  })

  it('forgets the selected native connection and propagates persistence failures', async () => {
    mockedInvoke.mockResolvedValueOnce(undefined)
    await forgetProviderConnection('synthetic-item')
    expect(mockedInvoke).toHaveBeenCalledWith('forget_provider_connection', {
      itemId: 'synthetic-item',
    })
    mockedInvoke.mockRejectedValueOnce(new Error('Synthetic Keychain failure'))
    await expect(forgetProviderConnection('synthetic-item')).rejects.toThrow(
      'Synthetic Keychain failure',
    )
  })

  it('accepts only the snapshot committed by the canonical native refresh', async () => {
    mockedInvoke.mockResolvedValue({ ...empty, revision: 7, syncWarnings: ['Cached source'] })
    const result = await refreshFinanceSnapshot()

    expect(result.snapshot.revision).toBe(7)
    expect(result.warnings).toEqual(['Cached source'])
    expect(mockedInvoke).toHaveBeenCalledWith('refresh_finance_snapshot', { liveBalances: false })
    expect(mockedInvoke).toHaveBeenCalledTimes(1)
  })

  it('requests billed real-time balances only when asked', async () => {
    mockedInvoke.mockResolvedValue({ ...empty, revision: 8 })
    await refreshFinanceSnapshot({ liveBalances: true })
    expect(mockedInvoke).toHaveBeenCalledWith('refresh_finance_snapshot', { liveBalances: true })
  })

  it('never exposes an uncommitted snapshot when native refresh fails', async () => {
    mockedInvoke.mockRejectedValue(new Error('Refresh superseded'))
    await expect(refreshFinanceSnapshot()).rejects.toThrow('Refresh superseded')
  })
})

describe('external links', () => {
  it('accepts only complete HTTPS URLs', () => {
    expect(safeExternalUrl('https://merchant.example/store')).toBe('https://merchant.example/store')
    expect(safeExternalUrl('http://merchant.example')).toBeUndefined()
    expect(safeExternalUrl('javascript:alert(1)')).toBeUndefined()
    expect(safeExternalUrl('merchant.example')).toBe('https://merchant.example/')
  })
})

describe('native explanation cancellation', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
    mockedInvoke.mockReset()
  })
  afterEach(() => vi.unstubAllGlobals())
  it('defers cancellation until native registration and rejects the abandoned result', async () => {
    let finish!: (value: unknown) => void
    mockedInvoke.mockImplementation((command) =>
      command === 'generate_foundation_explanation'
        ? new Promise((resolve) => {
            finish = resolve
          })
        : Promise.resolve(undefined),
    )
    const controller = new AbortController()
    const result = generateFoundationExplanation('What changed?', controller.signal)
    const rejected = expect(result).rejects.toMatchObject({ name: 'AbortError' })
    await Promise.resolve()
    const args = mockedInvoke.mock.calls[0][1]
    if (
      !args ||
      Array.isArray(args) ||
      args instanceof ArrayBuffer ||
      ArrayBuffer.isView(args) ||
      !(args.started instanceof Channel)
    )
      throw new Error('Missing native registration channel')
    // The renderer sends only the question; native ledger tools supply any figures.
    expect(args).toMatchObject({ question: 'What changed?' })
    expect(args).not.toHaveProperty('evidence')
    controller.abort()
    expect(mockedInvoke).toHaveBeenCalledTimes(1)
    args.started.onmessage(null)
    expect(mockedInvoke).toHaveBeenLastCalledWith('cancel_foundation_explanation', {
      requestId: args.requestId,
    })
    finish({ text: 'Discard this result', evidence: [] })
    await rejected
  })
  it('keeps an answer’s outcome and the figures its queries did not return', async () => {
    mockedInvoke.mockResolvedValue({
      text: 'You spent $900.',
      outcome: 'answered',
      evidence: [],
      unverified: ['$900'],
    })
    await expect(generateFoundationExplanation('What did I spend?', undefined)).resolves.toEqual({
      text: 'You spent $900.',
      outcome: 'answered',
      evidence: [],
      unverified: ['$900'],
    })
    // Evidence is kept only as the ledger queries Rust ran: objects in and out.
    mockedInvoke.mockResolvedValue({
      text: 'Checked.',
      outcome: 'answered',
      evidence: [
        { name: 'cashFlow', arguments: {}, result: { spending: -812.4 } },
        { name: 'document', arguments: null, result: 'summary text' },
      ],
      unverified: [],
    })
    await expect(
      generateFoundationExplanation('What did I spend?', undefined),
    ).resolves.toMatchObject({
      evidence: [{ name: 'cashFlow', arguments: {}, result: { spending: -812.4 } }],
    })
  })
  it('never starts a job already cancelled while queued', async () => {
    const controller = new AbortController()
    controller.abort()
    await expect(
      generateFoundationExplanation('What changed?', controller.signal),
    ).rejects.toMatchObject({ name: 'AbortError' })
    expect(mockedInvoke).not.toHaveBeenCalled()
  })
})
