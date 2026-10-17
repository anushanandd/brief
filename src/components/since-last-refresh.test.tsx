import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { account, emptySnapshot } from '../data/fixtures/records'
import type { FinanceSnapshot, TransactionChange } from '../lib/schema'
import { hasRefreshChanges, SinceLastRefresh } from './since-last-refresh'

const change = (
  kind: TransactionChange['kind'],
  merchant: string,
  amount: number,
  extra: Partial<TransactionChange> = {},
): TransactionChange => ({
  id: `2026-10-11T15:00:00Z:${kind}:${merchant}`,
  observedAt: '2026-10-11T15:00:00Z',
  kind,
  transactionId: merchant,
  accountId: 'plaid:card',
  merchant,
  date: '2026-10-11',
  amount,
  pending: false,
  ...extra,
})

const snapshot = (lastChange?: FinanceSnapshot['lastChange']): FinanceSnapshot => ({
  ...emptySnapshot(),
  updatedAt: '2026-10-11T15:00:00Z',
  netWorth: 0,
  accounts: [
    account({
      id: 'plaid:card',
      name: 'Synthetic Card',
      institution: 'Bank',
      type: 'credit',
      value: -49,
    }),
  ],
  lastChange,
})

it('lists charges that settled differently or vanished, and counts the rest', () => {
  const data = snapshot({
    observedAt: '2026-10-11T15:00:00Z',
    previousUpdatedAt: '2026-10-10T15:00:00Z',
    previousNetWorth: 0,
    netWorthChange: -49,
    comparisonComplete: true,
    accountChanges: [{ accountId: 'plaid:card', name: 'Synthetic Card', change: -49 }],
    newTransactionIds: [],
    transactionChanges: [
      change('settled', 'Synthetic Bistro', -48, { previousAmount: -40 }),
      change('removed', 'Synthetic Fuel', -1, { pending: true }),
      change('added', 'Synthetic Store', -12),
      change('added', 'Synthetic Cafe', -4),
    ],
    backfilled: 3,
  })
  const html = renderToStaticMarkup(<SinceLastRefresh data={data} accountNames={{}} />)
  expect(html).toContain('Since last refresh')
  expect(html).toContain('across 1 account')
  expect(html).toContain('Synthetic Bistro')
  expect(html).toContain('Posted for a different amount')
  expect(html).toContain('Pending charge dropped')
  expect(html).toContain('2 new transactions')
  expect(html).toContain('3 older transactions arrived')
  // Amounts render through screen privacy's masked value.
  expect(html).toMatch(/was <span class="sensitive-value">/)
  expect(html).not.toContain('Synthetic Store')
})

it('is left out when the last refresh changed nothing', () => {
  const quiet = snapshot({
    observedAt: '2026-10-11T15:00:00Z',
    previousUpdatedAt: '2026-10-10T15:00:00Z',
    previousNetWorth: 0,
    netWorthChange: 0,
    accountChanges: [],
    newTransactionIds: [],
    transactionChanges: [],
    backfilled: 0,
  })
  expect(hasRefreshChanges(quiet)).toBe(false)
  expect(hasRefreshChanges(snapshot())).toBe(false)
  expect(renderToStaticMarkup(<SinceLastRefresh data={quiet} accountNames={{}} />)).toBe('')
})
