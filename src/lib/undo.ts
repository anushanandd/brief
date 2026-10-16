import { useSyncExternalStore } from 'react'

import type { TransactionAnnotations } from './annotations'

/** One undoable edit: the changed transactions' annotations as they were before it. */
export type UndoEntry = { label: string; previous: TransactionAnnotations }

const MAX_ENTRIES = 20
let entries: readonly UndoEntry[] = []
const listeners = new Set<() => void>()

const notify = () => listeners.forEach((listener) => listener())

/** Remembers an edit so ⌘Z can put back what it changed. Nothing is stored past this session. */
export function rememberEdit(entry: UndoEntry) {
  if (!Object.keys(entry.previous).length) return
  entries = [...entries.slice(-(MAX_ENTRIES - 1)), entry]
  notify()
}

/** Takes the latest remembered edit, or nothing when there is none. */
export function takeLastEdit(): UndoEntry | undefined {
  const last = entries.at(-1)
  if (!last) return undefined
  entries = entries.slice(0, -1)
  notify()
  return last
}

/** Forgets every remembered edit, as a refresh or restore that rewrites annotations must. */
export function forgetEdits() {
  if (!entries.length) return
  entries = []
  notify()
}

export function useUndoStack() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    () => entries,
    () => entries,
  )
}
