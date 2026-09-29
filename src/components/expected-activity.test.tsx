import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import type { FinanceSnapshot } from '../lib/schema'
import { ExpectedActivity, expectedActivityDateLabel } from './expected-activity'

afterEach(() => {
  vi.useRealTimers()
})

async function renderExpectedActivity(element: ReactNode) {
  const route = createRootRoute({ component: () => element })
  const router = createRouter({
    routeTree: route,
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  await router.load()
  return renderToStaticMarkup(<RouterProvider router={router} />)
}

it('uses relative labels for activity due within five days', () => {
  expect(expectedActivityDateLabel('2026-09-18', '2026-09-18')).toBe('today')
  expect(expectedActivityDateLabel('2026-09-19', '2026-09-18')).toBe('tomorrow')
  expect(expectedActivityDateLabel('2026-09-20', '2026-09-18')).toBe('in 2 days')
  expect(expectedActivityDateLabel('2026-09-23', '2026-09-18')).toBe('in 5 days')
  expect(expectedActivityDateLabel('2026-10-10', '2026-09-18')).toBe('Oct 10')
})

it('shows a matched pending amount without treating it as posted evidence', async () => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-09-18T12:00:00Z'))
  const posted = ['2026-06-18', '2026-07-18', '2026-08-18'].map((date, index) => ({
    id: `phone-${index}`,
    accountId: 'cash',
    account: 'Checking',
    merchant: 'Phone Utility',
    category: 'Utilities',
    date,
    amount: -100,
    pending: false,
    classification: classification('expense'),
  }))
  const data: FinanceSnapshot = {
    updatedAt: '2026-09-18T12:00:00Z',
    netWorth: 0,
    accounts: [],
    holdings: [],
    trades: [],
    spending: { monthTotal: 0, categories: [] },
    netWorthHistory: [],
    netWorthHistoryEstimated: false,
    benchmarkHistory: [],
    accountBalanceHistory: [],
    brokeragePerformance: [],
    accountMovements: [],
    possibleDuplicateAccounts: [],
    accountLinks: {},
    transactions: [
      ...posted,
      { ...posted[0], id: 'phone-pending', date: '2026-09-18', amount: -102, pending: true },
    ],
  }

  const html = await renderExpectedActivity(<ExpectedActivity data={data} />)
  expect(html).toContain('Pending')
  expect(html).toContain('aria-label="Pending amount: -$102.00"')
})

it('scopes recurring subscriptions to the selected account', async () => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-09-18T12:00:00Z'))
  const data: FinanceSnapshot = {
    updatedAt: '2026-09-18T12:00:00Z',
    netWorth: 0,
    accounts: [],
    holdings: [],
    trades: [],
    spending: { monthTotal: 0, categories: [] },
    netWorthHistory: [],
    netWorthHistoryEstimated: false,
    benchmarkHistory: [],
    accountBalanceHistory: [],
    brokeragePerformance: [],
    accountMovements: [],
    possibleDuplicateAccounts: [],
    accountLinks: {},
    transactions: ['selected', 'other'].flatMap((accountId) =>
      ['2026-06-20', '2026-07-20', '2026-08-20'].map((date) => ({
        id: `${accountId}-${date}`,
        accountId,
        account: accountId,
        merchant: `${accountId} streaming`,
        category: 'Subscription',
        date,
        amount: -15,
        pending: false,
        classification: classification('expense'),
      })),
    ),
  }
  const render = () => renderExpectedActivity(<ExpectedActivity data={data} account="selected" />)
  const html = await render()
  expect(html).toContain('selected streaming')
  expect(html).toContain('selected · Monthly subscription')
  expect(html).toContain('category-mark')
  expect(html).not.toContain('selected streaming transaction logo')
  const merchantLink = html.match(/<a [^>]*class="expected-activity-merchant"[^>]*>/)?.[0]
  expect(merchantLink).toContain('/activities?')
  expect(merchantLink).toContain('account=selected')
  expect(merchantLink).toContain('q=description%3Aselected')
  expect(merchantLink).toContain(
    'aria-label="View previous selected streaming activity in selected"',
  )
  expect(html).toContain('class="expected-activity-info"')
  expect(html).toContain('aria-label="Estimate details for selected streaming"')
  expect(html).toContain('class="financial-activity-copy"')
  expect(html).toContain('class="financial-activity-meta"')
  expect(html).toContain('<time dateTime="2026-09-20">in 2 days</time>')
  expect(html).toContain('class="expected-activity-amount negative"')
  expect(html).toContain('aria-label="Estimated amount: -$15.00"')
  expect(html).not.toContain('https://img.logo.dev')
  expect(html).not.toContain('Based on 3')
  expect(html).toContain('-$15.00')
  expect(html).not.toContain('No longer expected')
  expect(html).not.toContain('other streaming')

  vi.setSystemTime(new Date('2026-09-25T12:00:00Z'))
  expect(await render()).toContain('Refresh saved activity to check what is expected next.')
  expect(await render()).not.toContain('selected streaming')
})
