import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import { isTauri } from './api'

const thesisSchema = z.string().max(1000)
const key = (ticker: string) => `brief.holdingThesis:${ticker}`
export async function getHoldingThesis(ticker: string): Promise<string> {
  return thesisSchema.parse(
    isTauri()
      ? await invoke('get_holding_thesis', { ticker })
      : (localStorage.getItem(key(ticker)) ?? ''),
  )
}
export async function saveHoldingThesis(ticker: string, value: string): Promise<string> {
  const thesis = thesisSchema.parse(value.trim())
  if (isTauri()) return thesisSchema.parse(await invoke('save_holding_thesis', { ticker, thesis }))
  if (thesis) localStorage.setItem(key(ticker), thesis)
  else localStorage.removeItem(key(ticker))
  return thesis
}
