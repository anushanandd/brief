// Spending, Benefits, and the Money and Investments reports.

import { expect, it, vi } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { renderRoute, renderSettings } from './routes.harness'
import { scenario } from './routes.scenario'

it('keeps a short Platinum preview on Spending and links to Benefits', async () => {
  const html = await renderRoute('/spending')
  expect(html).toContain('<h2>Categories</h2>')
  expect(html).toContain('a day</span>')
  // A glance leads with the fee, then what to use soon; captured credits and exact ranges read
  // on Benefits.
  expect(html).not.toContain('<dt>Captured</dt>')
  // Coming up lives on Home; Spending's companion ends with subscriptions instead.
  expect(html).not.toContain('<h2>Coming up</h2>')
  expect(html).toContain('<h2>Subscriptions</h2>')
  expect(html.indexOf('spending-benefits-card')).toBeLessThan(
    html.indexOf('<h2>Subscriptions</h2>'),
  )
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
  expect(html).toContain('aria-label="Spending by merchant"')
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

/** Which of the two synthetic categories Spending ranks at `path`. */
async function rankedCategories(path: string) {
  const html = await renderRoute(path)
  const ranked = html.slice(html.indexOf('class="ranked-bars"'))
  return ['Travel', 'Food and Drink'].filter((name) => ranked.includes(name))
}

it('counts your spending, someone else’s, or both once a purchase is theirs', async () => {
  scenario.uberPurchase = true
  const mine = await renderRoute('/spending?range=month')
  expect(mine).not.toContain('data-command="spending.for."')

  scenario.extraTransactions = [
    {
      id: 'their-purchase',
      merchant: 'Example bistro',
      category: 'Food and Drink',
      date: '2026-09-02',
      amount: -30,
      account: 'Selected card',
      accountId: 'selected',
      other: true,
      classification: classification('owed', { othersSpending: true }),
      pending: false,
    },
  ]
  const owned = await renderRoute('/spending?range=month')
  expect(owned).toContain('data-command="spending.for."')
  expect(owned).toContain('aria-label="Whose spending. Showing: For me"')
  expect(await rankedCategories('/spending?range=month')).toEqual(['Travel'])
  expect(await rankedCategories('/spending?range=month&for=other')).toEqual(['Food and Drink'])
  expect(await rankedCategories('/spending?range=month&for=all')).toEqual([
    'Travel',
    'Food and Drink',
  ])
  // The lists hold only what the choice counts, and Activity opens on the same choice.
  const theirRow = 'data-activity-id="spending:their-purchase"'
  const mineRow = 'data-activity-id="spending:uber-purchase"'
  expect(owned).toContain(mineRow)
  expect(owned).not.toContain(theirRow)
  expect(owned).toMatch(/href="\/activities\?[^"]*for=me/)
  const theirs = await renderRoute('/spending?range=month&for=other')
  expect(theirs).toContain(theirRow)
  expect(theirs).not.toContain(mineRow)
  expect(theirs).toMatch(/href="\/activities\?[^"]*for=other/)
  const everyone = await renderRoute('/spending?range=month&for=all')
  expect(everyone).toContain(theirRow)
  expect(everyone).toContain(mineRow)
  expect(everyone).not.toMatch(/href="\/activities\?[^"]*for=/)
})

it('compares spending categories with the last period or the average', async () => {
  scenario.uberPurchase = true
  const withoutAverage = await renderRoute('/spending')
  // Without earlier periods to average, the comparison is fixed.
  expect(withoutAverage).toContain('aria-hidden="true"></i>vs last month</span>')
  // The track marks what the change compares with: here, last month by this point.
  expect(withoutAverage).toContain('class="ranked-bars-mark"')
  expect(withoutAverage).toMatch(/, by this point last month \$[\d,.]+/)
  expect(withoutAverage).not.toContain('aria-label="Compare categories with"')

  const periods = [{ kind: 'month' as const, start: '2026-09-01', typical: { Travel: 5 } }]
  scenario.insights = {
    spending: ['all', 'selected'].map((scope) => ({ scope, owner: 'me' as const, periods })),
  }
  const html = await renderRoute('/spending')
  const tabs = html.slice(
    html.indexOf('aria-label="Compare categories with"'),
    html.indexOf('class="ranked-bars"'),
  )
  expect(tabs).toContain('aria-label="Compare categories with"')
  expect(tabs).toMatch(/aria-selected="true"[^>]*data-command="spending.compare.previous"/)
  expect(tabs).toMatch(/aria-selected="false"[^>]*data-command="spending.compare.typical"/)
  expect(tabs).toContain('Last month')
  expect(tabs).toContain('Average')
})

