import { expect, it } from 'vitest'

import { resolveBinding, type AppAction } from './action-system'
import {
  assignNumberKey,
  numberKeyDestinations,
  parseNumberKeys,
  resolveNumberKeys,
  withNumberKeys,
} from './number-keys'

const open = (id: string, sequence: string[] = []): AppAction => ({
  id,
  title: `Open ${id.split('.').at(-1)}`,
  scope: 'app',
  bindings: sequence.length ? [{ sequence, label: sequence.join(' ') }] : [],
  run: () => undefined,
})
const actions = [
  open('navigation.open./', ['Meta+1']),
  open('navigation.open./spending', ['Meta+2']),
  open('navigation.report.fees', ['g', 'r', 'e']),
  open('navigation.account.b1', ['b', '1']),
  open('settings.open', ['Meta+,']),
]
const account = (id: string, name: string) => ({ id, name, institution: 'Example Bank' })
const groups = [
  {
    type: 'brokerage',
    label: 'Brokerage',
    accounts: [account('acct-a', 'Taxable'), account('acct-b', 'Joint')],
  },
  { type: 'cash', label: 'Cash', accounts: [account('acct-c', 'Checking')] },
]
const destinations = numberKeyDestinations({
  actions,
  groups,
  names: { 'acct-a': 'Long term' },
  holdings: [{ ticker: 'ZZZT', name: 'Example Fund' }],
})

it('lists pages, reports, accounts, and holdings, and nothing else', () => {
  expect(destinations.map(({ value, section }) => `${section} ${value}`)).toEqual([
    'Pages page:navigation.open./',
    'Pages page:navigation.open./spending',
    'Reports page:navigation.report.fees',
    'Accounts group:brokerage',
    'Accounts account:acct-a',
    'Accounts account:acct-b',
    'Accounts group:cash',
    'Accounts account:acct-c',
    'Holdings ticker:ZZZT',
  ])
  // A name the user gave the account wins over the provider's.
  expect(destinations[4]?.title).toBe('Open Long term')
  expect(destinations[4]?.label).toBe('Long term · Example Bank')
  // A type with one account opens that account; one with several opens the type.
  expect(destinations[3]?.search).toEqual({ group: 'brokerage' })
  expect(destinations[6]?.search).toEqual({ account: 'acct-c' })
})

it('starts unassigned, and reads back only what is valid', () => {
  expect(parseNumberKeys(null)).toEqual({})
  expect(parseNumberKeys('not json')).toEqual({})
  expect(parseNumberKeys('null')).toEqual({})
  // The first version saved a list of action IDs, defaults included; it reads as unassigned.
  expect(
    parseNumberKeys(
      JSON.stringify(['navigation.open./', ...Array.from({ length: 9 }, () => null)]),
    ),
  ).toEqual({})
  const parsed = parseNumberKeys(
    JSON.stringify({
      1: { kind: 'ticker', ticker: 'ZZZT', name: 'Example Fund' },
      2: { kind: 'page', action: 'settings.open' },
      3: { kind: 'account' },
      4: { kind: 'group', group: 'cash' },
      x: { kind: 'ticker', ticker: 'ZZZT' },
    }),
  )
  // Only navigation actions may be pages; malformed entries and unknown keys are dropped.
  expect(parsed).toEqual({
    1: { kind: 'ticker', ticker: 'ZZZT', name: 'Example Fund' },
    4: { kind: 'group', group: 'cash' },
  })
})

it('binds a page to its action, and leaves the rest to actions of their own', () => {
  let choices = assignNumberKey({}, '3', destinations[2]?.target ?? null)
  choices = assignNumberKey(choices, '7', destinations[8]?.target ?? null)
  const resolved = resolveNumberKeys(choices, destinations)
  const bound = withNumberKeys(actions, resolved)
  expect(resolveBinding(bound, ['3']).exact?.action.id).toBe('navigation.report.fees')
  expect(bound[2]?.bindings?.[0]).toEqual({ sequence: ['3'], label: '3' })
  // A security has no action to bind to; the caller makes one.
  expect(resolved['7']?.target).toEqual({ kind: 'ticker', ticker: 'ZZZT', name: 'Example Fund' })
  expect(resolveBinding(bound, ['7']).exact).toBeUndefined()
  // Unassigned keys open nothing.
  expect(resolveBinding(bound, ['1']).exact).toBeUndefined()
  // Clearing a key removes it.
  expect(assignNumberKey(choices, '3', null)).toEqual({ 7: choices['7'] })
})

it('opens nothing for a destination that is gone', () => {
  const choices = {
    1: { kind: 'ticker', ticker: 'GONE', name: 'Sold Fund' },
    2: { kind: 'account', account: 'closed', name: 'Old savings' },
  } as const
  expect(resolveNumberKeys(choices, destinations)).toEqual({})
})

it('leaves a digit after a prefix to its sequence', () => {
  const resolved = resolveNumberKeys(
    { 1: { kind: 'page', action: 'navigation.open./spending' } },
    destinations,
  )
  const afterB = resolveBinding(withNumberKeys(actions, resolved), ['b', '1'])
  expect(afterB.exact?.action.id).toBe('navigation.account.b1')
  expect(afterB.candidates.map(({ action }) => action.id)).toEqual(['navigation.account.b1'])
})
