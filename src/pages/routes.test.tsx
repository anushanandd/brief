import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { renderToString } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { formatUpdatedAt } from '../lib/format'
import { router } from '../router'

const scenario = vi.hoisted(() => ({
  recovery: undefined as { message: string; canRestore: boolean } | undefined,
  annotationWarning: undefined as string | undefined,
  holdings: [] as Array<Record<string, unknown>>,
  trades: [] as Array<Record<string, unknown>>,
  incomplete: false,
  providerError: false,
  singleHistory: false,
  homeHistory: false,
  accountType: 'credit',
  otherAccountType: 'credit',
  otherKind: 'other' as import('../lib/schema').Transaction['classification']['kind'],
  otherCategory: 'Credit',
  otherMerchant: 'Other account transaction',
  partialBasis: false,
  linkedPending: false,
  uberPurchase: false,
  hiddenBenefits: '',
  benefitAmount: 10,
  postingDate: undefined as string | undefined,
  activityMethod: undefined as string | undefined,
  snaptradeReference: false,
}))

vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
    integrationStatus: { plaid: false, snaptrade: false, alpaca: false },
    marketSymbols: ['SYNTH'],
    runtimeLog: [],
    annotationWarning: scenario.annotationWarning,
    data: {
      recovery: scenario.recovery,
      netWorthIncomplete: scenario.incomplete,
      providerStatus: scenario.providerError
        ? { plaid: { updatedAt: null, error: 'Synthetic provider error' } }
        : {},
      updatedAt: '2026-09-03T12:00:00Z',
      netWorth: scenario.snaptradeReference ? 300 : -2_100,
      benchmarkHistory: [],
      netWorthHistory: [{ date: '2026-09-03', value: scenario.snaptradeReference ? 300 : -2_100 }],
      spending: { monthTotal: 0, categories: [] },
      accounts: [
        {
          id: scenario.snaptradeReference ? 'snaptrade:selected' : 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          type: scenario.accountType,
          value: scenario.snaptradeReference ? 1_200 : -1_200,
          ...(scenario.snaptradeReference
            ? {
                balanceSource: 'cash-and-positions',
                cashValue: 200,
                investedValue: 1_000,
                reportedBalance: 900,
                balanceAsOf: '2026-09-02T20:00:00Z',
              }
            : {}),
          costBasisCoverage: scenario.partialBasis ? 'partial' : 'unavailable',
          knownCostBasis: scenario.partialBasis ? 100 : null,
          knownUnrealizedGain: scenario.partialBasis ? 0 : null,
        },
        {
          id: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          type: scenario.otherAccountType,
          value: -900,
        },
      ],
      transactions: [
        ...Array.from({ length: 7 }, (_, index) => ({
          id: `transaction-${index}`,
          ...(index === 0 && scenario.postingDate ? { postedOn: scenario.postingDate } : {}),
          ...(index === 0 && scenario.activityMethod
            ? { paymentChannel: scenario.activityMethod }
            : {}),
          ...(scenario.linkedPending && index === 0
            ? { website: 'https://example.com/', description: 'Synthetic provider description' }
            : {}),
          merchant: `Resy credit ${index}`,
          category: 'Credit',
          date: `2026-09-0${(index % 3) + 1}`,
          amount: scenario.benefitAmount,
          account: 'Selected card',
          accountId: 'selected',
          classification: classification('other', { credit: true }),
          pending: scenario.linkedPending && index === 0,
        })),
        ...(scenario.uberPurchase
          ? [
              {
                id: 'uber-purchase',
                merchant: 'Uber',
                category: 'Travel',
                date: '2026-09-02',
                amount: -5,
                account: 'Selected card',
                accountId: 'selected',
                classification: classification('expense'),
                pending: false,
              },
            ]
          : []),
        {
          id: 'other',
          merchant: scenario.otherMerchant,
          category: scenario.otherCategory,
          date: '2026-09-01',
          amount: 900,
          account: 'Other card',
          accountId: 'other',
          classification: classification(scenario.otherKind),
          pending: false,
        },
      ],
      trades: scenario.trades,
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
            ...(scenario.singleHistory
              ? []
              : [{ date: '2026-09-01', value: -1_100, netDeposits: null, sp500: null }]),
            { date: '2026-09-03', value: -1_200, netDeposits: null, sp500: null },
          ],
        },
      ],
      brokeragePerformance: scenario.homeHistory
        ? [
            {
              accountId: 'selected',
              name: 'Selected card',
              institution: 'Test Bank',
              currentValue: -1_200,
              points: [
                { date: '2026-09-01', value: -1_100, netDeposits: -1_100, sp500: -1_100 },
                { date: '2026-09-03', value: -1_200, netDeposits: -1_150, sp500: -1_050 },
              ],
            },
          ]
        : [],
      holdings: scenario.holdings,
    },
  }),
}))
vi.mock('../hooks/use-refresh-finance', () => ({
  useAutoRefreshFinance: () => undefined,
  useRefreshFinance: () => () => Promise.resolve(),
  useFinanceRefreshState: () => false,
}))

// Server rendering checks route output, not mounted effects or user interactions.
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-16T12:00:00Z'))
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  scenario.recovery = undefined
  scenario.annotationWarning = undefined
  scenario.holdings = []
  scenario.trades = []
  scenario.incomplete = false
  scenario.providerError = false
  scenario.singleHistory = false
  scenario.homeHistory = false
  scenario.accountType = 'credit'
  scenario.otherAccountType = 'credit'
  scenario.otherKind = 'other'
  scenario.otherCategory = 'Credit'
  scenario.otherMerchant = 'Other account transaction'
  scenario.partialBasis = false
  scenario.linkedPending = false
  scenario.uberPurchase = false
  scenario.hiddenBenefits = ''
  scenario.benefitAmount = 10
  scenario.postingDate = undefined
  scenario.activityMethod = undefined
  scenario.snaptradeReference = false
})

