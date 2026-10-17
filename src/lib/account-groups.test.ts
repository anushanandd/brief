import { expect, it } from 'vitest'

import { accountGroups, groupLabel } from './account-groups'
import type { Account } from './schema'

const account = (id: string, type: string, value: number | null): Account => ({
  id,
  name: id,
  institution: 'Example',
  type,
  value,
})

it("groups accounts under Rust's totals in its order, each group's accounts largest first", () => {
  const groups = accountGroups(
    [
      account('card', 'credit', -20.1),
      account('checking', 'cash', 0.1),
      account('savings', 'cash', 0.2),
      account('broker', 'brokerage', 0.25),
    ],
    [
      { type: 'cash', value: 0.3 },
      { type: 'brokerage', value: 0.25 },
      { type: 'credit', value: -20.1 },
      // A type with no account here is left out.
      { type: 'loan', value: -1 },
    ],
    [
      { type: 'cash', change: 0.3 },
      { type: 'credit', change: null },
    ],
  )
  expect(groups.map(({ type, label, value, change }) => [type, label, value, change])).toEqual([
    ['cash', 'Cash', 0.3, 0.3],
    ['brokerage', 'Brokerage', 0.25, null],
    ['credit', 'Credit cards', -20.1, null],
  ])
  expect(groups[0].accounts.map(({ id }) => id)).toEqual(['savings', 'checking'])
})

it('names each account type, and the spending cards apart', () => {
  expect(groupLabel('loan')).toBe('Loan')
  expect(groupLabel('spending')).toBe('Spending cards')
})
