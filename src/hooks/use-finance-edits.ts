import { useQueryClient } from '@tanstack/react-query'
import { useCallback } from 'react'

import { saveTransactionAnnotations, type TransactionAnnotationChanges } from '../lib/annotations'
import { saveCategoryRules, saveFinanceSettings } from '../lib/finance-settings'
import type { CategoryRule, FinanceSettings, FinanceSnapshot } from '../lib/schema'
import { financeQueryKey, useFinance } from './use-finance'

/**
 * Durable edits go through narrow native commands. Each returns the annotation- and
 * rule-aware snapshot, which replaces the cached view so every mounted page updates.
 */
export function useFinanceEdits() {
  const client = useQueryClient()
  const finance = useFinance()
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
  const updateRules = useCallback(
    async (change: (current: CategoryRule[]) => CategoryRule[]) => {
      const current = client.getQueryData<FinanceSnapshot>(financeQueryKey)?.rules ?? []
      return commit(await saveCategoryRules(change(current)))
    },
    [client, commit],
  )
  const annotate = useCallback(
    async (changes: TransactionAnnotationChanges) =>
      commit((await saveTransactionAnnotations(changes)).snapshot),
    [commit],
  )
  return { settings: finance.data?.settings, updateSettings, updateRules, annotate }
}
