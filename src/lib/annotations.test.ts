import { invoke } from '@tauri-apps/api/core'
import { afterEach, expect, it, vi } from 'vitest'

import native from '../data/fixtures/native-finance.json'
import { saveTransactionCategory } from './annotations'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('./api', () => ({ isTauri: () => true }))
afterEach(() => vi.resetAllMocks())

it('returns the native annotation-aware snapshot without interpreting its transactions', async () => {
  const result = { annotations: { synthetic: { category: 'Travel' } }, snapshot: native }
  vi.mocked(invoke).mockResolvedValue(result)
  expect(await saveTransactionCategory('synthetic', 'Travel')).toEqual(result)
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
