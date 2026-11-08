import { expect, it } from 'vitest'

import { account as record } from '../data/fixtures/records'
import type { AccountGroup } from '../lib/account-groups'
import { accountJumps } from './use-account-jump-actions'

const account = (id: string, type: string) =>
  record({ id, name: id, institution: 'Example', type, value: 100 })
const group = (type: string, label: string, ids: string[]): AccountGroup => ({
  type,
  label,
  accounts: ids.map((id) => account(id, type)),
  value: 100,
  change: null,
})

it('numbers each brokerage and cash account in menu order, with 0 for the total', () => {
  const jumps = accountJumps(
    [
      group('brokerage', 'Brokerage', ['taxable', 'roth']),
      group('cash', 'Cash', ['checking']),
      group('credit', 'Credit cards', ['card']),
    ],
    { roth: 'Roth IRA' },
  )
  expect(jumps.map(({ sequence, title, search }) => [sequence.join(''), title, search])).toEqual([
    ['b0', 'Open all brokerage accounts', { group: 'brokerage' }],
    ['b1', 'Open taxable', { account: 'taxable' }],
    ['b2', 'Open Roth IRA', { account: 'roth' }],
    // One cash account has no total row, so 0 opens the account itself.
    ['c0', 'Open all cash accounts', { account: 'checking' }],
    ['c1', 'Open checking', { account: 'checking' }],
  ])
})

it('stops at nine accounts a type', () => {
  const ids = Array.from({ length: 11 }, (_, index) => `cash-${index}`)
  const jumps = accountJumps([group('cash', 'Cash', ids)], {})
  expect(jumps.at(-1)?.sequence).toEqual(['c', '9'])
  expect(jumps).toHaveLength(10)
})
