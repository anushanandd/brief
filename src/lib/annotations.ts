import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import { isTauri } from './api'
import type { FinanceSnapshot } from './schema'

const annotationsSchema = z.record(
  z.string(),
  z.object({
    category: z.string().min(1).max(100).optional(),
    reviewed: z.boolean().optional(),
    benefitConfirmed: z.boolean().optional(),
  }),
)
export type TransactionAnnotations = z.infer<typeof annotationsSchema>

function legacyAnnotations(warnings: string[]): TransactionAnnotations {
  const result: TransactionAnnotations = {}
  for (const [key, field] of [
    ['brief:transaction-categories', 'category'],
    ['brief:reviewed-transactions', 'reviewed'],
  ] as const) {
    let values: unknown
    try {
      const raw = localStorage.getItem(key)
      if (!raw) continue
      values = JSON.parse(raw)
    } catch {
      warnings.push(
        'Some legacy transaction annotations could not be read; original entries were retained.',
      )
      continue
    }
    if (!values || typeof values !== 'object' || Array.isArray(values)) continue
    for (const [id, value] of Object.entries(values)) {
      if (
        (field === 'category' &&
          typeof value === 'string' &&
          value.trim() &&
          value.length <= 100) ||
        (field === 'reviewed' && typeof value === 'boolean')
      ) {
        result[id] = { ...result[id], [field]: value }
      }
    }
  }
  return result
}

export async function loadTransactionAnnotations(): Promise<{
  annotations: TransactionAnnotations
  warning?: string
}> {
  const warnings: string[] = []
  const changes = legacyAnnotations(warnings)
  let annotations: TransactionAnnotations
  if (isTauri()) {
    annotations = annotationsSchema.parse(
      await invoke('transaction_annotations', { changes, importing: true }),
    )
  } else {
    annotations = changes
    try {
      annotations = {
        ...changes,
        ...annotationsSchema.parse(JSON.parse(localStorage.getItem('brief:annotations') ?? '{}')),
      }
    } catch {
      warnings.push(
        'Saved transaction annotations could not be read. Finance data remains available.',
      )
    }
  }
  return { annotations, ...(warnings.length ? { warning: [...new Set(warnings)].join(' ') } : {}) }
}

export async function saveTransactionAnnotation(
  id: string,
  change: TransactionAnnotations[string],
): Promise<TransactionAnnotations> {
  const changes = annotationsSchema.parse({ [id]: change })
  if (isTauri())
    return annotationsSchema.parse(
      await invoke('transaction_annotations', { changes, importing: false }),
    )
  const { annotations: current, warning } = await loadTransactionAnnotations()
  if (warning) {
    const original = localStorage.getItem('brief:annotations')
    if (original) localStorage.setItem(`brief:annotations:retained:${Date.now()}`, original)
  }
  const next = { ...current, [id]: { ...current[id], ...change } }
  localStorage.setItem('brief:annotations', JSON.stringify(next))
  return next
}

export function applyTransactionAnnotations(
  snapshot: FinanceSnapshot,
  annotations: TransactionAnnotations,
): FinanceSnapshot {
  const transactions = snapshot.transactions.map((transaction) => {
    const matching = snapshot.accounts.filter(
      (account) => account.id !== 'all' && account.name === transaction.account,
    )
    return {
      ...transaction,
      accountId: transaction.accountId ?? (matching.length === 1 ? matching[0].id : undefined),
      category: annotations[transaction.id]?.category ?? transaction.category,
      benefitConfirmed:
        annotations[transaction.id]?.benefitConfirmed ?? transaction.benefitConfirmed,
    }
  })
  return {
    ...snapshot,
    transactions,
  }
}
