import type { Transaction } from './schema'

// Native policy is authoritative; missing fields are rejected at the IPC boundary.
export type TransactionMarkKind = Transaction['classification']['mark']
export const transactionMarkKind = (t: Transaction) => t.classification.mark
export const moneyKind = (t: Transaction) => t.classification.kind
export const isSpendingTransaction = (t: Transaction) => t.classification.spending

/**
 * A transaction as the parts its categories total over: one per saved split, each with that
 * part's category and amount, or the transaction itself. Parts keep the transaction's id.
 */
export function splitParts(transaction: Transaction): Transaction[] {
  return transaction.splits?.length
    ? transaction.splits.map((part) => ({ ...transaction, ...part }))
    : [transaction]
}
