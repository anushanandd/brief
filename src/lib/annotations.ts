import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import { isTauri } from './api'
import { financeSnapshotSchema } from './schema'

const splitSchema = z.object({ category: z.string().min(1).max(100), amount: z.number() })
const annotationsSchema = z.record(
  z.string(),
  z.object({
    category: z.string().min(1).max(100).optional(),
    reviewed: z.boolean().optional(),
    benefitConfirmed: z.boolean().optional(),
    tags: z.array(z.string()).optional(),
    splits: z.array(splitSchema).optional(),
    refundOf: z.string().optional(),
    other: z.boolean().optional(),
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
  return saveTransactionAnnotations({ [id]: annotation })
}

// An empty category, tag list, split list, or refund id clears that field; false clears other.
const annotationChangesSchema = z.record(
  z.string(),
  z.object({
    category: z.string().max(100).optional(),
    reviewed: z.boolean().optional(),
    benefitConfirmed: z.boolean().optional(),
    tags: z.array(z.string().max(40)).max(12).optional(),
    splits: z.array(splitSchema).max(12).optional(),
    refundOf: z.string().max(200).optional(),
    other: z.boolean().optional(),
  }),
)
export type TransactionAnnotationChanges = z.infer<typeof annotationChangesSchema>

export async function saveTransactionAnnotations(changes: TransactionAnnotationChanges) {
  const parsed = annotationChangesSchema.parse(changes)
  if (isTauri()) {
    return annotationResultSchema.parse(
      await invoke('transaction_annotations', { changes: parsed, importing: false }),
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
      warnings.push('Couldn’t read some legacy annotations; originals kept')
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
      warnings.push('Couldn’t read saved annotations')
    }
  }
  return { annotations, ...(warnings.length ? { warning: [...new Set(warnings)].join(' ') } : {}) }
}
