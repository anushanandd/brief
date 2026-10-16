import type { Transaction } from './schema'
import { moneyKind, splitParts } from './transaction-kind'

/** Whole cents from dollars. Halves round up, as Rust's `numeric::minor_units` does. */
export const cents = (dollars: number) => Math.round(dollars * 100)

/** Dollars summed in whole cents, so float error never reaches a total. */
export const sumCents = (values: Iterable<number>) => {
  let total = 0
  for (const value of values) total += cents(value)
  return total / 100
}

const money = (dollars: number) => cents(dollars) / 100

export type CashFlowBreakdownItem = {
  id: string
  label: string
  value: number
  children: CashFlowBreakdownTransaction[]
}

export type CashFlowBreakdownTransaction = {
  id: string
  label: string
  value: number
  date: string
}

function aggregateCashFlowItems(
  entries: Array<{ group: string; transaction: CashFlowBreakdownTransaction }>,
  prefix: string,
): CashFlowBreakdownItem[] {
  const groups = new Map<string, CashFlowBreakdownTransaction[]>()
  for (const { group, transaction } of entries) {
    const children = groups.get(group) ?? []
    children.push(transaction)
    groups.set(group, children)
  }
  return [...groups]
    .map(([label, children]) => ({
      id: `${prefix}:${label}`,
      label,
      value: money(children.reduce((total, child) => total + child.value, 0)),
      children: children.toSorted(
        (left, right) =>
          right.date.localeCompare(left.date) ||
          right.value - left.value ||
          left.label.localeCompare(right.label),
      ),
    }))
    .toSorted((left, right) => right.value - left.value || left.label.localeCompare(right.label))
}

export function cashFlowBreakdown(transactions: Transaction[]) {
  const sources: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  const purchases: Array<{ group: string; transaction: CashFlowBreakdownTransaction }> = []
  let fees = 0
  let taxes = 0
  for (const transaction of transactions) {
    const kind = moneyKind(transaction)
    const amount = Math.abs(transaction.amount)
    if (
      transaction.amount > 0 &&
      (kind === 'income' ||
        kind === 'dividend' ||
        kind === 'interest' ||
        kind === 'reimbursement' ||
        kind === 'fee' ||
        kind === 'tax')
    ) {
      const source = transaction.merchant.trim() || 'Unknown source'
      sources.push({
        group: source,
        transaction: {
          id: `source-transaction:${transaction.id}`,
          label: transaction.description?.trim() || source,
          value: money(amount),
          date: (transaction.postedOn ?? transaction.date).slice(0, 10),
        },
      })
    } else if (kind === 'expense') {
      splitParts(transaction).forEach((part, index) =>
        purchases.push({
          group: part.category.trim() || 'Other',
          transaction: {
            id: `purchase-transaction:${transaction.id}${index ? `:${index}` : ''}`,
            label: transaction.merchant.trim() || transaction.description?.trim() || 'Purchase',
            value: money(Math.abs(part.amount)),
            date: (transaction.postedOn ?? transaction.date).slice(0, 10),
          },
        }),
      )
    } else if (kind === 'fee') {
      fees += amount
    } else if (kind === 'tax') {
      taxes += amount
    }
  }
  return {
    sources: aggregateCashFlowItems(sources, 'source'),
    purchases: aggregateCashFlowItems(purchases, 'purchase'),
    fees: money(fees),
    taxes: money(taxes),
  }
}
