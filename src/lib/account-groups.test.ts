import { expect, it } from 'vitest'

import { accountGroups, balanceSheet } from './account-groups'
import type { Account } from './schema'

const account = (id: string, type: string, value: number | null): Account => ({
  id,
  name: id,
  institution: 'Example',
  type,
  value,
})

it('totals accounts by type in minor units, largest group first', () => {
  const groups = accountGroups(
    [
      account('card', 'credit', -20.1),
      account('checking', 'cash', 0.1),
      account('savings', 'cash', 0.2),
      account('broker', 'brokerage', 0.25),
    ],
    [
      { accountId: 'checking', change: 0.1 },
      { accountId: 'savings', change: 0.2 },
      { accountId: 'card', change: -5 },
    ],
  )
  expect(groups.map(({ type, label, value }) => [type, label, value])).toEqual([
    ['cash', 'Cash', 0.3],
    ['brokerage', 'Brokerage', 0.25],
    ['credit', 'Credit cards', -20.1],
  ])
  expect(groups[0].accounts.map(({ id }) => id)).toEqual(['savings', 'checking'])
  expect(groups[0].change).toBe(0.3)
  expect(groups[2].change).toBe(-5)
})

it('leaves a total unavailable when any account in the group is unknown', () => {
  const [group] = accountGroups(
    [account('known', 'loan', 100), account('unknown', 'loan', null)],
    [{ accountId: 'known', change: 4 }],
  )
  expect(group.label).toBe('Loan')
  expect(group.value).toBeNull()
  expect(group.change).toBeNull()
})

it('splits assets from liabilities in minor units and flags accounts without a value', () => {
  expect(
    balanceSheet([
      account('checking', 'cash', 0.1),
      account('savings', 'cash', 0.2),
      account('card', 'credit', -20.1),
    ]),
  ).toEqual({ assets: 0.3, liabilities: -20.1, complete: true })
  expect(
    balanceSheet([account('checking', 'cash', 0.1), account('broker', 'brokerage', null)]),
  ).toEqual({ assets: 0.1, liabilities: 0, complete: false })
})
