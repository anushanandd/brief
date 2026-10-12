// Spending, Benefits, and the Money and Investments reports.

import { expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { renderRoute, renderSettings } from './routes.harness'
import { scenario } from './routes.scenario'

it('keeps a short Platinum preview on Spending and links to Benefits', async () => {
  const html = await renderRoute('/spending')
  expect(html).toContain('<h2>Categories</h2>')
  expect(html).toContain('a day</span>')
  expect(html).toContain('<dt>Captured</dt>')
  expect(html).toContain('<dt>Left</dt>')
  // Coming up lives on Today; Money keeps its companion to two cards.
  expect(html).not.toContain('expected-activity-card')
  expect(html).toContain('class="pace-chart"')
  expect(html).not.toContain('spending-bar-chart')
  expect(html).toContain('href="/analytics?chart=amex-credits"')
  const preview = html.slice(html.indexOf('class="surface-card spending-benefits-card"'))
  // Only windows with allowance left, soonest deadline first: a used credit has nothing to act on.
  expect(preview.match(/class="spending-benefit-row"/g)).toHaveLength(2)
  expect(preview).toMatch(/class="benefit-allowance">\$[\d,.]+<\/strong>/)
  expect(preview).not.toMatch(/class="benefit-allowance">[^<]*\//)
  expect(preview).toContain('Resy')
  expect(preview).not.toContain('Global Entry / TSA PreCheck')
  // Activity is the page's body, under the hero; benefits sit in the companion beside it.
  expect(html.indexOf('spending-ledger-card')).toBeGreaterThan(-1)
  expect(html.indexOf('spending-ledger-card')).toBeLessThan(html.indexOf('spending-companion'))
  expect(html.indexOf('spending-companion')).toBeLessThan(html.indexOf('spending-benefits-card'))
  expect(html).toContain('Selected card · Credit')
  expect(html).not.toContain('Other account transaction')
})

it('ranks spending categories below the pace chart', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/spending')
  const categories = html.slice(html.indexOf('class="ranked-bars"'))
  expect(categories).toMatch(/aria-label="[^"]* spending by category"/)
  expect(categories).toContain('Travel')
  expect(html.indexOf('class="ranked-bars"')).toBeGreaterThan(html.indexOf('class="pace-chart"'))
  expect(html).not.toContain('aria-label="Chart legend"')
})

it('places current benefits below the Amex chart and credit activity below Overview', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&range=year')
  expect(html).toContain('Current benefit windows')
  expect(html).toContain('Credit activity')
  expect(html).toContain('>Overview</h2>')
  expect(html).toContain('$30.00 left')
  expect(html).toContain('$70.00 of $100.00 credited')
  expect(html).toContain('aria-label="Resy credit progress"')
  expect(html).toContain('aria-label="About benefit tracking"')
  expect(html).toContain('aria-label="About Resy"')
  expect(html).toContain('class="benefit-allowance">$25 / month</strong>')
  expect(html).toContain('href="#settings/platinum-benefits"')
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
  expect(current).toContain('$60.00 of $100.00 credited')
  const activity = html.slice(html.indexOf('analytics-activities-card'))
  expect(activity).not.toContain('Resy credit 0')
  expect(activity).toContain('Resy credit 1')
})

it('shows every account without a saved card and never treats another card as the spending card', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/spending', '')
  // Spending is across accounts; statements and benefits still wait for a saved card.
  expect(html).toContain('class="financial-activity-row')
  expect(html).not.toContain('Current statement')
  expect(html).toContain('No spending card')
  expect(html).toContain('href="#settings/spending-account"')
  expect(html).not.toContain('id="benefit-')
})

