import type { Transaction } from './schema'

// Native policy is authoritative; missing fields are rejected at the IPC boundary.
export type TransactionMarkKind = Transaction['classification']['mark']
export type MoneyKind = Transaction['classification']['kind']
export const transactionMarkKind = (t: Transaction) => t.classification.mark
export const moneyKind = (t: Transaction) => t.classification.kind
export const isSpendingTransaction = (t: Transaction) => t.classification.spending
export const isZelle = (t: Transaction) => t.classification.zelle

/** Presentation groups for records already admitted to the Income chart. */
export function incomeActivityGroup(transaction: Transaction) {
  if (isZelle(transaction)) return 'Zelle payments'
  if (/\btransfer\s+money\s+from\s+brokerage\b/i.test(transaction.merchant))
    return 'Brokerage transfers'
  const mark = transactionMarkKind(transaction)
  if (mark === 'dividend') return 'Dividends'
  if (mark === 'interest') return 'Interest'
  if (/\bpayroll\b|\bpaycheck\b|\bsalary\b/i.test(transaction.merchant)) return 'Payroll'
  return transaction.merchant.trim().replace(/\s+/g, ' ')
}
