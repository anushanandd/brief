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

/**
 * Largest first. `rank` holds the values to order by, the committed snapshot's, so live prices
 * never reorder rows or move a number-key jump off the row it names; each account's own value
 * stands in without one.
 */
export const largestFirst =
  (rank?: ReadonlyMap<string, number | null>) => (left: Account, right: Account) =>
    ((rank?.has(right.id) ? rank.get(right.id) : right.value) ?? -Infinity) -
    ((rank?.has(left.id) ? rank.get(left.id) : left.value) ?? -Infinity)

/**
 * Accounts grouped by type in the order Rust ranked the types, each with the total and the
 * period's change Rust summed for it; each group's accounts largest first.
 */
export function accountGroups(
  accounts: Account[],
  totals: Rollup['groups'],
  changes: Array<{ type: string; change: number | null }> = [],
  rank?: ReadonlyMap<string, number | null>,
): AccountGroup[] {
  return totals.flatMap(({ type, value }) => {
    const members = accounts.filter((account) => account.type === type)
    return members.length
      ? [
          {
            type,
            label: groupLabel(type),
            accounts: members.toSorted(largestFirst(rank)),
            value,
            change: changes.find((change) => change.type === type)?.change ?? null,
          },
        ]
      : []
  })
}
