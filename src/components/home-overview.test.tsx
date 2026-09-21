import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import seed from '../data/seed.json'
import { financeSnapshotSchema } from '../lib/schema'
import { HomeOverview } from './home-overview'
vi.mock('../lib/api', () => ({
  getFoundationModelStatus: () => {
    throw new Error('Overview must not request an LLM')
  },
  generateFoundationExplanation: () => {
    throw new Error('Overview must not request an LLM')
  },
}))
async function renderOverview(children: ReactNode) {
  const router = createRouter({
    routeTree: createRootRoute({ component: () => children }),
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  await router.load()
  return renderToStaticMarkup(<RouterProvider router={router} />)
}
it('renders the four deterministic rows without a model or query provider', async () => {
  const data = financeSnapshotSchema.parse(seed)
  const end = Date.parse(data.updatedAt) / 1000
  const html = await renderOverview(
    <HomeOverview data={data} range={{ start: end - 7 * 86400, end }} allAccountsPoints={[]} />,
  )
  expect(html).toContain('Portfolio')
  expect(html).toContain('All accounts')
  expect(html).toContain('Spending')
  expect(html).toContain('Income')
  expect(html).toContain('vs')
})

it('shows posted spending and income totals', async () => {
  const data = financeSnapshotSchema.parse(seed)
  data.updatedAt = '2026-09-18T12:00:00Z'
  data.transactions = [
    {
      ...data.transactions[0],
      id: 'expense',
      accountId: 'card',
      merchant: 'Example Grocery Market',
      category: 'Groceries',
      amount: -50,
      postedOn: '2026-09-17',
      classification: classification('expense'),
      pending: false,
    },
    {
      ...data.transactions[0],
      id: 'income',
      accountId: 'cash',
      merchant: 'Example Payroll Deposit',
      category: 'Income',
      amount: 500,
      postedOn: '2026-09-17',
      classification: classification('income'),
      pending: false,
    },
  ]
  const end = Date.parse(data.updatedAt) / 1000
  const html = await renderOverview(
    <HomeOverview
      data={data}
      spendingAccountId="card"
      range={{ start: end - 7 * 86400, end }}
      allAccountsPoints={[]}
    />,
  )
  expect(html).toContain('$50.00')
  expect(html).toContain('$500.00')
})
