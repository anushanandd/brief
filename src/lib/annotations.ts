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
    benefitId: z.string().optional(),
    note: z.string().optional(),
    people: z.array(z.string()).optional(),
  }),
)
export type TransactionAnnotations = z.infer<typeof annotationsSchema>
const annotationResultSchema = z.object({
  annotations: annotationsSchema,
  /** The changed transactions' annotations as they were, for undo. */
  previous: annotationsSchema.default({}),
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

// An empty category, tag list, split list, refund id, benefit id, or note clears that field;
// false clears other; clearBenefit forgets both benefit decisions. People are a hand choice
// either way: an empty list means nobody, and stops matching by name.
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
    benefitId: z.string().max(64).optional(),
    note: z.string().max(500).optional(),
    people: z.array(z.string().startsWith('person:').max(64)).max(12).optional(),
    clearBenefit: z.boolean().optional(),
  }),
)
export type TransactionAnnotationChanges = z.infer<typeof annotationChangesSchema>

/** Puts back annotations a save returned as `previous`, undoing that save. */
export async function restoreTransactionAnnotations(previous: TransactionAnnotations) {
  if (!isTauri()) throw new Error('Transaction edits require the Brief desktop app')
  return z
    .object({ annotations: annotationsSchema, snapshot: financeSnapshotSchema })
    .parse(await invoke('restore_transaction_annotations', { previous }))
}

export async function saveTransactionAnnotations(changes: TransactionAnnotationChanges) {
  const parsed = annotationChangesSchema.parse(changes)
  if (isTauri()) {
    return annotationResultSchema.parse(
      await invoke('transaction_annotations', { changes: parsed, importing: false }),
    )
  }
  throw new Error('Transaction edits require the Brief desktop app')
}

const legacyAnnotationKeys = ['brief:transaction-categories', 'brief:reviewed-transactions']

/** Whether renderer storage still holds annotations from before they moved to Rust. */
export function hasLegacyAnnotations() {
  try {
    return legacyAnnotationKeys.some((key) => localStorage.getItem(key) != null)
  } catch {
    return false
  }
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
  if (isTauri() && !Object.keys(changes).length) {
    // Nothing to import, so no native write: annotations already live in Rust.
    annotations = {}
  } else if (isTauri()) {
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
