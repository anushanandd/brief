import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { useCallback } from 'react'

import {
  restoreTransactionAnnotations,
  saveTransactionAnnotations,
  type TransactionAnnotationChanges,
} from '../lib/annotations'
import { completeSettings, saveFinanceSettings, type Settings } from '../lib/finance-settings'
import type { FinanceSettings, FinanceSnapshot } from '../lib/schema'
import { rememberEdit, takeLastEdit, useUndoStack } from '../lib/undo'
import { financeQueryKey, setFinanceSnapshot } from './use-finance'

/** A few words for what an edit changed, for the undo action's title. */
function describeEdit(changes: TransactionAnnotationChanges) {
  const ids = Object.keys(changes)
  const fields = new Set(Object.values(changes).flatMap((change) => Object.keys(change)))
  const field = [...fields][0]
  const what =
    fields.size !== 1
      ? 'edit'
      : field === 'category'
        ? 'category'
        : field === 'reviewed'
          ? 'review'
          : field === 'tags'
            ? 'tags'
            : field === 'splits'
              ? 'split'
              : field === 'refundOf'
                ? 'refund link'
                : field === 'other'
                  ? 'who it’s for'
                  : field === 'note'
                    ? 'note'
                    : field === 'people'
                      ? 'people'
                      : 'benefit'
  return ids.length > 1 ? `${what} on ${ids.length} transactions` : what
}

let settingsSaves: Promise<unknown> = Promise.resolve()

/**
 * Saves settings once every earlier save has landed, so each change reads the settings the
 * last one committed; two quick toggles never write over each other.
 */
export function saveSettingsInTurn(
  client: QueryClient,
  change: (current: Settings) => FinanceSettings,
) {
  const saved = settingsSaves.then(async () => {
    const current = client.getQueryData<FinanceSnapshot>(financeQueryKey)?.settings
    if (!current) throw new Error('Finance settings are unavailable')
    const snapshot = await saveFinanceSettings(change(completeSettings(current)))
    setFinanceSnapshot(client, snapshot)
    return snapshot
  })
  settingsSaves = saved.catch(() => undefined)
  return saved
}

/**
 * Durable edits go through narrow native commands. Each returns the annotation-aware snapshot,
 * which replaces the cached view so every mounted page updates. Transaction edits are
 * remembered for undo until the session ends.
 */
export function useFinanceEdits() {
  const client = useQueryClient()
  const commit = useCallback(
    (snapshot: FinanceSnapshot) => {
      setFinanceSnapshot(client, snapshot)
      return snapshot
    },
    [client],
  )
  const updateSettings = useCallback(
    (change: (current: Settings) => FinanceSettings) => saveSettingsInTurn(client, change),
    [client],
  )
  const annotate = useCallback(
    async (changes: TransactionAnnotationChanges) => {
      const saved = await saveTransactionAnnotations(changes)
      rememberEdit({ label: describeEdit(changes), previous: saved.previous })
      return commit(saved.snapshot)
    },
    [commit],
  )
  const undo = useCallback(async () => {
    const last = takeLastEdit()
    if (!last) return undefined
    try {
      const restored = await restoreTransactionAnnotations(last.previous)
      return { label: last.label, snapshot: commit(restored.snapshot) }
    } catch (error) {
      // The edit stays undoable; the failure is reported by the caller.
      rememberEdit(last)
      throw error
    }
  }, [commit])
  return { updateSettings, annotate, undo }
}

/** Undo and what ⌘Z would undo, or nothing; only this re-renders when the undo stack changes. */
export function useUndo() {
  const { undo } = useFinanceEdits()
  return { undo, undoLabel: useUndoStack().at(-1)?.label }
}