it('lays out Benefits as a year grid of periods with its totals and review link', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits')
  const grid = html.slice(html.indexOf('aria-label="Benefits in 2026"'))
  expect(grid).toContain('Use by deadline')
  // Each row's estimate reads beside what it captured and missed.
  expect(grid).toContain('<th scope="col" class="benefit-grid-figure">Estimated</th>')
  // The page carries no footnote: the panel hides a perk and links its terms.
  expect(html).not.toContain('Each cell is one period')
  expect(grid).toContain('Recurring')
  // Each period is one cell, named by its state and what it left.
  expect(grid).toContain('aria-label="Q1: Missed. $100.00 missed"')
  expect(grid).toContain('aria-label="Q2: Partly used. $40.00 of $100.00 · $60.00 missed"')
  expect(grid).toContain('aria-label="Q3: Open. $70.00 credited · $30.00 left · ends Sep 30"')
  expect(grid).toMatch(/colspan="3"/i)
  // Totals sum the year to the cent: credits captured for you and settled allowance missed.
  // What's open now reads in the grid's own cells. The page title already says Benefits.
  expect(html).not.toContain('>Benefits</h2>')
  const metric = (label: string) => html.slice(html.indexOf(`<span>${label}</span>`)).slice(0, 240)
  expect(metric('For you in 2026')).toContain('$310.00<')
  // No credit landed on someone else's purchase, so there is nothing to split out.
  expect(html).not.toContain('<span>For others</span>')
  expect(metric('Missed')).toContain('$160.00<')
  expect(html).not.toContain('<span>Left now</span>')
  // Three future months each expect a $13.99 streaming charge.
  expect(metric('Estimated')).toContain('$41.97<')
  expect(html).toContain('aria-label="Oct: Not started. Opens Oct 1 · $13.99 estimated"')
  expect(html).toMatch(/data-command="benefits\.review"[^>]*>1 match/)
  // Each benefit's panel hides it, so the page no longer sends you to Settings.
  expect(html).not.toContain('href="#settings/platinum-benefits"')
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Benefits year/)
  // The grid replaces the generic report: no chart, Overview, or credit list.
  expect(html).not.toContain('>Overview</h2>')
  expect(html).not.toContain('Credit activity')
  expect(html).not.toContain('Other account transaction')
})

it('keeps estimated Uber Cash out of the captured total', async () => {
  scenario.uberPurchase = true
  const html = await renderRoute('/analytics?chart=amex-credits')
  expect(html).toContain('aria-label="Sep: Likely used. An Uber charge suggests it ran out"')
  const captured = html.slice(html.indexOf('<span>For you in 2026</span>')).slice(0, 240)
  expect(captured).toContain('$310.00<')
})

it('requires the saved spending account in the combined view', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits', '')
  expect(html).toContain('Choose a spending account')
  expect(html).not.toContain('id="benefit-resy"')
  expect(html).toContain('>Overview</h2>')
  expect(html).toContain('Credit activity')
})

it('shows one calendar year, and the strip follows it', async () => {
  // A window that isn't a whole year shows this year to date.
  const custom = await renderRoute('/analytics?chart=amex-credits&from=2026-09-01&to=2026-09-03')
  expect(custom).toContain('aria-label="Benefits in 2026"')
  const earlier = await renderRoute('/analytics?chart=amex-credits&from=2025-01-01&to=2025-12-31')
  expect(earlier).toMatch(/No benefits in (<!-- -->)?2025/)
  expect(earlier).not.toContain('left now')
})

