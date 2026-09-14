import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router'
import { renderToString } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { router } from '../router'
import { accountIncomeBreakdown } from './account-detail'

const scenario = vi.hoisted(() => ({
  recovery: undefined as { message: string; canRestore: boolean } | undefined,
  annotationWarning: undefined as string | undefined,
  holdings: [] as Array<Record<string, unknown>>,
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
      netWorth: -2_100,
      netWorthHistory: [{ date: '2026-09-03', value: -2_100 }],
      spending: { monthTotal: 0, categories: [] },
      accounts: [
        {
          id: 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          type: 'credit',
          value: -1_200,
        },
        {
          id: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          type: 'credit',
          value: -900,
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
      accountBalanceHistory: [
        {
          accountId: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          currentValue: -900,
          historySource: 'transaction-derived',
          historyStart: '2026-09-01',
          performanceMethod: 'value-only',
          points: [
            { date: '2026-09-01', value: -800, netDeposits: null, sp500: null },
            { date: '2026-09-03', value: -900, netDeposits: null, sp500: null },
          ],
        },
        {
          accountId: 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          currentValue: -1_200,
          historySource: 'transaction-derived',
          historyStart: '2026-09-01',
          performanceMethod: 'value-only',
          points: [
            { date: '2026-09-01', value: -1_100, netDeposits: null, sp500: null },
            { date: '2026-09-03', value: -1_200, netDeposits: null, sp500: null },
          ],
        },
      ],
      brokeragePerformance: [],
      holdings: scenario.holdings,
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
  scenario.holdings = []
})

it('splits posted current-year dividends and interest without counting other income', () => {
  const transaction = {
    category: 'Income',
    date: '2026-08-21',
    amount: 10,
    account: 'Brokerage',
    accountId: 'brokerage',
    pending: false,
  }
  expect(
    accountIncomeBreakdown(
      [
        { ...transaction, id: 'dividend', merchant: 'Dividend · VTI', amount: 12.5 },
        { ...transaction, id: 'interest', merchant: 'Cash interest', amount: 3.25 },
        { ...transaction, id: 'salary', merchant: 'Salary', amount: 5_000 },
        { ...transaction, id: 'old', merchant: 'Dividend · VTI', date: '2025-12-31' },
        { ...transaction, id: 'pending', merchant: 'Interest', pending: true },
      ],
      '2026-09-03T12:00:00Z',
    ),
  ).toEqual({ dividends: 12.5, interest: 3.25 })
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
  const html = await renderRoute('/spending/transactions')
  expect(html).toContain('7<!-- --> of <!-- -->7<!-- --> transactions')
  expect(html.match(/class="spending-transaction-row"/g)).toHaveLength(7)
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('href="/spending"')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).not.toContain('href="/spending/transactions"')
  expect(sidebar).not.toContain('href="/spending/benefits"')
})

it('previews the complete spending story in one dedicated overview', async () => {
  const html = await renderRoute('/spending')
  expect(html).toContain('Spending by category')
  expect(html).toContain('Largest expense')
  expect(html).not.toContain('Current balance')
  expect(html).not.toContain('account-mark-credit')
  expect(html).not.toContain('Previous pace')
  expect(html).not.toContain('Daily pace')
  expect(html).not.toContain('vs comparable period')
  expect(html).toContain('Amex benefit tracker')
  expect(html).toContain('By benefit')
  expect(html).toContain('Matched credits in 2026')
  expect(html).toContain('7<!-- --> matched')
  expect(html).toContain('Latest activity')
  expect(html).not.toContain('View all')
  expect(html).toContain('This month')
  expect(html).not.toContain('period-control')
  expect(html).not.toContain('month-control')
  expect(html).not.toContain('aria-label="Previous month"')
  expect(html).not.toContain('aria-label="Next month"')
  expect(html).not.toContain('Possible match to review')
  expect(html).not.toContain('href="/spending/benefits"')
  expect(html).toContain('href="/spending/transactions"')
  expect(html.match(/class="spending-benefit-row"/g)).toHaveLength(11)
  expect(html.match(/class="financial-activity-row"/g)).toHaveLength(7)
  expect(html).toContain('Selected card · Credit')
  expect(html).toContain('<span class="financial-activity-meta"><small>Sep 3</small>')
  expect(html).not.toContain('Other account transaction')
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
  expect(html).toContain('Account start dates')
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
  expect(html).toContain('Monthly spending')
  expect(html).toContain('monthly-bar-chart')
  expect(html).not.toContain('>Income<')
  expect(html).not.toContain('>Dividends<')
  expect(html).not.toContain('>Interest<')
  expect(html).toContain('Latest activity')
  expect(html.match(/class="financial-activity-row"/g)).toHaveLength(7)
  expect(html).not.toContain('workspace-tabs')
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('href="/accounts"')
})

it('links account selections and defaults the workspace to all accounts', async () => {
  const allHtml = await renderRoute('/accounts')
  expect(allHtml).toContain('All accounts')
  expect(allHtml).toContain('href="/accounts?account=other"')
  expect(allHtml).not.toContain('href="/accounts?account=selected"')
  expect(allHtml).toContain('>Accounts</span><strong>1</strong>')
  expect(allHtml).toContain('Started')
  expect(allHtml.match(/class="account-switcher-button/g)).toHaveLength(2)

  const html = await renderRoute('/accounts?account=other')
  expect(html).not.toContain('Choose an account')
  expect(html).toContain('Other card')
  expect(html).toContain('Account details')
  expect(html).not.toContain('Monthly card spending')
  const chartStart = html.indexOf('account-overview-chart-card')
  const chart = html.slice(chartStart, html.indexOf('</section>', chartStart))
  const detailsStart = html.indexOf('account-overview-summary-card')
  const details = html.slice(detailsStart, html.indexOf('</section>', detailsStart))
  expect(chart).toContain('Started')
  expect(chart).toContain('Sep 1, 2026')
  expect(details).not.toContain('Started')
  expect(html).not.toContain('>Dividends<')
  expect(html).not.toContain('>Interest<')
  expect(html).not.toContain('Income &amp; sales')
  expect(html).not.toContain('View all')
  expect(html).toContain('Latest activity')
  expect(html).toContain('href="/activities?account=other"')
  expect(html.match(/account-preview-card/g)).toHaveLength(1)
  expect(html.match(/account-switcher-button active/g)).toHaveLength(1)
  expect(html).not.toContain('workspace-destination-card')
})

it('opens the complete activity ledger with the originating account selected', async () => {
  const html = await renderRoute('/activities?account=selected&category=Credit')
  expect(html).toContain('<h1>Activity</h1>')
  expect(html).not.toContain('activities · Page')
  expect(html).toContain('aria-label="Previous activity page"')
  expect(html).toContain('aria-label="Next activity page"')
  expect(html.match(/class="financial-activity-row"/g)).toHaveLength(7)
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('Search merchant, security or amount')
  expect(html).toContain('aria-label="Activity account"')
  expect(html).toContain('aria-label="Activity category"')
  expect(html).toContain('class="ledger-select-value">Selected card</span>')
  expect(html).toContain('class="ledger-select-value">Credit</span>')
})

it('filters the holdings ledger by account and category', async () => {
  scenario.holdings = [
    {
      ticker: 'AAA',
      name: 'Alpha fund',
      accountId: 'selected',
      shares: 2,
      price: 50,
      value: 100,
      costBasis: 80,
      unrealizedGain: 20,
      dailyChangePct: 1,
      weeklyChangePct: null,
      weeklyReferencePrice: null,
      weeklyReferenceDate: null,
      totalChangePct: 25,
      instrumentKind: 'equity',
      color: '#fff',
    },
    {
      ticker: 'BBB',
      name: 'Beta asset',
      accountId: 'other',
      shares: 1,
      price: 40,
      value: 40,
      costBasis: 50,
      unrealizedGain: -10,
      dailyChangePct: -1,
      weeklyChangePct: null,
      weeklyReferencePrice: null,
      weeklyReferenceDate: null,
      totalChangePct: -20,
      instrumentKind: 'crypto',
      color: '#fff',
    },
  ]

  const html = await renderRoute('/holdings?account=selected&category=equity')
  expect(html).toContain('<h1>Holdings</h1>')
  expect(html).not.toContain('of <!-- -->2<!-- --> holdings')
  expect(html).toContain('<strong>AAA</strong>')
  expect(html).not.toContain('<strong>BBB</strong>')
  expect(html).toContain('Search security, account or value')
  expect(html).toContain('aria-label="Holding account"')
  expect(html).toContain('aria-label="Holding category"')
  expect(html).toContain('class="ledger-select-value">Equity</span>')
})

it('links each Home ledger preview to its primary page', async () => {
  const html = await renderRoute('/')
  const main = html.slice(html.indexOf('<main'), html.indexOf('</main>'))
  expect(main).toContain('href="/accounts"')
  expect(main).toContain('href="/accounts/selected"')
  expect(main).toContain('href="/holdings"')
  expect(main).toContain('href="/activities"')
})

it.each([
  ['/accounts/investments', 'Combined portfolio'],
  ['/accounts/cash', 'Available cash'],
  ['/spending', 'Largest expense'],
  ['/spending/subscriptions', 'Possible subscriptions'],
  ['/activities/trades', 'Trade ledger'],
  ['/activities/changes', 'Latest net-worth change'],
])('opens the focused workspace route %s', async (path, expected) => {
  expect(await renderRoute(path)).toContain(expected)
})

it('keeps Settings in one scroll and Logs out of primary navigation', async () => {
  const html = await renderRoute('/settings')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).toContain('Spending')
  expect(sidebar).toContain('Accounts')
  expect(sidebar).toContain('href="/holdings"')
  expect(sidebar).toContain('href="/activities"')
  expect(sidebar).not.toContain('Analytics')
  expect(sidebar).not.toContain('Logs')
  expect(html).toContain('href="/logs"')
  expect(html).not.toContain('settings-section-nav')
  expect(html).not.toContain('Connections, display preferences')
})

it('asks for a spending account instead of falling back to another card', async () => {
  const html = await renderRoute('/spending/transactions', '')
  expect(html).toContain('Choose a spending account')
  expect(html).not.toContain('class="spending-transaction-row"')
})
