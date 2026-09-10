import { invoke } from '@tauri-apps/api/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import empty from '../data/empty.json'
import { refreshFinanceSnapshot, safeExternalUrl } from './api'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const mockedInvoke = vi.mocked(invoke)

describe('refresh process', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
    mockedInvoke.mockReset()
  })
  afterEach(() => vi.unstubAllGlobals())

  it('accepts only the snapshot committed by the canonical native refresh', async () => {
    mockedInvoke.mockResolvedValue({ ...empty, revision: 7, syncWarnings: ['Cached source'] })
    const result = await refreshFinanceSnapshot()

    expect(result.snapshot.revision).toBe(7)
    expect(result.warnings).toEqual(['Cached source'])
    expect(mockedInvoke).toHaveBeenCalledWith('refresh_finance_snapshot')
    expect(mockedInvoke).toHaveBeenCalledTimes(1)
  })

  it('rejects an invalid native projection', async () => {
    mockedInvoke.mockResolvedValue({ ...empty, netWorth: 'invalid' })
    await expect(refreshFinanceSnapshot()).rejects.toThrow()
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