async function renderRoute(path: string, selectedAccount = 'selected') {
  const storage = {
    getItem: (key: string) =>
      key === 'brief:spending-account-id'
        ? selectedAccount
        : key === 'brief:hidden-platinum-benefits'
          ? scenario.hiddenBenefits
          : '',
  }
  vi.stubGlobal('window', { localStorage: storage })
  vi.stubGlobal('localStorage', storage)
  const testRouter = createRouter({
    routeTree: router.routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
  })
  await testRouter.load()
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  })
  try {
    return renderToString(
      <QueryClientProvider client={client}>
        <LiveMarketProvider>
          <RouterProvider router={testRouter} />
        </LiveMarketProvider>
      </QueryClientProvider>,
    )
  } finally {
    client.clear()
  }
}

it('keeps a short Platinum preview on Spending and links to Amex credits', async () => {
  const html = await renderRoute('/spending')
  expect(html).toContain('Statement estimate')
  expect(html).toContain('Daily average')
  expect(html).toContain('Credits earned')
  expect(html).toContain('Est. credits missed')
  expect(html).toContain('All-time largest')
  expect(html).toContain('Expected activity</h2>')
  expect(html.indexOf('expected-activity-card')).toBeGreaterThan(
    html.indexOf('spending-benefits-card'),
  )
  expect(html).toContain('class="analytics-plot spending-bar-chart"')
  expect(html).toContain('href="/analytics?chart=amex-credits"')
  const preview = html.slice(html.indexOf('class="surface-card spending-benefits-card"'))
  expect(preview.match(/class="spending-benefit-row"/g)).toHaveLength(3)
  expect(preview).toMatch(/class="benefit-allowance">\$[\d,.]+<\/strong>/)
  expect(preview).not.toMatch(/class="benefit-allowance">[^<]*\//)
  expect(preview).toContain('Resy')
  expect(preview).not.toContain('Global Entry / TSA PreCheck')
  expect(html.indexOf('spending-transactions-preview')).toBeLessThan(
    html.indexOf('spending-benefits-card'),
  )
  expect(html).toContain('Selected card · Credit')
  expect(html).not.toContain('Other account transaction')
})

it('places the spending category legend below the chart', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/spending')
  expect(html).toContain('>Travel</li>')
  expect(html.indexOf('aria-label="Chart legend"')).toBeGreaterThan(
    html.indexOf('class="analytics-plot spending-bar-chart"'),
  )
})

it('places current benefits below the Amex chart and credit activity below Overview', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&range=year')
  expect(html).toContain('Current benefit windows')
  expect(html).toContain('Credit activity')
  expect(html).toContain('>Overview</h2>')
  expect(html).toContain('Estimated remaining <!-- -->$30.00')
  expect(html).toContain('$70.00 of $100.00 detected')
  expect(html).toContain('aria-label="Resy credit progress"')
  expect(html).toContain('aria-label="About benefit tracking"')
  expect(html).toContain('aria-label="About Resy"')
  expect(html).toContain('class="benefit-allowance">$25 / month</strong>')
  expect(html).toContain('href="/settings#platinum-benefits"')
  expect(html.indexOf('analytics-chart-card')).toBeLessThan(html.indexOf('platinum-current-card'))
  expect(html.indexOf('>Overview</h2>')).toBeLessThan(html.indexOf('analytics-activities-card'))
  expect(html).not.toContain('Credit history')
  expect(html).not.toContain('Spending account</h2>')
  expect(html).not.toContain('Posted credits net of reversals · Uber Cash estimates excluded.')
  expect(html).not.toContain('aria-label="Chart accounts"')
  expect(html).not.toContain('Other account transaction')
})

it('keeps estimated Uber Cash separate from the posted credit total', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/analytics?chart=amex-credits&range=year')
  expect(html).toContain('$70.00')
  expect(html).toContain('$15.00')
  expect(html).toContain(
    'aria-label="Uber Cash credit progress" aria-valuetext="Estimated $15.00 used"',
  )
  expect(html).toContain('aria-label="Purchase evidence for Uber Cash"')
})

it('requires the saved spending account in the combined view', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits', '')
  expect(html).toContain('Choose a spending account')
  expect(html).not.toContain('id="benefit-resy"')
  expect(html).toContain('>Overview</h2>')
  expect(html).toContain('Credit activity')
})

it('registers data health without the removed Performance workspace', () => {
  expect(router.routesByPath['/health']).toBeDefined()
  expect('/performance' in router.routesByPath).toBe(false)
})

it('redirects the former Platinum page to Amex credits', () => {
  const route = router.routesByPath['/spending/platinum']
  let result: unknown
  try {
    Reflect.apply(route.options.beforeLoad!, undefined, [{}])
  } catch (error) {
    result = error
  }
  expect(result).toMatchObject({ options: { to: '/analytics', search: { chart: 'amex-credits' } } })
})

it('keeps current benefit windows independent of the chart period', async () => {
  vi.setSystemTime(new Date('2026-10-02T12:00:00Z'))
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('Ending <!-- -->Oct 31')
  expect(html).toContain('Ending <!-- -->Dec 31')
  expect(html).not.toContain('Ending <!-- -->Sep 30')
  expect(html).not.toContain('$30.00 remaining')
  expect(html).toContain('aria-label="Last detected credit:')
})

it('keeps hidden benefits in posted credit activity without showing current windows', async () => {
  scenario.hiddenBenefits = 'resy'
  scenario.linkedPending = true
  const html = await renderRoute('/analytics?chart=amex-credits&range=year')
  const current = html.slice(
    html.indexOf('class="surface-card platinum-current-card"'),
    html.indexOf('analytics-companion'),
  )
  expect(current).not.toContain('id="benefit-resy"')
  const activity = html.slice(html.indexOf('analytics-activities-card'))
  expect(activity).toContain('Resy credit 1')
  expect(activity).not.toContain('Resy credit 0')
})

