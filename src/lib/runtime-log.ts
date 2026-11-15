import { useSyncExternalStore } from 'react'

/**
 * This session's diagnostics for the Logs page: what loaded and what failed. It sits outside the
 * app's data, so a new line renders Logs alone instead of every page reading finance.
 */
export type RuntimeLogEntry = {
  id: number
  at: string
  source: string
  message: string
  detail?: string
  tone: 'positive' | 'negative' | 'neutral'
}

let entries: readonly RuntimeLogEntry[] = []
let lastId = 0
const listeners = new Set<() => void>()

export function appendRuntimeLog(
  source: string,
  message: string,
  tone: RuntimeLogEntry['tone'] = 'neutral',
  detail?: string,
) {
  const at = new Date().toISOString()
  const latest = entries[0]
  if (
    latest?.source === source &&
    latest.message === message &&
    latest.detail === detail &&
    Date.parse(at) - Date.parse(latest.at) < 1_500
  )
    return
  lastId += 1
  entries = [{ id: lastId, at, source, message, detail, tone }, ...entries].slice(0, 50)
  for (const notify of listeners) notify()
}

const subscribe = (notify: () => void) => {
  listeners.add(notify)
  return () => {
    listeners.delete(notify)
  }
}
const current = () => entries

export function useRuntimeLog() {
  return useSyncExternalStore(subscribe, current, current)
}
