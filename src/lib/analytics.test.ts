import { expect, it } from 'vitest'

import {
  analyticsAccountMatches,
  analyticsGroupColors,
  analyticsSearch,
  emptyReport,
  noAnalyticsAccounts,
  reportRequest,
  toggleAnalyticsAccount,
  withoutReportView,
} from './analytics'

it('validates incoming chart and date state and preserves account scope', () => {
  const search = analyticsSearch({
    chart: 'interest',
    range: 'year',
    from: '2026-02-30',
    to: 'bad',
    account: 'cash',
  })
  expect(search).toEqual({
    chart: 'interest',
    range: 'year',
    account: 'cash',
    from: undefined,
    to: undefined,
  })
  expect(analyticsSearch({ range: 'day' }).range).toBe('day')
})

it('asks Rust for the window and accounts a search names, the month by default', () => {
  expect(reportRequest({})).toEqual({ range: 'month' })
  expect(reportRequest({ range: 'week' }, 'income')).toEqual({ chart: 'income', range: 'week' })
  expect(reportRequest({ from: '2026-08-10', to: '2026-08-12', account: 'cash,card' })).toEqual({
    range: 'month',
    from: '2026-08-10',
    to: '2026-08-12',
    accounts: ['cash', 'card'],
  })
  // An explicit empty choice is no account, never every account.
  expect(reportRequest({ account: noAnalyticsAccounts }).accounts).toEqual([])
})

it('keeps account checkbox choices exact through multiple and empty selections', () => {
  const available = ['cash', 'card']
  let scope = toggleAnalyticsAccount(undefined, 'cash', available)
  expect(analyticsAccountMatches(scope, 'cash')).toBe(false)
  expect(analyticsAccountMatches(scope, 'card')).toBe(true)
  scope = toggleAnalyticsAccount(scope, 'card', available)
  expect(scope).toBe(noAnalyticsAccounts)
  expect(analyticsAccountMatches(scope, 'card')).toBe(false)
  scope = toggleAnalyticsAccount(scope, 'cash', available)
  scope = toggleAnalyticsAccount(scope, 'card', available)
  expect(scope).toBe('cash,card')
  expect(analyticsAccountMatches(undefined, 'anything')).toBe(true)
})

it('reads nothing as a figure while Rust has no report', () => {
  expect(emptyReport('fees')).toMatchObject({
    chart: 'fees',
    total: null,
    previous: null,
    change: null,
    entries: [],
    buckets: [],
  })
})

it('assigns distinct colors from the complete chart domain, independent of record order', () => {
  const groups = [
    'Income',
    'Purchases',
    'Interest',
    'Dividends',
    'Fees',
    'Taxes',
    'Reimbursements',
    'Other',
    'Extra',
  ]
  const colors = analyticsGroupColors([...groups, 'Income'])
  expect(colors.size).toBe(groups.length)
  expect(new Set(colors.values()).size).toBe(groups.length)
  expect(analyticsGroupColors(groups.toReversed())).toEqual(colors)
})

it('keeps a report’s transactions view and group in the URL, and drops them between reports', () => {
  expect(analyticsSearch({ chart: 'income', view: 'transactions', group: 'Payroll' })).toEqual({
    chart: 'income',
    range: undefined,
    account: undefined,
    from: undefined,
    to: undefined,
    view: 'transactions',
    group: 'Payroll',
  })
  // A group means nothing without the transactions view, and unknown views are ignored.
  expect(analyticsSearch({ view: 'breakdown', group: 'Payroll' })).not.toHaveProperty('view')
  expect(analyticsSearch({ group: 'Payroll' })).not.toHaveProperty('group')
  expect(
    withoutReportView({ chart: 'fees', range: 'month', view: 'transactions', group: 'ATM' }),
  ).toEqual({ chart: 'fees', range: 'month' })
})