it('keeps pending benefit evidence by its current benefit without counting it as posted', async () => {
  scenario.linkedPending = true
  const html = await renderRoute('/analytics?chart=amex-credits&range=year')
  const current = html.slice(
    html.indexOf('class="surface-card platinum-current-card"'),
    html.indexOf('analytics-companion'),
  )
  expect(current).toContain('Resy credit 0')
  expect(current).toContain('Benefit credit · Pending')
  expect(current).toContain('$60.00 of $100.00 detected')
  const activity = html.slice(html.indexOf('analytics-activities-card'))
  expect(activity).not.toContain('Resy credit 0')
  expect(activity).toContain('Resy credit 1')
})

it('shows unknown quote timestamps instead of the saved snapshot time', async () => {
  const html = await renderRoute('/logs')
  expect(html).toContain('<strong>SYNTH</strong><time>Unknown</time>')
  expect(html).toContain('Source timestamps')
})

it('keeps Settings available when optional annotations fail', async () => {
  scenario.annotationWarning = 'Transaction annotations could not be loaded.'
  const html = await renderRoute('/settings#data-sources')
  expect(html).toContain('Transaction annotations could not be loaded.')
  expect(html).toContain('Save and test')
  expect(html).toContain('aria-label="Start date for Selected card"')
  expect(html).toContain('id="data-sources" tabindex="-1"')
})

it('opens explicit recovery without showing a fabricated empty finance page', async () => {
  scenario.recovery = { message: 'Synthetic damaged state', canRestore: false }
  const html = await renderRoute('/activities')
  expect(html).toContain('Local data needs recovery')
  expect(html).toContain('Start new snapshot')
  expect(html).not.toContain('Restore retained snapshot')
  expect(html).not.toContain('Other account transaction')
})

