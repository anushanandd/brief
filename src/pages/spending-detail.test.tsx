import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { router } from '../router'

const scenario = vi.hoisted(() => ({
  recovery: undefined as { message: string; canRestore: boolean } | undefined,
  annotationWarning: undefined as string | undefined,
}))

vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
    integrationStatus: { plaid: false, snaptrade: false, alpaca: false },
    marketSymbols: ['SYNTH'],
    runtimeLog: [],
    annotationWarning: scenario.annotationWarning,
    data: {
      recovery: scenario.recovery,
      updatedAt: '2026-09-03T12:00:00Z',
      accounts: [
        {
          id: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          type: 'credit',
          value: -900,
        },
        {
          id: 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          type: 'credit',
          value: -1_200,
        },
      ],
      transactions: [
        ...Array.from({ length: 7 }, (_, index) => ({
          id: `transaction-${index}`,
          merchant: `Resy credit ${index}`,
          category: 'Credit',
          date: `2026-09-0${(index % 3) + 1}`,
          amount: 10,
          account: 'Selected card',
          accountId: 'selected',
          pending: false,
        })),
        {
          id: 'other',
          merchant: 'Other account transaction',
          category: 'Credit',
          date: '2026-09-01',
          amount: 900,
          account: 'Other card',
          accountId: 'other',
          pending: false,
        },
      ],
      trades: [],
      accountMovements: [],
      brokeragePerformance: [],
      holdings: [],
    },
  }),
}))
vi.mock('../hooks/use-refresh-finance', () => ({
  useAutoRefreshFinance: () => undefined,
  useRefreshFinance: () => () => Promise.resolve(),
  useFinanceRefreshState: () => false,
}))

afterEach(() => {
  vi.unstubAllGlobals()
  scenario.recovery = undefined
  scenario.annotationWarning = undefined
})

async function renderRoute(path: string, selectedAccount = 'selected') {
  const storage = {
    getItem: (key: string) => (key === 'brief:spending-account-id' ? selectedAccount : ''),
  }
  vi.stubGlobal('window', { localStorage: storage })
  vi.stubGlobal('localStorage', storage)
  router.update({ history: createMemoryHistory({ initialEntries: [path] }) })
  await router.load()
  return renderToString(
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
}

it('opens the full transaction route without truncating or mixing accounts', async () => {
  const html = await renderRoute('/activities/transactions')
  expect(html).toContain('7<!-- --> of <!-- -->7<!-- --> transactions')
  expect(html.match(/class="spending-transaction-row"/g)).toHaveLength(7)
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('href="/activities"')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).not.toContain('href="/activities/transactions"')
  expect(sidebar).not.toContain('href="/activities/benefits"')
})

it('opens benefit history for the saved account', async () => {
  const html = await renderRoute('/activities/benefits')
  expect(html).toContain('Matched credits')
  expect(html).toContain('$70.00')
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('By benefit')
  expect(html).toContain('Matched activity')
  expect(html.match(/Confirmed benefit reimbursement/g)).toHaveLength(7)
})

it('organizes Activity into focused views', async () => {
  const html = await renderRoute('/activities')
  expect(html).toContain('href="/activities/spending"')
  expect(html).toContain('href="/activities/subscriptions"')
  expect(html).toContain('href="/activities/trades"')
  expect(html).toContain('href="/activities/changes"')
  expect(html).toContain('href="/activities/benefits"')
  expect(html).toContain('Other account transaction')
  expect(html.match(/class="workspace-destination-card"/g)).toHaveLength(5)
  expect(html).not.toContain('A chronological record')
})

it('shows unknown quote timestamps instead of the saved snapshot time', async () => {
  const html = await renderRoute('/logs')
  expect(html).toContain('<strong>SYNTH</strong><time>Unknown</time>')
  expect(html).toContain('Source timestamps')
})

it('keeps Settings available when optional annotations fail', async () => {
  scenario.annotationWarning = 'Transaction annotations could not be loaded.'
  const html = await renderRoute('/settings')
  expect(html).toContain('Transaction annotations could not be loaded.')
  expect(html).toContain('Integrations')
})

it('opens explicit recovery without showing a fabricated empty finance page', async () => {
  scenario.recovery = { message: 'Synthetic damaged state', canRestore: false }
  const html = await renderRoute('/activities')
  expect(html).toContain('Local data needs recovery')
  expect(html).toContain('Start new snapshot')
  expect(html).not.toContain('Restore retained snapshot')
  expect(html).not.toContain('Other account transaction')
})

it('shows each account as a single card-based detail page', async () => {
  const html = await renderRoute('/accounts/selected')
  expect(html).toContain('Selected card')
  expect(html).toContain('Current provider balance')
  expect(html).toContain('This month')
  expect(html).toContain('Transactions')
  expect(html.match(/class="spending-transaction-row"/g)).toHaveLength(7)
  expect(html).not.toContain('workspace-tabs')
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('href="/accounts"')
})

it('opens the Accounts overview from primary navigation', async () => {
  const html = await renderRoute('/accounts')
  expect(html).toContain('Total net worth')
  expect(html).toContain('href="/accounts/investments"')
  expect(html).toContain('href="/accounts/cash"')
  expect(html).toContain('Cash &amp; cards')
  expect(html).toContain('Selected card')
  expect(html).not.toContain('Your balances, investments, cash, and cards in one place.')
})

it.each([
  ['/accounts/investments', 'Combined portfolio'],
  ['/accounts/cash', 'Available cash'],
  ['/activities/spending', 'Daily pace'],
  ['/activities/subscriptions', 'Possible subscriptions'],
  ['/activities/trades', 'Trade ledger'],
  ['/activities/changes', 'Latest net-worth change'],
])('opens the focused workspace route %s', async (path, expected) => {
  expect(await renderRoute(path)).toContain(expected)
})

it('keeps Settings in one scroll and Logs out of primary navigation', async () => {
  const html = await renderRoute('/settings')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).toContain('Activity')
  expect(sidebar).toContain('Accounts')
  expect(sidebar).not.toContain('Analytics')
  expect(sidebar).not.toContain('Logs')
  expect(html).toContain('href="/logs"')
  expect(html).not.toContain('settings-section-nav')
  expect(html).not.toContain('Connections, display preferences')
})

it('asks for a spending account instead of falling back to another card', async () => {
  const html = await renderRoute('/activities/transactions', '')
  expect(html).toContain('Choose a spending account')
  expect(html).not.toContain('class="spending-transaction-row"')
})