it('leaves hidden benefits out of the grid and its review', async () => {
  scenario.hiddenBenefits = 'resy'
  const html = await renderRoute('/analytics?chart=amex-credits')
  expect(html).not.toContain('aria-label="Resy"')
  expect(html).not.toContain('to review')
  expect(html).toContain('aria-label="Digital entertainment"')
})

it('opens a benefit’s period with the transactions behind it', async () => {
  scenario.linkedPending = true
  const html = await renderRoute('/analytics?chart=amex-credits&benefit=resy&period=2026-07-01')
  const inspector = html.slice(html.indexOf('class="transaction-dialog" aria-label="Resy"'))
  expect(inspector).toContain('aria-label="Resy periods"')
  expect(inspector).toContain('Official benefit terms')
  expect(inspector).toContain('Is this the Resy credit, or a return?')
  // The focused period is open; the posted credits and the pending one are its evidence.
  expect(inspector).toMatch(/aria-expanded="true"[^>]*>.*Q3/)
  expect(inspector).toContain('Resy credit 1')
  expect(inspector).toContain('Pending')
  expect(inspector).toMatch(/aria-expanded="false"[^>]*>.*Q2/)
  // A perk you don't use can leave Missed and reminders without a trip to Settings.
  expect(inspector).toMatch(/data-command="benefits\.use\.toggle"[^>]*>Not using this</)
})

it('offers to track a hidden benefit again from its details', async () => {
  scenario.hiddenBenefits = 'resy'
  const html = await renderRoute('/analytics?chart=amex-credits&benefit=resy')
  const inspector = html.slice(html.indexOf('class="transaction-dialog" aria-label="Resy"'))
  expect(inspector).toMatch(/data-command="benefits\.use\.toggle"[^>]*>Track this again</)
  expect(inspector).toContain('Hidden from Missed and reminders')
})

