import { cents, sumCents } from './money'
import type { Account } from './schema'

export type AccountGroup = {
  type: string
  label: string
  accounts: Account[]
  /** Null when any account in the group has no known value. */
  value: number | null
  /** Null when any account in the group has no change for the period. */
  change: number | null
  /** Listed for reference but left out of the accounts total (spending cards). */
  outsideTotal?: boolean
}

const groupLabels: Record<string, string> = {
  cash: 'Cash',
  brokerage: 'Brokerage',
  retirement: 'Retirement',
  credit: 'Credit cards',
}

// Presentation totals over the native account values; values are summed in minor units.
function total(values: Array<number | null | undefined>) {
  return values.some((value) => value == null)
    ? null
    : values.reduce<number>((sum, value) => sum + cents(value!), 0) / 100
}

const byValue = (left: { value: number | null }, right: { value: number | null }) =>
  (right.value ?? -Infinity) - (left.value ?? -Infinity)

/** Accounts grouped by type, largest total first, each group's accounts largest first. */
export function accountGroups(
  accounts: Account[],
  changes: Array<{ accountId: string; change: number }> = [],
): AccountGroup[] {
  const byType = new Map<string, Account[]>()
  for (const account of accounts) {
    byType.set(account.type, [...(byType.get(account.type) ?? []), account])
  }
  return [...byType]
    .map(([type, members]) => ({
      type,
      label: groupLabels[type] ?? type.charAt(0).toLocaleUpperCase() + type.slice(1),
      accounts: members.toSorted(byValue),
      // A manual account awaiting its first balance has nothing to add yet.
      value: total(
        members
          .filter(({ id, value }) => value != null || !id.startsWith('manual:'))
          .map(({ value }) => value),
      ),
      change: total(
        members.map(({ id }) => changes.find(({ accountId }) => accountId === id)?.change),
      ),
    }))
    .toSorted((left, right) => byValue(left, right) || left.label.localeCompare(right.label))
}

/**
 * What the accounts own and owe, by each account's sign. Accounts without a value are left out
 * and flagged, so the totals read as known amounts.
 */
export function balanceSheet(accounts: Account[]) {
  const values = accounts.flatMap(({ value }) => (value == null ? [] : [value]))
  return {
    assets: sumCents(values.filter((value) => value > 0)),
    liabilities: sumCents(values.filter((value) => value < 0)),
    complete: values.length === accounts.length,
  }
}
