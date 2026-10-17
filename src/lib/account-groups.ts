import type { Account, Rollup } from './schema'

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
  spending: 'Spending cards',
}

/** What an account type is called in a list of groups. */
export const groupLabel = (type: string) =>
  groupLabels[type] ?? type.charAt(0).toLocaleUpperCase() + type.slice(1)

const byValue = (left: { value: number | null }, right: { value: number | null }) =>
  (right.value ?? -Infinity) - (left.value ?? -Infinity)

/**
 * Accounts grouped by type in the order Rust ranked the types, each with the total and the
 * period's change Rust summed for it; each group's accounts largest first.
 */
export function accountGroups(
  accounts: Account[],
  totals: Rollup['groups'],
  changes: Array<{ type: string; change: number | null }> = [],
): AccountGroup[] {
  return totals.flatMap(({ type, value }) => {
    const members = accounts.filter((account) => account.type === type)
    return members.length
      ? [
          {
            type,
            label: groupLabel(type),
            accounts: members.toSorted(byValue),
            value,
            change: changes.find((change) => change.type === type)?.change ?? null,
          },
        ]
      : []
  })
}