it('preserves merchant links and pending status in the spending preview', async () => {
  scenario.linkedPending = true
  const html = await renderRoute('/spending')
  expect(html).toContain('href="https://example.com/"')
  expect(html).toContain('Pending')
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

it('opens the selected analytics chart with shared account and date scope', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('class="account-switcher" aria-label="Money"')
  expect(html).toContain('aria-keyshortcuts="Meta+ArrowLeft Meta+ArrowRight"')
  expect(html).toContain('aria-current="page">Benefits')
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

it('shows analytics-specific expected activity below Accounts', async () => {
  vi.setSystemTime(new Date('2026-09-03T12:00:00Z'))
  scenario.extraTransactions = ['2026-06-03', '2026-07-03', '2026-08-03'].flatMap((date, index) => [
    {
      id: `payroll-${index}`,
      merchant: 'Example payroll',
      category: 'Income',
      date,
      amount: 1_000,
      account: 'Other card',
      accountId: 'other',
      classification: classification('income'),
      pending: false,
    },
    {
      id: `streaming-${index}`,
      merchant: 'Example streaming subscription',
      category: 'Subscription',
      date,
      amount: -15,
      account: 'Other card',
      accountId: 'other',
      classification: classification('expense'),
      pending: false,
    },
  ])
  const html = await renderRoute('/analytics?chart=income')
  const companion = html.slice(html.indexOf('class="analytics-companion"'))
  const expected = companion.slice(companion.indexOf('<h2>Coming up</h2>'))
  expect(companion.indexOf('<h2>Accounts</h2>')).toBeLessThan(
    companion.indexOf('<h2>Coming up</h2>'),
  )
  expect(expected).toContain('Example payroll')
  expect(expected).not.toContain('Example streaming subscription')
})

it('opens Cash flow by default and lists charts in one fixed order', async () => {
  const analytics = await renderRoute('/analytics')
  expect(analytics).toContain('aria-current="page">Cash flow')
  expect(analytics).not.toContain('Reorder analytics')
  const settings = await renderSettings()
  expect(settings).toContain('Page order')
  expect(settings).not.toContain('Analytics order')
  expect(settings).not.toContain('Overview accounts')
})

it('keeps an unconfigured Amex chart unavailable and preserves unavailable account scope', async () => {
  expect(await renderRoute('/analytics?chart=amex-credits', '')).toContain(
    'Choose a spending account',
  )
  expect(await renderRoute('/analytics?chart=income&account=unknown')).toContain(
    'This account is unavailable',
  )
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
  const html = await renderRoute('/analytics?chart=income&view=transactions')
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
  const html = await renderRoute('/analytics?chart=interest&account=other&view=transactions')
  // The transactions sit in the report's lower card, behind its Breakdown/Transactions switch.
  const activityCard = html.slice(
    html.indexOf('analytics-breakdown-card'),
    html.indexOf('analytics-companion'),
  )
  expect(activityCard).toContain('<h2>Transactions</h2>')
  expect(activityCard).toMatch(/<a[^>]*href="\/activities\?[^"]*"[^>]*>Open in Activity/)
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
  const html = await renderRoute('/analytics?chart=cash-flow&view=transactions')
  const primary = html.slice(html.indexOf('analytics-primary'), html.indexOf('analytics-companion'))
  expect(primary.indexOf('cash-flow-sankey')).toBeGreaterThan(0)
  expect(primary.indexOf('cash-flow-sankey')).toBeLessThan(
    primary.indexOf('analytics-breakdown-card'),
  )
  expect(primary).not.toContain('analytics-period-plot')
  // One list of the report's activity, grouped by date, in the lower card.
  expect(primary).toContain('Last week')
  const companion = html.slice(html.indexOf('analytics-companion'))
  expect(companion).not.toContain('analytics-activities-card')
  expect(companion).toContain('<h2>Coming up</h2>')
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

it('shows Analytics dollar and percentage changes versus the matching previous period', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&from=2026-09-02&to=2026-09-02')
  const comparison = html.slice(
    html.indexOf('aria-label="Change versus previous period"'),
    html.indexOf('</header>', html.indexOf('aria-label="Change versus previous period"')),
  )
  expect(comparison).toContain('−$10.00')
  expect(comparison).toContain('−33.33%')
})

it('lists a report beside the others of its destination, after that destination’s own pages', async () => {
  scenario.otherKind = 'income'
  scenario.otherCategory = 'Income'
  const html = await renderRoute('/analytics?range=month')
  const selector = html.slice(
    html.indexOf('class="account-switcher" aria-label="Money"'),
    html.indexOf('account-overview-dashboard'),
  )
  expect(selector.indexOf('>Spending<')).toBeGreaterThan(-1)
  expect(selector.indexOf('>Spending<')).toBeLessThan(selector.indexOf('>Cash flow<'))
  expect(selector.indexOf('>Cash flow<')).toBeLessThan(selector.indexOf('>Income<'))
  expect(selector.indexOf('>Spending<')).toBeLessThan(selector.indexOf('>Recurring<'))
  expect(selector.indexOf('>Recurring<')).toBeLessThan(selector.indexOf('>Cash flow<'))
  expect(selector.indexOf('>Income<')).toBeLessThan(selector.indexOf('>Interest<'))
  expect(selector.indexOf('>Fees<')).toBeLessThan(selector.indexOf('>Benefits<'))
  // Interest is mostly cash and savings, so it is a Money report.
  expect(selector).toContain('>Interest<')
  // Investment reports live under Investments, not here.
  expect(selector).not.toContain('>Dividends<')
  // Investments reports have no strip: they open from the portfolio and lead back by breadcrumb.
  const investments = await renderRoute('/analytics?chart=dividends&range=month')
  expect(investments).not.toContain('aria-label="Investments reports"')
  const heading = investments.slice(investments.indexOf('<h1'), investments.indexOf('</h1>'))
  expect(heading).toContain('href="/holdings"')
})

it('shows days until the statement closes in the Spending chart and only rules in Settings', async () => {
  scenario.insights = {
    today: '2026-09-20',
    statement: {
      source: 'issuer',
      boundaries: ['2026-08-25', '2026-09-25'],
      autopays: [],
      lastClose: '2026-08-25',
      nextClose: '2026-09-25',
    },
  }
  const spending = await renderRoute('/spending')
  expect(spending).toContain('Current statement (closes in 5 days)')
  const settings = await renderSettings('/#settings/category-rules')
  expect(settings).toContain('Category rules')
  expect(settings).not.toContain('Monthly targets')
})
