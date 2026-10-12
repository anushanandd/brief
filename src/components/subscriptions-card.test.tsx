import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { insights } from '../data/fixtures/insights'
import type { FinanceSnapshot } from '../lib/schema'
import { SubscriptionsCard } from './subscriptions-card'

async function render(element: ReactNode) {
  const route = createRootRoute({ component: () => element })
  const router = createRouter({
    routeTree: route,
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  await router.load()
  return renderToStaticMarkup(<RouterProvider router={router} />)
}

const snapshot = (subscriptions?: FinanceSnapshot['insights']): FinanceSnapshot => ({
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
  transactions: [],
  insights: subscriptions,
})

it('lists native subscriptions with the native monthly total', async () => {
  const html = await render(
    <SubscriptionsCard
      data={snapshot(
        insights({
          subscriptions: {
            monthly: -20,
            items: [
              {
                recurringId: 'storage',
                title: 'Example Storage',
                amount: -120,
                monthlyAmount: -10,
                frequency: 'Yearly',
                nextDate: '2027-03-02',
                pending: false,
                sourceTransactionId: 'storage-1',
              },
            ],
          },
        }),
      )}
    />,
  )
  expect(html).toContain('Example Storage')
  expect(html).toContain('Yearly')
  expect(html).toContain('$20.00')
  expect(html).toContain('aria-label="Example Storage charge: −$120.00"')
})

it('asks for a refresh when native insights are unavailable', async () => {
  const html = await render(<SubscriptionsCard data={snapshot()} />)
  expect(html).toContain('Refresh to see subscriptions.')
})
