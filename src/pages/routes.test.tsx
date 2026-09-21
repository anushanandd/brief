import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { renderToString } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { LiveMarketProvider } from '../hooks/live-market-provider'
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
      netWorth: -2_100,
      benchmarkHistory: [],
      netWorthHistory: [{ date: '2026-09-03', value: -2_100 }],
      spending: { monthTotal: 0, categories: [] },
      accounts: [
        {
          id: 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          type: scenario.accountType,
          value: -1_200,
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

it('shows every visible Platinum benefit on Spending', async () => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-16T12:00:00Z'))
  const html = await renderRoute('/spending')
  expect(html).toContain('Spending by category')
  expect(html).toContain('Largest expense')
  expect(html).toContain('Platinum benefits')
  expect(html).toContain('href="/spending/platinum"')
  expect(html).toContain('aria-label="Benefits ending Sep 30"')
  expect(html).toContain('lululemon')
  expect(html).toContain('Resy')
  expect(html).toContain('Digital entertainment')
  expect(html).toContain('Uber Cash')
  expect(html).toContain('Hotel credit')
  expect(html).toContain('Global Entry / TSA PreCheck')
  expect(html).toContain('Renewals &amp; purchases')
  expect(html).toContain('$70.00 of $100.00 detected')
  expect(html).toContain('aria-label="Resy credit progress"')
  expect(html).toContain('Resy logo')
  expect(html).toContain('Ending <!-- -->Sep 30')
  expect(html).toContain('14 days left')
  expect(html).not.toContain('Credits detected · 2026')
  expect(html).not.toContain('aria-label="Transaction evidence"')
  expect(html).toContain('Recent activity')
  expect(html).toContain('This month')
  expect(html).toContain('<number-flow-react')
  const recentActivity = html.slice(
    html.indexOf('class="surface-card spending-transactions-preview"'),
  )
  expect(recentActivity.match(/class="financial-activity-row"/g)).toHaveLength(7)
  expect(html).toContain('Selected card · Credit')
  expect(html).toContain('<span class="financial-activity-meta"><small>Sep 3</small>')
  expect(recentActivity).not.toContain('Other account transaction')
})

it('opens the Platinum workspace with visible balances and information controls', async () => {
  const html = await renderRoute('/spending/platinum')
  expect(html).toContain('href="/spending"')
  expect(html).toContain('aria-current="page">Platinum benefits')
  expect(html).toContain('Current benefits')
  expect(html).toContain('Credit history')
  expect(html).toContain('aria-label="Credit history year"')
  expect(html).toContain('Estimated remaining <!-- -->$30.00')
  expect(html).toContain('$70.00 of $100.00 detected')
  expect(html).toContain('aria-label="Resy credit progress"')
  expect(html).toContain(
    'aria-label="Uber Cash credit progress" aria-valuetext="No Uber purchase detected"',
  )
  expect(html).toContain('class="benefit-detail-row"')
  expect(html).toContain('No Uber purchase detected')
  const resyTitle = html.indexOf('id="benefit-resy"')
  const resyInfo = html.indexOf('aria-label="About Resy"', resyTitle)
  expect(resyInfo).toBeGreaterThan(resyTitle)
  expect(resyInfo).toBeLessThan(html.indexOf('class="benefit-allowance"', resyTitle))
  const creditHistory = html.slice(html.indexOf('class="benefit-history"'))
  expect(creditHistory).toContain('Resy logo')
  expect(creditHistory.indexOf('Resy logo')).toBeGreaterThan(
    creditHistory.indexOf('aria-label="Transaction evidence"'),
  )
  expect(html).toContain('aria-label="Digital entertainment benefit icon"')
  expect(html).toContain('aria-label="About Digital entertainment"')
  expect(html).toContain('aria-label="About benefit tracking"')
  expect(html).toContain('aria-haspopup="dialog"')
  expect(html).toContain('aria-expanded="false"')
  expect(html).not.toContain('No third-party billing')
  expect(html).toContain('href="/settings#platinum-benefits"')
  expect(html).not.toContain('Other account transaction')
})

it('lists estimated Uber Cash once without changing the posted credit total', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/spending/platinum')
  expect(html).toContain(
    'aria-label="Uber Cash credit progress" aria-valuetext="Estimated $15.00 used"',
  )
  const uberHistory = html.slice(html.indexOf('aria-label="Uber Cash credit history"'))
  expect(uberHistory).toContain('1 estimated credit')
  expect(uberHistory).toContain('Estimated Uber Cash credit')
  expect(uberHistory).toContain('$15.00')
  expect(uberHistory).toContain('Estimated from 1 posted Uber purchase.')
  const purchase = html.indexOf('aria-label="1 purchase for Uber Cash"')
  expect(purchase).toBeGreaterThan(0)
  const purchaseTrigger = html.slice(
    html.lastIndexOf('<button', purchase),
    html.indexOf('</button>', purchase) + '</button>'.length,
  )
  expect(purchaseTrigger).toContain('data-slot="tooltip-trigger"')
  expect(purchaseTrigger).toContain('lucide-receipt-text')
  expect(purchaseTrigger).not.toContain('lucide-info')
  expect(html).not.toContain('verify in Uber')
  expect(html).not.toContain('Qualifying purchase')
  expect(html).toContain('Uber Cash is estimated from Uber purchases')
  expect(html).toContain(
    'href="/analytics?chart=amex-credits&amp;from=2026-01-01&amp;to=2026-12-31"',
  )
})

