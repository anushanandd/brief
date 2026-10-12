// Accounts and Activity.

import { expect, it } from 'vitest'

import { renderRoute } from './routes.harness'
import { scenario } from './routes.scenario'

it('links account selections and defaults the workspace to all accounts', async () => {
  scenario.otherAccountType = 'cash'
  const allHtml = await renderRoute('/accounts', '')
  expect(allHtml).toContain('All accounts')
  expect(allHtml).toContain('href="/accounts?account=other"')
  expect(allHtml).toContain('href="/accounts?account=selected"')
  // Every account sits under its group; the list, not a strip, is the way into each one.
  expect(allHtml).toContain('<h2>Balances</h2>')
  expect(allHtml).toContain('<h2>Made of</h2>')
  expect(allHtml).toContain('class="account-group-section"')
  expect(allHtml).not.toContain('class="account-switcher"')
  expect(allHtml.indexOf('<h2>Account details</h2>')).toBeLessThan(
    allHtml.indexOf('<h2>Made of</h2>'),
  )
  expect(allHtml).toContain('<h2>Coming up</h2>')

  const html = await renderRoute('/accounts?account=other', '')
  expect(html).toContain('Other card')
  expect(html).toContain('Account details')
  expect(html).toContain('Recent activity')
  expect(html).toContain('href="/activities?account=other"')
  expect(html).toContain('account-preview-grid account-preview-grid-wide-recent')
  expect(html).toContain('<h2>Coming up</h2>')
  expect(html.indexOf('Recent activity')).toBeLessThan(html.indexOf('<h2>Coming up</h2>'))
  expect(html.match(/account-preview-card/g)).toHaveLength(2)
  expect(html).toContain('aria-current="page">Other card')
})

it('shows all-account investment figures in their own card before the composition', async () => {
  scenario.accountType = 'brokerage'
  scenario.partialBasis = true
  scenario.trades = [
    {
      id: 'gain',
      type: 'SELL',
      date: '2026-09-03',
      account: 'Other card',
      accountId: 'other',
      amount: 25,
      estimatedRealizedGain: 5,
    },
  ]
  const html = await renderRoute('/accounts')
  // Gains, income, and contributions across accounts sit in Account details, above "Made of".
  expect(html).toContain('<h2>Balances</h2>')
  expect(html).not.toContain('account-details-more')
  const details = html.slice(
    html.indexOf('<h2>Account details</h2>'),
    html.indexOf('<h2>Made of</h2>'),
  )
  expect(details).toContain('<span>Known unrealized P/L</span>')
  expect(details).toContain('<span>Estimated realized P/L</span>')
  expect(details).toContain('<span>Net contributions</span>')
  expect(details).toContain('<span>Investment gain</span>')
  expect(details).toContain('chart=interest')
  expect(details).toContain('chart=dividends')
  expect(html).not.toContain('Estimated FIFO P/L $5.00')
})

it('keeps the saved spending account out of the Accounts workspace', async () => {
  const html = await renderRoute('/accounts?account=selected')
  const page = html.slice(html.indexOf('class="page accounts-workspace-page"'))
  // The saved card can't be opened here; the page falls back to every other account.
  expect(page).toContain('href="/accounts?account=other"')
  expect(page).not.toContain('href="/accounts?account=selected"')
  expect(page).not.toContain('aria-current="page">Selected card')
  expect(page).not.toContain('Resy credit')
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
  expect(html).toContain('aria-label="Search and filter activity"')
  expect(html).toContain('class="financial-activity-header"')
  expect(html).not.toContain('<h2>Activity</h2>')
  expect(html).toContain('aria-label="Sort by Account"')
  expect(html).toContain('aria-label="Sort by Category"')
  expect(html).toContain('aria-label="Sort by Location"')
  expect(html).toContain('aria-label="Sort by For"')
  expect(html).not.toContain('aria-label="Sort by Method"')
  expect(html).toMatch(/aria-haspopup="dialog">Open details: /)
  expect(html).toContain('aria-label="Filter by account:')
  expect(html).toContain('aria-label="Filter by category:')
  expect(html).toContain('aria-label="Filter by date:')
  expect(html).toContain('Selected card')
  expect(html).toContain('aria-label="Filters, 3 active"')
  // A typed range reads as its dates on the preset button; there is no custom panel.
  expect(html).toContain('aria-label="Activity dates: Sep 1, 2026 – Sep 3, 2026"')
  expect(html).not.toContain('aria-label="Custom date range"')
  expect(html.indexOf('aria-label="Activity dates:')).toBeLessThan(
    html.indexOf('aria-label="Download filtered activity as CSV"'),
  )
  expect(html).not.toContain('<legend>Dates</legend>')
  expect(html).toMatch(/aria-label="Search and filter activity"[^>]*aria-keyshortcuts="\/"/)
  // F opens the filters card; it starts closed.
  expect(html).toContain('data-command="activity.filters.toggle"')
  expect(html).not.toContain('activity-filter-card')
  expect(html).not.toContain('role="combobox"')
  // Coming up lives on Today; Activity is the ledger alone.
  expect(html).not.toContain('<h2>Coming up</h2>')
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
  expect(html).toContain('No history yet')
  expect(html).not.toContain('live-performance-chart')
})

it('qualifies partial unrealized gain in the investment account workspace', async () => {
  scenario.accountType = 'brokerage'
  scenario.partialBasis = true
  const html = await renderRoute('/accounts?account=selected', '')
  expect(html).toContain('<span>Investment gain</span>')
  expect(html).toContain('<span>Net contributions</span>')
  expect(html).toContain('Known total unrealized')
  expect(html).toContain('account-preview-grid account-preview-grid-wide-primary')
  expect(html).toContain('Recent activity')
  expect(html).not.toContain('<h2>Coming up</h2>')
  expect(html).toContain('class="muted">$0.00')
  expect(html).toContain('<span>Estimated realized P/L</span>')
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
  expect(html).not.toContain('No history yet')
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
  expect(html).toContain('aria-label="Remove text filter: “description:provider”"')
  expect(html).toContain('<strong>Resy credit 0</strong>')
  expect(html).not.toContain('<strong>Resy credit 1</strong>')
})

it('opens the exact Amex evidence in Activity and keeps empty account scopes empty', async () => {
  const html = await renderRoute('/activities?analysis=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('Resy credit 0')
  expect(html).not.toContain('<strong>Other account transaction</strong>')
  expect(html).toContain('aria-label="Remove report filter:')
  const other = await renderRoute('/activities?analysis=amex-credits&account=other')
  expect(other).toContain('No activity matches these filters.')
  const unknown = await renderRoute('/activities?analysis=amex-credits&account=missing')
  expect(unknown).toContain('No activity matches these filters.')
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

  // Today's hero is net worth, which no single provider reports.
  const home = await renderRoute('/')
  const homeText = home.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ')
  expect(homeText).not.toContain('SnapTrade:')
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
