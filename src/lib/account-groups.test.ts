import { expect, it } from 'vitest'

import { account as record } from '../data/fixtures/records'
import { accountGroups, groupLabel } from './account-groups'
import type { Account } from './schema'

const account = (id: string, type: string, value: number | null): Account =>
  record({ id, name: id, institution: 'Example', type, value })

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
  expect(groups[0]?.accounts.map(({ id }) => id)).toEqual(['savings', 'checking'])
})

it('names each account type, and the spending cards apart', () => {
  expect(groupLabel('loan')).toBe('Loan')
  expect(groupLabel('spending')).toBe('Spending cards')
})

it('keeps the committed order while live values move accounts past each other', () => {
  const live = [account('a', 'brokerage', 9_900), account('b', 'brokerage', 10_050)]
  const committed = new Map([
    ['a', 10_000],
    ['b', 9_950],
  ])
  const order = (rank?: Map<string, number>) =>
    accountGroups(live, [{ type: 'brokerage', value: 19_950 }], [], rank)
      .flatMap(({ accounts }) => accounts)
      .map(({ id }) => id)
  expect(order(committed)).toEqual(['a', 'b'])
  expect(order()).toEqual(['b', 'a'])
})
