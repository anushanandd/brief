import { useQueryClient } from '@tanstack/react-query'
import { useCallback } from 'react'

import {
  restoreTransactionAnnotations,
  saveTransactionAnnotations,
  type TransactionAnnotationChanges,
} from '../lib/annotations'
import { saveFinanceSettings } from '../lib/finance-settings'
import type { FinanceSettings, FinanceSnapshot } from '../lib/schema'
import { rememberEdit, takeLastEdit, useUndoStack } from '../lib/undo'
import { financeQueryKey, useFinance } from './use-finance'

/** A few words for what an edit changed, for the undo action's title. */
function describeEdit(changes: TransactionAnnotationChanges) {
  const ids = Object.keys(changes)
  const fields = new Set(ids.flatMap((id) => Object.keys(changes[id])))
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

/**
 * Durable edits go through narrow native commands. Each returns the annotation-aware snapshot,
 * which replaces the cached view so every mounted page updates. Transaction edits are
 * remembered for undo until the session ends.
 */
export function useFinanceEdits() {
  const client = useQueryClient()
  const finance = useFinance()
  const undoStack = useUndoStack()
  const commit = useCallback(
    (snapshot: FinanceSnapshot) => {
      client.setQueryData(financeQueryKey, snapshot)
      return snapshot
    },
    [client],
  )
  const updateSettings = useCallback(
    async (change: (current: FinanceSettings) => FinanceSettings) => {
      const current = client.getQueryData<FinanceSnapshot>(financeQueryKey)?.settings
      if (!current) throw new Error('Finance settings are unavailable')
      return commit(await saveFinanceSettings(change(current)))
    },
    [client, commit],
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
  return {
    settings: finance.data?.settings,
    updateSettings,
    annotate,
    undo,
    /** What ⌘Z would undo, or nothing. */
    undoLabel: undoStack.at(-1)?.label,
  }
}
