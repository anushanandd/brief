import { expect, it } from 'vitest'

import { ledgerView } from './ledger-view'

it('shows single figures and a short table exactly as native code returned them', () => {
  const view = ledgerView({
    name: 'spendingByCategory',
    arguments: { start: '2026-09-01', end: '2026-09-30' },
    result: {
      start: '2026-09-01',
      end: '2026-09-30',
      totalSpending: 1234.5,
      categories: Array.from({ length: 8 }, (_, index) => ({
        category: `Category ${index}`,
        spending: 100 + index,
        transactions: index,
      })),
    },
  })
  expect(view.title).toBe('Spending by category')
  expect(view.scope).toBe('2026-09-01 to 2026-09-30')
  expect(view.facts).toEqual([{ label: 'Total spending', text: '$1,234.50', sensitive: true }])
  expect(view.table?.columns).toEqual(['Category', 'Spending', 'Transactions'])
  expect(view.table?.rows).toHaveLength(6)
  expect(view.table?.rows[0]).toEqual([
    { text: 'Category 0', sensitive: false },
    { text: '$100.00', sensitive: true },
    { text: '0', sensitive: false },
  ])
  expect(view.table?.more).toBe(2)
})

it('names the subject, masks only amounts, and handles results without a list', () => {
  const merchant = ledgerView({
    name: 'merchantHistory',
    arguments: { merchant: 'corner cafe' },
    result: {
      merchant: 'corner cafe',
      from: '2025-10-01',
      transactions: 12,
      netAmount: -480,
      months: [],
    },
  })
  expect(merchant.scope).toBe('corner cafe · 2025-10-01')
  expect(merchant.facts).toEqual([
    { label: 'Merchant', text: 'corner cafe', sensitive: false },
    { label: 'Transactions', text: '12', sensitive: false },
    { label: 'Net amount', text: '−$480.00', sensitive: true },
  ])
  expect(merchant.table).toBeUndefined()

  const holdings = ledgerView({
    name: 'topHoldings',
    arguments: {},
    result: {
      holdings: [{ ticker: 'VTI', value: null, totalChangePercent: 12.345, account: 'A' }],
    },
  })
  expect(holdings.table?.rows[0]).toEqual([
    { text: 'VTI', sensitive: false },
    { text: '—', sensitive: false },
    { text: '12.35%', sensitive: false },
    { text: 'A', sensitive: false },
  ])
  expect(ledgerView({ name: 'somethingNew', arguments: {}, result: {} }).title).toBe(
    'Something new',
  )
})