it('links account selections and defaults the workspace to all accounts', async () => {
  scenario.otherAccountType = 'cash'
  const allHtml = await renderRoute('/accounts', '')
  expect(allHtml).toContain('All accounts')
  expect(allHtml).toContain('href="/accounts?account=other"')
  expect(allHtml).toContain('href="/accounts?account=selected"')
  expect(allHtml).toContain('<h2>Account details</h2>')
  expect(allHtml).toContain('<span>Cash</span>')
  expect(allHtml).toContain('<span>Stock</span>')
  expect(allHtml).toContain('<span>Known unrealized P/L</span>')
  expect(allHtml).toContain('<span>Estimated realized P/L</span>')
  expect(allHtml).toContain('<h2>Expected activity</h2>')
  expect(allHtml.match(/class="account-switcher-button/g)).toHaveLength(3)

  const html = await renderRoute('/accounts?account=other', '')
  expect(html).toContain('Other card')
  expect(html).toContain('Account details')
  expect(html).toContain('Recent activity')
  expect(html).toContain('href="/activities?account=other"')
  expect(html).toContain('account-preview-grid account-preview-grid-wide-recent')
  expect(html).toContain('<h2>Expected activity</h2>')
  expect(html.indexOf('Recent activity')).toBeLessThan(html.indexOf('<h2>Expected activity</h2>'))
  expect(html.match(/account-preview-card/g)).toHaveLength(2)
  expect(html.match(/account-switcher-button active/g)).toHaveLength(1)
})

it('shows known investment totals and signed trade gains in all-account details', async () => {
  scenario.accountType = 'brokerage'
  scenario.partialBasis = true
  scenario.trades = [
    {
      id: 'gain',
      type: 'SELL',
      date: '2026-09-03',
      account: 'Selected card',
      accountId: 'selected',
      amount: 25,
      estimatedRealizedGain: 5,
    },
  ]
  const html = await renderRoute('/accounts')
  expect(html).toContain('<span>Known unrealized P/L</span>')
  expect(html.replaceAll('<!-- -->', '')).toContain(
    'Selected card · <span class="positive">+$5.00</span>',
  )
  expect(html).not.toContain('Estimated FIFO P/L $5.00')
})

it('keeps the saved spending account out of the Accounts workspace', async () => {
  const html = await renderRoute('/accounts?account=selected')
  const page = html.slice(html.indexOf('class="page accounts-workspace-page"'))
  expect(page).toContain('href="/accounts?account=other"')
  expect(page).not.toContain('href="/accounts?account=selected"')
  expect(page).not.toContain('aria-current="page">Selected card')
})

it('offers CSV for matching Activity results and disables it for an empty date filter', async () => {
  const matching = await renderRoute('/activities?account=selected')
  const download = matching.match(
    /<button[^>]*aria-label="Download filtered activity as CSV"[^>]*>/,
  )?.[0]
  expect(download).toBeDefined()
  expect(download).not.toContain('disabled')
  const empty = await renderRoute('/activities?from=2099-01-01')
  expect(
    empty.match(/<button[^>]*aria-label="Download filtered activity as CSV"[^>]*>/)?.[0],
  ).toContain('disabled')
  expect(empty).toContain('No activity matches these filters.')
})

it('opens the complete activity ledger with the originating account selected', async () => {
  const html = await renderRoute(
    '/activities?account=selected&category=Credit&from=2026-09-01&to=2026-09-03',
  )
  expect(html).toContain('>Activity</a>')
  expect(html).toContain('role="region" aria-label="Activity results" tabindex="0"')
  expect(html.match(/<strong>Resy credit \d<\/strong>/g)).toHaveLength(7)
  expect(html).not.toContain('Other account transaction')
  expect(html).toContain('placeholder="Search activity"')
  expect(html).toContain('class="financial-activity-header"')
  expect(html).not.toContain('<h2>Activity</h2>')
  expect(html).toContain('aria-label="Sort by Account"')
  expect(html).toContain('aria-label="Sort by Category"')
  expect(html).toContain('aria-label="Sort by Location"')
  expect(html).not.toContain('aria-label="Sort by Method"')
  expect(html).toContain('aria-label="Filter by description:')
  expect(html).toContain('aria-label="Filter by account:')
  expect(html).toContain('aria-label="Filter by category:')
  expect(html).toContain('aria-label="Filter by date:')
  expect(html).toContain('Selected card')
  expect(html).toContain('aria-label="Filters, 3 active"')
  expect(html).toContain('aria-label="Activity date range"')
  expect(html).toContain('aria-label="Custom date range"')
  expect(html).toContain('aria-label="Start date"')
  expect(html.indexOf('aria-label="Activity date range"')).toBeLessThan(
    html.indexOf('aria-label="Download filtered activity as CSV"'),
  )
  expect(html).not.toContain('<legend>Dates</legend>')
  expect(html).toContain('aria-keyshortcuts="F Escape"')
  expect(html).toContain('aria-keyshortcuts="C"')
  expect(html).not.toContain('Latest day')
  expect(html).not.toContain('>↵</kbd>')
  expect(html).toContain('aria-label="Remove account filter: Selected card"')
  expect(html).toContain('aria-label="Remove category filter: Credit"')
  expect(html).toContain('aria-label="Remove date filter:')
  expect(html).toContain('Sep 1, 2026')
  expect(html).toContain('Sep 3, 2026')
  expect(html).not.toContain('>Today</h3>')
})

it('filters activity by method and shows its removable tag', async () => {
  scenario.activityMethod = 'online'
  const html = await renderRoute('/activities?method=Online')
  expect(html).toContain('aria-label="Filters, 1 active"')
  expect(html).toContain('aria-label="Remove method filter: Online"')
  expect(html).toContain('aria-label="Filter by method: Online"')
  expect(html).toContain('Resy credit 0')
  expect(html).not.toContain('Resy credit 1')
})

it('shows the selected holding without duplicating the portfolio holdings table', async () => {
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

  scenario.trades = [
    {
      id: 'buy-aaa',
      ticker: 'AAA',
      type: 'Buy',
      date: '2026-09-02',
      amount: -37,
      accountId: 'selected',
      account: 'Selected card',
      units: 1,
      price: 37,
    },
    {
      id: 'sell-bbb',
      ticker: 'BBB',
      type: 'Sell',
      date: '2026-09-01',
      amount: 29,
      accountId: 'other',
      account: 'Other account',
      units: 1,
      price: 29,
    },
  ]
  const html = await renderRoute('/holdings?account=selected&category=equity')
  const heading = html.slice(html.indexOf('<h1'), html.indexOf('</h1>'))
  expect(heading).toContain('href="/holdings"')
  expect(heading).toContain('aria-current="page">AAA')
  const selected = await renderRoute('/holdings?ticker=BBB')
  expect(selected.slice(selected.indexOf('<h1'), selected.indexOf('</h1>'))).toContain(
    'aria-current="page">BBB',
  )
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Security price range"/)
  const holdingChart = html.slice(html.indexOf('holding-chart-card'))
  const holdingSummary = holdingChart.slice(
    holdingChart.indexOf('chart-summary-row'),
    holdingChart.indexOf('</header>'),
  )
  expect(holdingSummary).not.toContain('chart-range-select')
  expect(holdingSummary).toContain('Range change unavailable')
  expect(holdingChart.slice(0, holdingChart.indexOf('chart-summary-row'))).toContain(
    'aria-label="Price chart details"',
  )
  expect(holdingChart.slice(0, holdingChart.indexOf('chart-summary-row'))).toContain(
    'chart-range-select',
  )
  expect(holdingChart).not.toContain('<details class="holding-chart-provenance">')
  expect(html).not.toContain('aria-label="Chart style"')
  expect(html).not.toContain('aria-label="Line chart"')
  expect(html).not.toContain('aria-label="Candlestick chart"')
  expect(html).toContain('Day')
  const selector = html.match(/<nav[^>]*aria-label="Held securities"[\s\S]*?<\/nav>/)?.[0] ?? ''
  expect(selector).toContain('href="/holdings?ticker=AAA"')
  expect(selector).toContain('href="/holdings?ticker=BBB"')
  expect(selector).toContain('aria-label="+1.00% daily change"')
  expect(selector).toContain('$100.00')
  expect(selected).toMatch(/aria-current="page"[^>]*href="\/holdings\?ticker=BBB"/)
  expect(html).toContain('<h2>Overview</h2>')
  expect(html).toContain('<strong>Bought AAA</strong>')
  expect(selected).toContain('<strong>Sold BBB</strong>')
  expect(html).toContain('class="financial-activity-row"')
  expect(html).toContain('+25.00%')
  expect(selected).toContain('-20.00%')
  expect(selected).not.toContain('position-table-market')
  expect(html).toContain('<h3>Earnings date</h3>')
  expect(html).toContain('71.4%')
  expect(selected).toContain('28.6%')
  expect(html).toContain('aria-label="AAA activity"')
  expect(html).toContain('-$37.00')
  expect(html).not.toContain('$29.00')
  expect(selected).toContain('aria-label="BBB activity"')
  expect(selected).toContain('$29.00')
  expect(selected).not.toContain('-$37.00')

  scenario.holdings.push({ ...scenario.holdings[0], accountId: 'other', value: 50 })
  scenario.holdings[1].value = 120
  const combined = await renderRoute('/holdings')
  const cards = combined.match(/<nav[^>]*aria-label="Held securities"[\s\S]*?<\/nav>/)?.[0] ?? ''
  expect(cards.match(/href="\/holdings\?ticker=AAA"/g)).toHaveLength(1)
  expect(cards).toContain('$150.00')
  expect(cards.indexOf('ticker=AAA')).toBeLessThan(cards.indexOf('ticker=BBB'))
})

it('links each Home ledger preview to its primary page', async () => {
  const html = await renderRoute('/')
  const main = html.slice(html.indexOf('<main'), html.indexOf('</main>'))
  expect(main).toContain('href="/accounts"')
  expect(main).toContain('href="/accounts?account=selected"')
  expect(main).toContain('href="/holdings"')
  expect(main).toContain('href="/activities"')
  expect(main.match(/<h2>Overview<\/h2>/g)).toHaveLength(1)
  expect(main.match(/class="financial-activity-row/g)).toHaveLength(2)
  expect(main).not.toContain('home-finance-grid')
  expect(main).not.toContain('class="treemap"')
})

it('keeps the Midday studies separate while preserving Activity filters', async () => {
  const design = await renderRoute('/settings/design')
  expect(design).toContain('href="/settings/design/midday/home"')
  expect(design).toContain('href="/settings/design/midday/activity"')

  const home = await renderRoute('/settings/design/midday/home')
  expect(home).toContain('class="app-frame midday-prototype"')
  expect(home).toContain('class="page dashboard-page"')
  expect(home).toContain('href="/settings/design"')

  const activity = await renderRoute('/settings/design/midday/activity?account=selected')
  expect(activity).toContain('class="page spending-detail-page ledger-page activity-ledger-page"')
  expect(activity).toContain('Resy credit')
  expect(activity).not.toContain('Other account transaction')
  expect(activity).toContain('href="/settings/design/midday/activity"')
})

it('shows labeled account-position dots beneath the Home chart', async () => {
  scenario.accountType = 'brokerage'
  const html = await renderRoute('/')
  expect(html).toContain('class="home-account-dots"')
  expect(html).toContain('aria-label="Chart account"')
  expect(html).toContain('aria-label="Show Selected card chart"')
  expect(html).toContain('aria-pressed="true"')
})

it('places graph ranges at the top right, above the change row', async () => {
  scenario.accountType = 'brokerage'
  scenario.homeHistory = true
  for (const path of ['/', '/accounts?account=selected']) {
    const html = await renderRoute(path, '')
    const chart = html.slice(html.indexOf('home-balance-header'))
    const headerEnd = chart.indexOf('</header>')
    const summary = chart.slice(chart.indexOf('chart-summary-row'), headerEnd)
    expect(summary).toContain('hero-change')
    expect(summary).not.toContain('chart-range-select')
    expect(chart.slice(0, chart.indexOf('chart-summary-row'))).toContain('chart-range-select')
    expect(chart.slice(headerEnd, chart.indexOf('brokerage-chart-viewport'))).not.toContain(
      'chart-range-select',
    )
    expect(chart).not.toContain('aria-label="Chart key"')
  }
  for (const [path, heading] of [
    ['/analytics?chart=income', 'home-balance-header'],
    ['/spending', 'workspace-brief-heading'],
  ]) {
    if (path === '/spending') scenario.accountType = 'credit'
    const html = await renderRoute(path)
    expect(html).toContain(heading)
    const chart = html.slice(html.indexOf(heading))
    const summary = chart.slice(chart.indexOf('chart-summary-row'), chart.indexOf('</header>'))
    expect(summary).toContain('hero-change')
    if (path === '/analytics?chart=income') {
      expect(summary).not.toContain('chart-range-select')
      expect(chart.slice(0, chart.indexOf('chart-summary-row'))).toContain('chart-range-select')
    } else {
      expect(chart.slice(0, chart.indexOf('</header>'))).toContain('chart-range-select')
    }
  }
})

it('limits Home recent activity to the number of accounts in its card', async () => {
  const html = await renderRoute('/')
  const accounts = html.slice(
    html.indexOf('accounts-overview-card'),
    html.indexOf('home-activity-card'),
  )
  const activity = html.slice(html.indexOf('home-activity-card'))
  expect(accounts.match(/class="account-row"/g)).toHaveLength(2)
  expect(activity.match(/class="financial-activity-row"/g)).toHaveLength(2)
})

it('keeps account details out of the organized sidebar', async () => {
  const html = await renderRoute('/settings')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).toContain('href="/holdings"')
  expect(sidebar).toContain('href="/activities"')
  expect(sidebar).toContain('href="/health"')
  expect(sidebar).not.toContain('href="/performance"')
  expect(sidebar).toContain('href="/accounts"')
  expect(sidebar).not.toContain('href="/accounts/investments"')
  expect(sidebar).not.toContain('href="/accounts/cash"')
  expect(sidebar).not.toContain('href="/accounts?account=')
  expect(sidebar).not.toContain('Test Bank')
  expect(sidebar).not.toContain('Other Bank')
  expect(sidebar).not.toContain('href="/spending/platinum"')
  expect(sidebar).toContain('href="/analytics?chart=amex-credits"')
  for (const chart of [
    'income',
    'amex-credits',
    'dividends',
    'interest',
    'fees',
    'realized',
    'cash-flow',
  ]) {
    expect(sidebar).toContain(`href="/analytics?chart=${chart}"`)
  }
  expect(sidebar).toContain('href="/settings/design"')
  expect(sidebar).toContain('href="/logs"')
  expect(sidebar).toContain('Diagnostics')
  expect(html).toContain('Live price updates')
  expect(html).toContain('Portable financial backup')
  expect(html).toContain('Backup files contain unencrypted financial data')
})

it('asks for a spending account instead of falling back to another card', async () => {
  const html = await renderRoute('/spending', '')
  expect(html).toContain('Choose a spending account')
  expect(html).toContain('href="/settings"')
  expect(html).not.toContain('class="financial-activity-row"')
})

it('keeps the account selector to one horizontally scrollable row', async () => {
  const html = await renderRoute('/accounts')
  expect(html).toContain('class="account-switcher account-switcher-single-row"')
  expect(html).toContain('class="account-switcher-grid account-switcher-grid-single-row"')
})

it('keeps incomplete aggregate balances qualified in the account overview', async () => {
  scenario.incomplete = true
  const html = await renderRoute('/accounts')
  expect(html).toContain('known USD balances only')
  expect(html).toContain('Complete balance unavailable')
  expect(html).not.toContain('live-performance-chart')
})

it('treats one historical observation consistently in the account workspace', async () => {
  scenario.accountType = 'cash'
  scenario.singleHistory = true
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('Historical series unavailable')
  expect(html).not.toContain('live-performance-chart')
})

it('qualifies partial unrealized gain in the investment account workspace', async () => {
  scenario.accountType = 'brokerage'
  scenario.partialBasis = true
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('<span>Market change</span>')
  expect(html).toContain('Known total unrealized')
  expect(html).toContain('account-preview-grid account-preview-grid-wide-primary')
  expect(html).toContain('Recent activity')
  expect(html).not.toContain('<h2>Expected activity</h2>')
  expect(html).toContain('class="muted">$0.00')
  expect(html).toContain('<span>Estimated realized P/L</span>')
})

it('preserves merchant links and pending status in the spending preview', async () => {
  scenario.linkedPending = true
  const html = await renderRoute('/spending')
  expect(html).toContain('href="https://example.com/"')
  expect(html).toContain('Pending')
})

it('shows a saved date and provider warning without claiming fresh data', async () => {
  scenario.providerError = true
  const html = await renderRoute('/holdings')
  expect(html).toContain(
    `Saved <time dateTime="2026-09-03T12:00:00Z">${formatUpdatedAt('2026-09-03T12:00:00Z')}</time>`,
  )
  expect(html).toContain('Data needs attention')
  expect(html).toContain('snapshot-freshness warning')
  expect(html).toContain('dateTime="2026-09-03T12:00:00Z"')
})

it('points Diagnostics back to Settings', async () => {
  const html = await renderRoute('/logs')
  const header = html.slice(html.indexOf('<header class="page-header'), html.indexOf('</header>'))
  expect(header).toMatch(/href="\/settings"[^>]*>Settings<\/a>/)
  expect(header).toContain('aria-current="page">Diagnostics')
})

it('uses the shared Spending range control and keyboard period navigation', async () => {
  const html = await renderRoute('/spending')
  const spendingPanel = html.slice(
    html.indexOf('<section class="spending-total-panel"'),
    html.indexOf('</section>', html.indexOf('<section class="spending-total-panel"')),
  )
  const spendingHeader = html.slice(
    html.indexOf('<header class="workspace-brief-heading"'),
    html.indexOf('</header>', html.indexOf('<header class="workspace-brief-heading"')),
  )
  expect(spendingHeader).toContain('class="hero-change"')
  expect(spendingHeader).toContain('ledger-select-trigger chart-range-select')
  expect(spendingPanel).toContain('aria-keyshortcuts="S W M Q Y A ArrowLeft ArrowRight"')
  expect(html).not.toContain('aria-label="Previous spending period"')
  expect(html).not.toContain('aria-label="Next spending period"')
  expect(spendingPanel).toMatch(
    /role="combobox"[^>]*aria-label="Spending range\. Active period: This month"/,
  )
  expect(spendingPanel).toContain('Statement')
})

it('renders supported history with accessible range controls in the account workspace', async () => {
  scenario.accountType = 'cash'
  const html = await renderRoute('/accounts?account=selected', '')
  const chartHeader = html.slice(
    html.indexOf('home-balance-header'),
    html.indexOf('</header>', html.indexOf('home-balance-header')),
  )
  expect(html).toContain('live-performance-chart')
  expect(chartHeader).toContain('class="chart-summary-row"')
  expect(chartHeader).toContain('class="hero-change"')
  expect(chartHeader).toContain('ledger-select-trigger chart-range-select')
  expect(chartHeader.indexOf('chart-range-select')).toBeLessThan(
    chartHeader.indexOf('chart-summary-row'),
  )
  expect(chartHeader).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  expect(chartHeader).toContain('Week')
  expect(html).toContain('role="status">Chart range: <!-- -->Week</span>')
  expect(html).not.toContain('Historical series unavailable')
})

it('offers account activity CSV downloads', async () => {
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('aria-label="Download account activity as CSV"')
})

it('combines account and category filters without dropping selected filters', async () => {
  scenario.otherCategory = 'Food'
  const byCategory = await renderRoute('/activities?category=Food')
  expect(byCategory).toContain('aria-label="Filters, 1 active"')
  expect(byCategory).toContain('aria-label="Remove category filter: Food"')
  expect(byCategory).toContain('Other account transaction')
  expect(byCategory).not.toContain('Resy credit 0')

  const byAccount = await renderRoute('/activities?account=selected')
  expect(byAccount).toContain('aria-label="Filters, 1 active"')
  expect(byAccount).toContain('aria-label="Remove account filter: Selected card"')

  const incompatible = await renderRoute('/activities?account=selected&category=Food')
  expect(incompatible).toContain('aria-label="Filters, 2 active"')
  expect(incompatible).toContain('aria-label="Remove account filter: Selected card"')
  expect(incompatible).toContain('aria-label="Remove category filter: Food"')
  expect(incompatible).toContain('No activity matches these filters.')
})

it('shows readable Activity and account breadcrumbs with links to broader selections', async () => {
  const activity = await renderRoute(
    '/activities?account=selected&category=Credit&from=2026-09-01&to=2026-09-03',
  )
  const heading = activity.slice(activity.indexOf('<h1'), activity.indexOf('</h1>'))
  expect(heading).toContain('href="/activities"')
  expect(heading).toContain('href="/activities?account=selected"')
  expect(heading).toContain('href="/activities?account=selected&amp;category=Credit"')
  expect(heading).toContain('Selected card')
  expect(heading).toContain('aria-current="page">Sep 1, 2026 – Sep 3, 2026')

  scenario.otherAccountType = 'cash'
  const account = await renderRoute('/accounts?account=other')
  const accountHeading = account.slice(account.indexOf('<h1'), account.indexOf('</h1>'))
  expect(accountHeading).toContain('href="/accounts"')
  expect(accountHeading).toContain('aria-current="page">Other card')
})

it('opens a description search from the Activity URL', async () => {
  scenario.linkedPending = true
  const html = await renderRoute('/activities?q=description%3Aprovider')
  expect(html).toContain('value="description:provider"')
  expect(html).toContain('<strong>Resy credit 0</strong>')
  expect(html).not.toContain('<strong>Resy credit 1</strong>')
})

it('opens the selected analytics chart with shared account and date scope', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('aria-label="Analytics charts"')
  expect(html).toContain('aria-keyshortcuts="ArrowLeft ArrowRight"')
  expect(html).toContain('aria-current="page">Amex credits')
  expect(html).toContain('$70.00')
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Analytics period"/)
  expect(html).toContain('Custom')
  expect(html).toContain('Sep 1, 2026')
  expect(html).toContain('chart=interest')
  expect(html).toContain('from=2026-09-01')
  expect(html).toContain('analysis=amex-credits')
  const scoped = await renderRoute('/analytics?chart=amex-credits&account=other&range=year')
  expect(scoped).toContain('Amex credits use the saved spending account.')
})

it('opens Cash flow by default and exposes Analytics order in Settings', async () => {
  const analytics = await renderRoute('/analytics')
  expect(analytics).toContain('aria-current="page">Cash flow')
  const settings = await renderRoute('/settings')
  expect(settings).toContain('Analytics order')
  expect(settings).toContain('aria-label="Move Cash flow chart down"')
})

it('keeps an unconfigured Amex chart unavailable and preserves unavailable account scope', async () => {
  expect(await renderRoute('/analytics?chart=amex-credits', '')).toContain(
    'Choose a spending account',
  )
  expect(await renderRoute('/analytics?chart=income&account=unknown')).toContain(
    'This account is unavailable',
  )
})

it('opens the exact Amex evidence in Activity and keeps empty account scopes empty', async () => {
  const html = await renderRoute('/activities?analysis=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('Resy credit 0')
  expect(html).not.toContain('<strong>Other account transaction</strong>')
  expect(html).toContain('aria-label="Remove analytics filter:')
  const other = await renderRoute('/activities?analysis=amex-credits&account=other')
  expect(other).toContain('No activity matches these filters.')
  const unknown = await renderRoute('/activities?analysis=amex-credits&account=missing')
  expect(unknown).toContain('No activity matches these filters.')
})

it('links existing account metrics and the Spending preview to Analytics', async () => {
  scenario.accountType = 'brokerage'
  const accounts = await renderRoute('/accounts?account=selected', '')
  expect(accounts).toContain(
    'href="/analytics?chart=dividends&amp;range=week&amp;account=selected"',
  )
  expect(accounts).toContain('href="/analytics?chart=interest&amp;range=week&amp;account=selected"')
  scenario.accountType = 'credit'
  const spending = await renderRoute('/spending')
  expect(spending).toContain('href="/analytics?chart=amex-credits"')
})

it('shows the dated SnapTrade total on individual brokerage graphs only', async () => {
  scenario.accountType = 'brokerage'
  scenario.snaptradeReference = true

  const all = await renderRoute('/accounts')
  expect(all).not.toContain('SnapTrade:')
  expect(all).not.toContain('Correct history')

  const account = await renderRoute('/accounts?account=snaptrade%3Aselected')
  const accountText = account.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ')
  const chartHeader = account.slice(
    account.indexOf('home-balance-header'),
    account.indexOf('</header>', account.indexOf('home-balance-header')),
  )
  expect(chartHeader).toContain('SnapTrade:')
  expect(chartHeader).not.toContain('chart-range-select')
  expect(accountText).toContain('SnapTrade: $900.00 (Sep 2, 2026)')
  expect(accountText).toContain('Cash$200.00')
  expect(accountText).toContain('Total unrealized')
  expect(accountText).not.toContain('cash + positions')
  expect(accountText).not.toContain('Correct history')

  const home = await renderRoute('/')
  const homeText = home.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ')
  expect(homeText).toContain('SnapTrade: $900.00 (Sep 2, 2026)')
})

it('uses the chart’s posted date when opening Activity evidence', async () => {
  scenario.postingDate = '2026-08-31'
  const posted = await renderRoute(
    '/activities?analysis=amex-credits&from=2026-08-31&to=2026-08-31',
  )
  expect(posted).toContain('<strong>Resy credit 0</strong>')
  expect(posted).not.toContain('<strong>Resy credit 1</strong>')
  const original = await renderRoute('/activities?from=2026-08-31&to=2026-08-31')
  expect(original).not.toContain('<strong>Resy credit 0</strong>')
})

it('uses only the saved spending account for Amex credits', async () => {
  const single = await renderRoute('/analytics?chart=amex-credits&account=other&range=month')
  expect(single).toContain('Amex credits use the saved spending account.')
  expect(single).toContain('Clear account filter')
  expect(single).not.toContain('aria-label="Chart accounts"')
  const multiple = await renderRoute(
    '/analytics?chart=amex-credits&account=selected,other&range=month',
  )
  expect(multiple).toContain('$70.00')
  expect(multiple).toContain('Selected card')
  const activity = await renderRoute('/activities?analysis=amex-credits&account=selected,other')
  expect(activity).toContain('Resy credit 0')
  const empty = await renderRoute('/analytics?chart=amex-credits&account=__none__')
  expect(empty).toContain('Amex credits use the saved spending account.')
  const unknown = await renderRoute('/analytics?chart=amex-credits&account=missing')
  expect(unknown).toContain('Clear account filter')
  expect(await renderRoute('/activities?analysis=amex-credits&account=__none__')).toContain(
    'No activity matches these filters.',
  )
})

it('formats analytics income with the shared merchant title and normal activity details', async () => {
  scenario.otherKind = 'income'
  scenario.otherCategory = 'Income'
  scenario.otherMerchant = 'Example AI Sol EXAMPLE'
  const html = await renderRoute('/analytics?chart=income')
  expect(html).toContain('<strong>Example AI</strong>')
  expect(html).toContain('category-mark')
  expect(html).toContain('Other card · Income')
  expect(html).toContain('$900.00')
})

it('keeps relevant zero-net and unavailable accounts but hides accounts outside the period', async () => {
  scenario.trades = [
    {
      id: 'gain',
      type: 'SELL',
      date: '2026-09-01',
      account: 'Selected card',
      accountId: 'selected',
      amount: 20,
      estimatedRealizedGain: 5,
    },
    {
      id: 'loss',
      type: 'SELL',
      date: '2026-09-02',
      account: 'Selected card',
      accountId: 'selected',
      amount: 10,
      estimatedRealizedGain: -5,
    },
    {
      id: 'unknown',
      type: 'SELL',
      date: '2026-09-03',
      account: 'Other card',
      accountId: 'other',
      amount: 15,
    },
  ]
  const html = await renderRoute('/analytics?chart=realized&account=__none__')
  const rows =
    html.match(/<label class="analytics-breakdown-row analytics-account-row"[\s\S]*?<\/label>/g) ??
    []
  expect(rows).toHaveLength(2)
  expect(rows.find((row) => row.includes('Selected card'))).toContain('$0.00')
  expect(rows.find((row) => row.includes('Other card'))).toContain('—')
  expect(rows.every((row) => !row.includes('checked=""'))).toBe(true)
  const earlier = await renderRoute('/analytics?chart=realized&from=2026-09-01&to=2026-09-02')
  const earlierRows =
    earlier.match(
      /<label class="analytics-breakdown-row analytics-account-row"[\s\S]*?<\/label>/g,
    ) ?? []
  expect(earlierRows).toHaveLength(1)
  expect(earlierRows[0]).toContain('Selected card')
})

it('shows scoped interest activity beneath its chart and account type subtext', async () => {
  scenario.otherKind = 'income'
  scenario.otherCategory = 'Income'
  scenario.otherKind = 'interest'
  scenario.otherMerchant = 'Interest payment'
  const html = await renderRoute('/analytics?chart=interest&account=other')
  const activityCard = html.slice(
    html.indexOf('analytics-activities-card'),
    html.indexOf('analytics-companion'),
  )
  expect(activityCard).toMatch(/<h2><a[^>]*href="\/activities\?[^"]*"[^>]*>Activities<\/a><\/h2>/)
  expect(activityCard).toContain('<strong>Interest payment</strong>')
  expect(activityCard).toContain('$900.00')
  expect(activityCard).not.toContain('Resy credit')
  expect(activityCard).toContain('analysis=interest')
  expect(activityCard).toContain('account=other')
  const rows =
    html.match(/<label class="analytics-breakdown-row analytics-account-row"[\s\S]*?<\/label>/g) ??
    []
  expect(rows).toHaveLength(1)
  expect(rows[0]).toContain('<strong>Other card</strong><small>credit</small>')
})

it('lists each matching Amex activity separately in Analytics', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  const activityCard = html.slice(html.indexOf('analytics-activities-card'))
  for (let index = 0; index < 7; index++) {
    expect(activityCard).toContain(`<strong>Resy credit ${index}</strong>`)
  }
  expect(activityCard).toContain('Selected card · Credit')
  expect(activityCard).not.toContain('Other account transaction')
  expect(activityCard.match(/class="financial-activity-row"/g)).toHaveLength(7)
})

it('groups Analytics activity by date and places cash-flow context in its columns', async () => {
  scenario.otherKind = 'income'
  scenario.otherCategory = 'Income'
  const html = await renderRoute('/analytics?chart=cash-flow')
  const primary = html.slice(html.indexOf('analytics-primary'), html.indexOf('analytics-companion'))
  expect(primary.indexOf('cash-flow-sankey')).toBeGreaterThan(0)
  expect(primary.indexOf('cash-flow-sankey')).toBeLessThan(
    primary.indexOf('analytics-activities-card'),
  )
  expect(primary).not.toContain('analytics-period-plot')
  expect(primary).toContain('Last week')
  const companion = html.slice(html.indexOf('analytics-companion'))
  expect(companion).not.toContain('<h2>Expected activity</h2>')
})

it('attaches Analytics tooltip triggers to dated categories and provides a legend', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  const chart = html.slice(
    html.indexOf('aria-label="Period totals"'),
    html.indexOf('analytics-activities-card'),
  )
  const triggers = chart.match(/<a\b[^>]*data-slot="tooltip-trigger"[^>]*>/g) ?? []
  expect(chart).toContain('aria-label="Chart legend"')
  expect(chart.indexOf('aria-label="Chart legend"')).toBeGreaterThan(
    chart.indexOf('aria-label="Period totals"'),
  )
  expect(triggers).toHaveLength(3)
  expect(triggers[0]).toContain('Sep 1, 2026 through Sep 1, 2026: $30.00')
  expect(triggers[1]).toContain('Sep 2, 2026 through Sep 2, 2026: $20.00')
  expect(
    triggers.every(
      (trigger) =>
        trigger.includes('analytics-bar-category') && trigger.includes('analysis=amex-credits'),
    ),
  ).toBe(true)
})

it('opens the design reference from Settings with isolated sample controls', async () => {
  const settings = await renderRoute('/settings')
  expect(settings).toContain('href="/settings/design"')
  const html = await renderRoute('/settings/design')
  expect(html).toContain('aria-current="page">Design')
  expect(html).toContain('href="/settings"')
  expect(html).toContain('Data colors')
  expect(html).toContain('--data-7')
  expect(html).toContain('aria-label="Sample date range"')
  expect(html).toContain('Example reimbursement')
})

it('shows Analytics dollar and percentage changes versus the matching previous period', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-02&to=2026-09-02')
  const comparison = html.slice(
    html.indexOf('aria-label="Change versus previous period"'),
    html.indexOf('</header>', html.indexOf('aria-label="Change versus previous period"')),
  )
  expect(comparison).toContain('-$10.00')
  expect(comparison).toContain('-33.33%')
})

it('orders Analytics cards by current value while preserving their labels', async () => {
  scenario.otherKind = 'income'
  scenario.otherCategory = 'Income'
  const html = await renderRoute('/analytics?range=month')
  const selector = html.slice(
    html.indexOf('aria-label="Analytics charts"'),
    html.indexOf('account-overview-dashboard'),
  )
  expect(selector.indexOf('>Income<')).toBeLessThan(selector.indexOf('>Amex credits<'))
  expect(selector.indexOf('>Cash flow<')).toBeLessThan(selector.indexOf('>Amex credits<'))
  expect(selector.indexOf('>Amex credits<')).toBeLessThan(selector.indexOf('>Dividends<'))
})