it('requires the saved spending account on the Platinum detail route', async () => {
  const html = await renderRoute('/spending/platinum', '')
  expect(html).toContain('Choose a spending account')
  expect(html).not.toContain('id="benefit-resy"')
})

it('keeps benefit windows current when the saved spending statement is older', async () => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-10-02T12:00:00Z'))
  const summary = await renderRoute('/spending')
  expect(summary).toContain('Ending <!-- -->Oct 31')
  const html = await renderRoute('/spending/platinum')
  expect(html).toContain('Ending <!-- -->Oct 31')
  expect(html).toContain('Ending <!-- -->Dec 31')
  expect(html).not.toContain('Ending <!-- -->Sep 30')
  expect(html).not.toContain('$30.00 remaining')
  const lastCredit = html.indexOf('aria-label="Last detected credit:')
  expect(lastCredit).toBeGreaterThan(0)
  const lastCreditTrigger = html.slice(
    html.lastIndexOf('<button', lastCredit),
    html.indexOf('</button>', lastCredit) + '</button>'.length,
  )
  expect(lastCreditTrigger).toContain('data-slot="tooltip-trigger"')
  expect(lastCreditTrigger).toContain('lucide-rotate-ccw-clock')
  expect(lastCreditTrigger).not.toContain('lucide-info')
  expect(html).toContain('Posted in <!-- -->2026')
})

it('presents completed benefits and evidence openly and preserves hidden-benefit history', async () => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-16T12:00:00Z'))
  scenario.benefitAmount = 20
  const ledger = await renderRoute('/spending/platinum')
  expect(ledger).toContain('Full credit detected')
  expect(ledger.match(/Resy credit 0/g)).toHaveLength(1)
  expect(ledger).toContain('aria-label="Resy credit history"')
  expect(ledger).not.toContain('<details')
  scenario.hiddenBenefits = 'resy'
  const hidden = await renderRoute('/spending/platinum')
  const [current, history] = hidden.split('class="benefit-history"')
  expect(current).not.toContain('id="benefit-resy"')
  expect(history).toContain('Resy')
  expect(history).toContain('$140.00')
})

