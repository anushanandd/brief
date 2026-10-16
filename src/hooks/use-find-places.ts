import { listen } from '@tauri-apps/api/event'
import { useSyncExternalStore } from 'react'

import { findPlaces } from '../lib/api'

type Progress = { running: boolean; done: number; total: number }

let progress: Progress = { running: false, done: 0, total: 0 }
const listeners = new Set<() => void>()

function update(next: Progress) {
  progress = next
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/**
 * Starts finding places, once at a time wherever it's started from; cities report as they
 * finish. Rejects with the reason when the search stops.
 */
async function start() {
  if (progress.running) return
  update({ running: true, done: 0, total: 0 })
  const stop = await listen<{ done: number; total: number }>('places-progress', (event) =>
    update({ running: true, ...event.payload }),
  )
  try {
    await findPlaces()
  } finally {
    stop()
    update({ running: false, done: 0, total: 0 })
  }
}

/** Finding places for in-person purchases: whether it's running, how far, and how to start. */
export function useFindPlaces() {
  const state = useSyncExternalStore(
    subscribe,
    () => progress,
    () => progress,
  )
  return { ...state, start }
}
