import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import { isTauri } from './api'
import { financeSnapshotSchema } from './schema'

const annotationsSchema = z.record(
  z.string(),
  z.object({
    category: z.string().min(1).max(100).optional(),
    reviewed: z.boolean().optional(),
    benefitConfirmed: z.boolean().optional(),
  }),
)
export type TransactionAnnotations = z.infer<typeof annotationsSchema>
const annotationResultSchema = z.object({
  annotations: annotationsSchema,
  snapshot: financeSnapshotSchema,
})

export async function saveTransactionCategory(id: string, category: string) {
  return saveTransactionAnnotation(id, { category })
}

export async function saveTransactionAnnotation(
  id: string,
  annotation: TransactionAnnotations[string],
) {
  const change = annotationsSchema.parse({ [id]: annotation })
  if (isTauri()) {
    return annotationResultSchema.parse(
      await invoke('transaction_annotations', { changes: change, importing: false }),
    )
  }
  throw new Error('Transaction edits require the Brief desktop app')
}

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
    annotations = annotationResultSchema.parse(
      await invoke('transaction_annotations', { changes, importing: true }),
    ).annotations
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
