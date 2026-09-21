import { invoke } from '@tauri-apps/api/core'
import { afterEach, expect, it, vi } from 'vitest'

import { getHoldingThesis, saveHoldingThesis } from './holding-thesis'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), Channel: vi.fn() }))
afterEach(() => {
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it('saves one ticker through native persistence and propagates a failed write', async () => {
  vi.stubGlobal('window', { __TAURI_INTERNALS__: {} })
  vi.mocked(invoke).mockResolvedValueOnce('Synthetic thesis.')
  expect(await saveHoldingThesis('TEST', ' Synthetic thesis. ')).toBe('Synthetic thesis.')
  expect(invoke).toHaveBeenCalledWith('save_holding_thesis', {
    ticker: 'TEST',
    thesis: 'Synthetic thesis.',
  })
  vi.mocked(invoke).mockRejectedValueOnce(new Error('Write failed'))
  await expect(saveHoldingThesis('TEST', '')).rejects.toThrow('Write failed')
  await expect(saveHoldingThesis('TEST', 'x'.repeat(1001))).rejects.toThrow()
})

it('keeps browser preview theses isolated by ticker and supports clearing', async () => {
  vi.stubGlobal('window', {})
  const data = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => data.set(key, value),
    removeItem: (key: string) => data.delete(key),
  })
  await saveHoldingThesis('TEST', 'Synthetic thesis.')
  await saveHoldingThesis('OTHER', 'Other thesis.')
  expect(await getHoldingThesis('TEST')).toBe('Synthetic thesis.')
  await saveHoldingThesis('TEST', '')
  expect(await getHoldingThesis('TEST')).toBe('')
  expect(await getHoldingThesis('OTHER')).toBe('Other thesis.')
})
