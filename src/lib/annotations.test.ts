import { invoke } from '@tauri-apps/api/core'
import { afterEach, expect, it, vi } from 'vitest'

import native from '../data/fixtures/native-finance.json'
import {
  hasLegacyAnnotations,
  loadTransactionAnnotations,
  saveTransactionCategory,
} from './annotations'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('./api', () => ({ isTauri: () => true }))
afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
})

it('skips the native import at launch when renderer storage holds no legacy annotations', async () => {
  vi.stubGlobal('localStorage', { getItem: () => null })
  expect(hasLegacyAnnotations()).toBe(false)
  expect(await loadTransactionAnnotations()).toEqual({ annotations: {} })
  expect(invoke).not.toHaveBeenCalled()
})

it('imports legacy annotations into Rust when renderer storage still holds them', async () => {
  const stored: Record<string, string> = {
    'brief:transaction-categories': JSON.stringify({ synthetic: 'Travel' }),
  }
  vi.stubGlobal('localStorage', { getItem: (key: string) => stored[key] ?? null })
  vi.mocked(invoke).mockResolvedValue({
    annotations: { synthetic: { category: 'Travel' } },
    snapshot: native,
  })
  expect(hasLegacyAnnotations()).toBe(true)
  expect(await loadTransactionAnnotations()).toEqual({
    annotations: { synthetic: { category: 'Travel' } },
  })
  expect(invoke).toHaveBeenCalledWith('transaction_annotations', {
    changes: { synthetic: { category: 'Travel' } },
    importing: true,
  })
})

it('returns the native annotation-aware snapshot without interpreting its transactions', async () => {
  const result = { annotations: { synthetic: { category: 'Travel' } }, snapshot: native }
  vi.mocked(invoke).mockResolvedValue(result)
  expect(await saveTransactionCategory('synthetic', 'Travel')).toEqual({ ...result, previous: {} })
  expect(invoke).toHaveBeenCalledWith('transaction_annotations', {
    changes: { synthetic: { category: 'Travel' } },
    importing: false,
  })
})

it('rejects a native response missing financial classification instead of guessing', async () => {
  const snapshot = structuredClone(native)
  Reflect.deleteProperty(snapshot.transactions[0], 'classification')
  vi.mocked(invoke).mockResolvedValue({ annotations: {}, snapshot })
  await expect(saveTransactionCategory('synthetic', 'Travel')).rejects.toThrow()
})