it('lists matches to review with the decisions each needs', async () => {
  const html = await renderRoute('/analytics?chart=amex-credits&review=true')
  const inspector = html.slice(html.indexOf('class="transaction-dialog" aria-label="To review"'))
  expect(inspector).toContain('1 match')
  expect(inspector).toContain('Is this the Resy credit, or a return?')
  expect(inspector).toContain('It’s the credit')
  expect(inspector).toContain('A return')
  // The inspector moves through its own rows while app shortcuts pause.
  expect(inspector).toContain('aria-label="Keyboard"')
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

it('shows where spending went beside the latest activity', async () => {
  scenario.linkedPending = true
  scenario.extraTransactions = ['2026-09-01', '2026-09-02'].map((date, index) => ({
    id: `bistro-${index}`,
    merchant: 'Example bistro',
    category: 'Food and Drink',
    date,
    amount: -12,
    account: 'Selected card',
    accountId: 'selected',
    classification: classification('expense'),
    pending: false,
  }))
  const html = await renderRoute('/spending')
  const columns = html.slice(html.indexOf('spending-activity-columns'))
  expect(html).not.toContain('aria-label="Show activity"')
  // Where it went comes first, then Activities, which links to the full ledger.
  const activitiesHeading = columns.indexOf('class="section-heading-link">Activities ')
  expect(columns.indexOf('<h2>Where it went</h2>')).toBeGreaterThan(-1)
  expect(columns.indexOf('<h2>Where it went</h2>')).toBeLessThan(activitiesHeading)
  const merchants = columns.slice(0, activitiesHeading)
  // Merchants stay closed until opened, each with how many activities it holds.
  expect(merchants).toMatch(
    /class="financial-activity-row merchant-group-row tree-parent"[^>]*aria-expanded="false"/,
  )
  // The chevron follows the count, so the total lines up with its activities' amounts.
  expect(merchants).toMatch(/<small class="merchant-group-count">2 purchases<svg/)
  // A merchant with one activity reads as that activity, not a count of one.
  expect(merchants).not.toMatch(/<small>1 (purchase|transaction)<\/small>/)
  expect(merchants).toContain('data-activity-id="spending:transaction-0"')
  // Activities in both lists open the details dialog Activity uses.
  expect(merchants).toMatch(/data-activity-id="spending:transaction-0"[^>]*data-keyboard-open/)
  const latest = columns.slice(columns.indexOf('aria-label="Spending activities"'))
  expect(latest).toContain('financial-activity-row is-openable')
  expect(latest).not.toContain('href="https://example.com/"')
  expect(latest).toContain('Pending')
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
  scenario.creditKind = 'income'
  const html = await renderRoute('/analytics?chart=income&from=2026-09-01&to=2026-09-03')
  expect(html).toContain('class="account-switcher" aria-label="Money"')
  expect(html).toContain('aria-keyshortcuts="Meta+ArrowLeft Meta+ArrowRight"')
  expect(html).toContain('aria-current="page">Income')
  expect(html).toContain('$70.00')
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Analytics period"/)
  expect(html).toContain('Custom')
  expect(html).toContain('Sep 1, 2026')
  expect(html).toContain('chart=interest')
  expect(html).toContain('from=2026-09-01')
  expect(html).toContain('analysis=income')
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

it('keeps Spending’s benefits to the credits to use soonest', async () => {
  const html = await renderRoute('/spending')
  const card = html.slice(html.indexOf('class="surface-card spending-benefits-card"'))
  expect(card).toMatch(/class="section-heading-link">Benefits(<!-- -->)? /)
  // The fee and what was missed read on Benefits.
  expect(card).not.toContain('benefit-fee-recovery')
  expect(card).not.toContain('Missed this period')
  // The companion ends with subscriptions; what's coming up lives on Home.
  expect(html).not.toContain('Expected spending')
  // The activity list is called Activities and links to the full ledger.
  expect(html).toContain('class="section-heading-link">Activities ')
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
  expect(multiple).toContain('aria-label="Benefits in 2026"')
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

it('lists each matching activity separately in Analytics', async () => {
  scenario.creditKind = 'income'
  const html = await renderRoute(
    '/analytics?chart=income&from=2026-09-01&to=2026-09-03&view=transactions',
  )
  const list = html.slice(html.indexOf('aria-label="Period totals"'))
  expect(list.match(/class="financial-activity-row"/g)).toHaveLength(7)
  expect(list).toContain('Selected card')
  expect(list).not.toContain('Other account transaction')
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
  scenario.creditKind = 'income'
  const html = await renderRoute('/analytics?chart=income&from=2026-09-01&to=2026-09-03')
  const chart = html.slice(
    html.indexOf('aria-label="Period totals"'),
    html.indexOf('analytics-companion'),
  )
  const triggers = chart.match(/<a\b[^>]*data-slot="tooltip-trigger"[^>]*>/g) ?? []
  expect(chart).toContain('aria-label="Chart legend"')
  expect(chart.indexOf('aria-label="Chart legend"')).toBeGreaterThan(
    chart.indexOf('aria-label="Period totals"'),
  )
  // One trigger per income source per day.
  expect(triggers).toHaveLength(7)
  expect(triggers[0]).toContain('Sep 1, 2026 through Sep 1, 2026: $10.00')
  expect(
    triggers.every(
      (trigger) =>
        trigger.includes('analytics-bar-category') && trigger.includes('analysis=income'),
    ),
  ).toBe(true)
})

it('shows Analytics dollar and percentage changes versus the matching previous period', async () => {
  scenario.creditKind = 'income'
  const html = await renderRoute('/analytics?chart=income&from=2026-09-02&to=2026-09-02')
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
  // Subscriptions moved onto Spending, so Recurring is no longer a page of its own.
  expect(selector).not.toContain('>Recurring<')
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

it('shows days until the statement closes in the Spending chart and manual accounts in Settings', async () => {
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
  // The balance reads beside the statement's close; the headline stays what was spent.
  expect(spending).toMatch(
    /Statement balance <span class="sensitive-value">−?\$[\d,.]+<\/span> · closes in 5 days<\/h2>/,
  )
  const settings = await renderSettings('/#settings/manual-accounts')
  expect(settings).toContain('Manual accounts')
  expect(settings).not.toContain('Category rules')
})