it('keeps pending evidence beside its benefit without including it in posted credit history', async () => {
  scenario.linkedPending = true
  const ledger = await renderRoute('/spending/platinum')
  const [current, history] = ledger.split('class="benefit-history"')
  expect(current).toContain('Resy credit 0')
  expect(current).toContain('Benefit credit · Pending')
  expect(current).toContain('$60.00 of $100.00 detected')
  expect(history).not.toContain('Resy credit 0')
  expect(history).toContain('Resy credit 1')
  expect(history).toContain('$60.00')
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
  expect(html).toContain('Save and test')
  expect(html).toContain('aria-label="Start date for Selected card"')
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
  const allHtml = await renderRoute('/accounts')
  expect(allHtml).toContain('All accounts')
  expect(allHtml).toContain('href="/accounts?account=other"')
  expect(allHtml).not.toContain('href="/accounts?account=selected"')
  expect(allHtml).toContain('>Accounts</span><strong>1</strong>')
  expect(allHtml.match(/class="account-switcher-button/g)).toHaveLength(2)

  const html = await renderRoute('/accounts?account=other')
  expect(html).toContain('Other card')
  expect(html).toContain('Account details')
  expect(html).toContain('Recent activity')
  expect(html).toContain('href="/activities?account=other"')
  expect(html.match(/account-preview-card/g)).toHaveLength(1)
  expect(html.match(/account-switcher-button active/g)).toHaveLength(1)
})

it('keeps credit accounts and their activity out of Accounts regardless of the spending selection', async () => {
  for (const spendingAccount of ['', 'selected', 'other']) {
    const html = await renderRoute('/accounts?account=other', spendingAccount)
    const page = html.slice(html.indexOf('class="page accounts-workspace-page"'))
    expect(page).not.toContain('href="/accounts?account=other"')
    expect(page).not.toContain('href="/accounts?account=selected"')
    expect(page).not.toContain('Other account transaction')
    expect(page).not.toContain('Resy credit')
    expect(page).toContain('All accounts')
  }
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
  expect(html).toContain('placeholder="Search description, category, or account"')
  expect(html).toContain('>Accounts</legend>')
  expect(html).toContain('>Categories</legend>')
  expect(html).toContain('>Dates</legend>')
  expect(html).toContain('aria-label="Start date"')
  expect(html).toContain('aria-label="End date"')
  expect(html).toContain('Sep 1, 2026')
  expect(html).toContain('Sep 3, 2026')
  expect(html).toContain('>Today</h3>')
})

it('shows selected holding overview, shared activity rows, security cards, total holdings, and news', async () => {
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
  expect(html).toContain('aria-label="Security price range"')
  expect(html).toContain('aria-label="1 year"')
  expect(html).toMatch(/aria-label="1 day"[^>]*aria-pressed="true"/)
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
  const allHoldings = selected.match(/<table[^>]*position-table-market[\s\S]*?<\/table>/)?.[0] ?? ''
  expect(allHoldings).toContain('<strong>AAA</strong>')
  expect(allHoldings).toContain('<strong>BBB</strong>')
  expect(allHoldings).toContain('Today %')
  expect(allHoldings).toContain('Week %')
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
  expect(main.match(/class="financial-activity-row/g)).toHaveLength(8)
})

it('shows labeled account-position dots beneath the Home chart', async () => {
  scenario.accountType = 'brokerage'
  const html = await renderRoute('/')
  expect(html).toContain('class="home-account-dots"')
  expect(html).toContain('aria-label="Chart account"')
  expect(html).toContain('aria-label="Show Selected card chart"')
  expect(html).toContain('aria-pressed="true"')
})

it('places the passive series-key row above the Home and Accounts range controls', async () => {
  scenario.accountType = 'brokerage'
  scenario.homeHistory = true
  const html = await renderRoute('/')
  const controlsStart = html.indexOf('performance-chart-controls')
  const controls = html.slice(controlsStart, html.indexOf('</header>', controlsStart))
  expect(controls).toContain('aria-label="Chart key"')
  expect(controls).toContain('data-series="value"')
  expect(controls).toContain('data-series="deposits"')
  expect(controls).toContain('data-series="benchmark"')
  expect(controls).toContain('<span>Value</span>')
  expect(controls).toContain('<span>Net deposits</span>')
  expect(controls).toContain('<span>VOO</span>')
  expect(controls.indexOf('performance-chart-key')).toBeLessThan(controls.indexOf('range-selector'))

  const accountHtml = await renderRoute('/accounts?account=selected', '')
  const accountControlsStart = accountHtml.indexOf('performance-chart-controls')
  const accountControls = accountHtml.slice(
    accountControlsStart,
    accountHtml.indexOf('</header>', accountControlsStart),
  )
  expect(accountControls).toContain('aria-label="Chart key"')
  expect(accountControls).toContain('data-series="deposits"')
  expect(accountControls).toContain('data-series="benchmark"')
  expect(accountControls.indexOf('performance-chart-key')).toBeLessThan(
    accountControls.indexOf('range-selector'),
  )
})

it.each([
  ['/accounts/investments', 'Combined portfolio'],
  ['/accounts/cash', 'Available cash'],
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
  expect(sidebar).not.toContain('Logs')
  expect(html).toContain('href="/logs"')
  expect(html).toContain('Live price updates')
})

it('asks for a spending account instead of falling back to another card', async () => {
  const html = await renderRoute('/spending', '')
  expect(html).toContain('Choose a spending account')
  expect(html).toContain('href="/settings"')
  expect(html).not.toContain('class="financial-activity-row"')
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

it('qualifies partial cost basis in the investment account workspace', async () => {
  scenario.accountType = 'brokerage'
  scenario.partialBasis = true
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('Known cost basis')
  expect(html).toContain('Known unrealized P/L')
  expect(html).toContain('class="muted">$0.00')
  expect(html).toContain('Estimated realized P/L YTD')
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
  expect(html).toContain('Sep 3, 2026')
  expect(html).toContain('Data needs attention')
  expect(html).toContain('snapshot-freshness warning')
  expect(html).toContain('dateTime="2026-09-03T12:00:00Z"')
})

it('points Logs back to Settings', async () => {
  const html = await renderRoute('/logs')
  const header = html.slice(html.indexOf('<header class="page-header'), html.indexOf('</header>'))
  expect(header).toMatch(/href="\/settings"[^>]*>Settings<\/a>/)
})

it('offers pointer controls for the spending range and reference period', async () => {
  const html = await renderRoute('/spending')
  const spendingPanel = html.slice(
    html.indexOf('<section class="spending-total-panel"'),
    html.indexOf('</section>', html.indexOf('<section class="spending-total-panel"')),
  )
  const spendingHeader = html.slice(
    html.indexOf('<header class="workspace-brief-heading"'),
    html.indexOf('</header>', html.indexOf('<header class="workspace-brief-heading"')),
  )
  expect(spendingHeader).toContain('spending-period-indicators')
  expect(spendingHeader).toContain('class="hero-change"')
  expect(spendingHeader).toContain('class="range-selector"')
  expect(spendingPanel).toContain(
    'aria-keyshortcuts="M Q Y A ArrowLeft ArrowRight Meta+ArrowLeft Meta+ArrowRight"',
  )
  expect(html).toContain('aria-label="Previous spending period"')
  expect(html).toContain('aria-label="Next spending period" disabled=""')
  expect(html).toMatch(/<button[^>]*aria-label="1 month"[^>]*aria-pressed="true"[^>]*>M<\/button>/)
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
  expect(chartHeader).toContain('class="range-selector"')
  expect(html).toContain('role="status">Chart range: <!-- -->1 week</span>')
  expect(html).toMatch(/<button[^>]*>W<\/button>/)
  expect(html).not.toContain('Historical series unavailable')
})

it('offers account activity CSV downloads', async () => {
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('aria-label="Download account activity as CSV"')
})

it('narrows account and category choices reciprocally without dropping selected filters', async () => {
  scenario.otherCategory = 'Food'
  const byCategory = await renderRoute('/activities?category=Food')
  const accountChoices = byCategory.split('>Accounts</legend>')[1].split('</fieldset>')[0]
  expect(accountChoices).toContain('Other card')
  expect(accountChoices).not.toContain('Selected card')
  expect(byCategory).toContain('Other account transaction')
  expect(byCategory).not.toContain('Resy credit 0')

  const byAccount = await renderRoute('/activities?account=selected')
  const categoryChoices = byAccount.split('>Categories</legend>')[1].split('</fieldset>')[0]
  expect(categoryChoices).toContain('Credit')
  expect(categoryChoices).not.toContain('Food')

  const incompatible = await renderRoute('/activities?account=selected&category=Food')
  expect(incompatible.split('>Accounts</legend>')[1].split('</fieldset>')[0]).toContain(
    'Selected card',
  )
  expect(incompatible.split('>Categories</legend>')[1].split('</fieldset>')[0]).toContain('Food')
  expect(incompatible).toContain('Clear accounts')
  expect(incompatible).toContain('Clear categories')
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

it('opens the selected analytics chart with shared account and date scope', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('aria-label="Analytics charts"')
  expect(html).toContain('aria-keyshortcuts="Meta+ArrowLeft Meta+ArrowRight"')
  expect(html).toContain('aria-current="page">Amex credits')
  expect(html).toContain('$70.00')
  expect(html).toContain('aria-label="1 week"')
  expect(html).toContain('Sep 1, 2026')
  expect(html).toContain('chart=interest')
  expect(html).toContain('from=2026-09-01')
  expect(html).toContain('analysis=amex-credits')
  const scoped = await renderRoute('/analytics?chart=amex-credits&account=other&range=year')
  expect(scoped).toContain('No matching posted activity in this period.')
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
  expect(html).toContain('aria-label="Clear analytics filter"')
  expect(html).toContain('Posted dates')
  const other = await renderRoute('/activities?analysis=amex-credits&account=other')
  expect(other).toContain('No activity matches these filters.')
  const unknown = await renderRoute('/activities?analysis=amex-credits&account=missing')
  expect(unknown).toContain('No activity matches these filters.')
})

it('links existing summary metrics to their scoped analytics charts', async () => {
  scenario.accountType = 'brokerage'
  const accounts = await renderRoute('/accounts?account=selected')
  expect(accounts).toContain(
    'href="/analytics?chart=dividends&amp;range=year&amp;account=selected"',
  )
  expect(accounts).toContain('href="/analytics?chart=interest&amp;range=year&amp;account=selected"')
  scenario.accountType = 'credit'
  const platinum = await renderRoute('/spending/platinum')
  expect(platinum).toContain(
    'href="/analytics?chart=amex-credits&amp;from=2026-01-01&amp;to=2026-12-31"',
  )
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

it('keeps account rows available to reselect and carries multiple or empty scopes into Activity', async () => {
  const single = await renderRoute('/analytics?chart=amex-credits&account=other&range=month')
  const rows =
    single.match(
      /<label class="analytics-breakdown-row analytics-account-row"[\s\S]*?<\/label>/g,
    ) ?? []
  expect(rows).toHaveLength(1)
  expect(rows[0]).toContain('Selected card')
  expect(rows[0]).not.toContain('checked=""')
  expect(rows.some((row) => row.includes('Other card'))).toBe(false)
  expect(single).toContain('aria-label="Clear account selection"')
  const multiple = await renderRoute(
    '/analytics?chart=amex-credits&account=selected,other&range=month',
  )
  expect(multiple).toContain('$70.00')
  expect(multiple).toContain('account=selected%2Cother')
  const activity = await renderRoute('/activities?analysis=amex-credits&account=selected,other')
  expect(activity).toContain('Resy credit 0')
  const empty = await renderRoute('/analytics?chart=amex-credits&account=__none__')
  expect(empty).toContain('No matching posted activity in this period.')
  expect(empty).toContain('account=__none__')
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
  expect(html).toContain('activity-mark-income')
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
  const activityCard = html.slice(
    html.indexOf('analytics-activities-card'),
    html.indexOf('analytics-companion'),
  )
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
  expect(primary.indexOf('Sources and uses')).toBeGreaterThan(0)
  expect(primary.indexOf('Sources and uses')).toBeLessThan(
    primary.indexOf('analytics-activities-card'),
  )
  expect(primary).toContain('Last week')
  const companion = html.slice(html.indexOf('analytics-companion'))
  expect(companion).toContain('<h2>Expected activity</h2>')
})

it('attaches Analytics tooltip triggers to dated categories and provides a legend', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  const chart = html.slice(
    html.indexOf('aria-label="Period totals"'),
    html.indexOf('analytics-activities-card'),
  )
  const triggers = chart.match(/<a\b[^>]*data-slot="tooltip-trigger"[^>]*>/g) ?? []
  expect(html).toContain('aria-label="Chart legend"')
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
    html.indexOf('aria-label="Analytics period"'),
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
