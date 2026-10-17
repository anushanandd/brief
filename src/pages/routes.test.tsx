// The app shell, Home, Settings, recovery, and the command map every control must name.

import { expect, it } from 'vitest'

import { expectedEvent } from '../data/fixtures/insights'
import { settingsSections } from '../lib/settings-search'
import { hasRoute, redirectOf, renderRoute, renderSettings } from './routes.harness'
import { scenario } from './routes.scenario'

it('registers data health without the removed Performance workspace', () => {
  expect(hasRoute('/health')).toBe(true)
  expect(hasRoute('/performance')).toBe(false)
})

it('redirects the former Recurring page to Spending, where subscriptions live', () => {
  expect(redirectOf('/recurring', {})).toMatchObject({ options: { to: '/spending' } })
})

it('redirects the former Platinum page to Amex credits', () => {
  expect(redirectOf('/spending/platinum', {})).toMatchObject({
    options: { to: '/analytics', search: { chart: 'amex-credits' } },
  })
})

it('shows unknown quote timestamps instead of the saved snapshot time', async () => {
  const html = await renderRoute('/logs')
  expect(html).toContain('<strong>SYNTH</strong><time>Unknown</time>')
  expect(html).toContain('Source timestamps')
})

it('opens every Settings section in one searchable dialog over the current page', async () => {
  scenario.annotationWarning = 'Transaction annotations could not be loaded.'
  const html = await renderSettings('/accounts#settings')
  expect(html).toMatch(/<dialog class="settings-dialog" aria-labelledby="[^"]+"/)
  expect(html).toContain('aria-label="Close settings"')
  expect(html).toContain('<h2>Balances</h2>')
  expect(html).toContain('Transaction annotations could not be loaded.')
  expect(html).toMatch(/type="text"[^>]*aria-label="Search settings"[^>]*autoComplete="off"/)
  expect(html).toContain('aria-label="Settings sections"')
  for (const section of settingsSections) {
    expect(html).toContain(`id="${section.id}"`)
    expect(html).toContain(`href="#settings/${section.id}"`)
    for (const entry of section.entries) expect(html).toContain(`id="${entry.id}"`)
  }
  expect(html).toContain('Add credentials')
  expect(html).toContain('aria-label="Start date for Selected card"')
  expect(html).toContain('role="switch"')
})

it('opens explicit recovery without showing a fabricated empty finance page', async () => {
  scenario.recovery = { message: 'Synthetic damaged state', canRestore: false }
  const html = await renderRoute('/activities')
  expect(html).toContain('Local data needs recovery')
  expect(html).toContain('Start new snapshot')
  expect(html).not.toContain('Restore retained snapshot')
  expect(html).not.toContain('Other account transaction')
})

it('links each Home ledger preview to its primary page', async () => {
  const html = await renderRoute('/')
  const main = html.slice(html.indexOf('<main'), html.indexOf('</main>'))
  expect(main).toContain('href="/accounts"')
  expect(main).toMatch(/aria-expanded="false"[^>]*>.*?Credit cards/)
  expect(main).toContain('href="/holdings"')
  expect(main).toContain('href="/activities"')
  expect(main).not.toContain('<h2>Overview</h2>')
  expect(main.match(/class="financial-activity-row/g)).toHaveLength(1)
  expect(main).not.toContain('home-finance-grid')
})

it('heads Home with net worth alone, never a carousel of brokerages', async () => {
  scenario.accountType = 'brokerage'
  const html = await renderRoute('/')
  const start = html.indexOf('home-balance-header')
  const hero = html.slice(start, html.indexOf('</header>', start))
  expect(hero).toContain('Net worth')
  expect(html).not.toContain('home-account-dots')
  expect(html).not.toContain('aria-label="Chart account"')
})

it('places graph ranges at the top right, above the change row', async () => {
  scenario.accountType = 'brokerage'
  scenario.homeHistory = true
  for (const path of ['/accounts?account=selected']) {
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

it('shows as much recent activity on Home as the longer preview beside it', async () => {
  const html = await renderRoute('/')
  // Accounts preview beside Holdings under the chart; recent activity follows in the column beside.
  const accounts = html.slice(
    html.indexOf('accounts-overview-card'),
    html.indexOf('home-activity-card'),
  )
  const activity = html.slice(html.indexOf('home-activity-card'))
  expect(accounts.match(/class="account-row account-group-row tree-parent"/g)).toHaveLength(1)
  expect(accounts).toContain('2 accounts')
  expect(accounts).not.toContain('href="/accounts?account=')
  // No holdings and one account category, so one activity.
  // Each row opens its transaction on Activity.
  expect(activity.match(/class="financial-activity-row is-openable"/g)).toHaveLength(1)
  // Without holdings or a positive balance there is no line to split.
  expect(html).not.toContain('class="share-line"')
})

it('splits holdings by ticker and assets by type under their previews', async () => {
  scenario.snaptradeReference = true
  scenario.accountType = 'brokerage'
  scenario.holdings = [
    {
      ticker: 'AAA',
      name: 'Alpha fund',
      accountId: 'snaptrade:selected',
      shares: 2,
      price: 500,
      value: 1_000,
      costBasis: 800,
      unrealizedGain: 200,
      dailyChangePct: 1,
      weeklyChangePct: null,
      weeklyReferencePrice: null,
      weeklyReferenceDate: null,
      totalChangePct: 25,
      instrumentKind: 'etf',
      color: '#fff',
    },
  ]
  scenario.insights = {
    allocation: {
      total: 1_200,
      incomplete: false,
      cashUnknown: false,
      kinds: [],
      securities: [{ key: 'AAA', label: 'Alpha fund', value: 1_000, percent: 83.33 }],
      accounts: [],
    },
  }
  const html = await renderRoute('/')
  const holdings = html.slice(
    html.indexOf('brokerage-holdings-card'),
    html.indexOf('accounts-overview-card'),
  )
  const accounts = html.slice(
    html.indexOf('accounts-overview-card'),
    html.indexOf('home-companion', html.indexOf('accounts-overview-card')),
  )
  expect(holdings).toContain('aria-label="Holdings by ticker"')
  // Tickers come from the native allocation; uninvested cash closes the line.
  expect(holdings).toContain('aria-label="AAA: $1,000.00, 83.3%"')
  expect(holdings).toContain('aria-label="Cash: $200.00, 16.7%"')
  expect(accounts).toContain('aria-label="Assets by type"')
  // Grouped by account type; the credit card owes, so it has no share of what you hold.
  expect(accounts).toContain('aria-label="Brokerage: $1,200.00, 100.0%"')
  expect(accounts.match(/class="share-line-part"/g)).toHaveLength(1)
})

it('keeps account details and utilities out of the organized sidebar', async () => {
  const html = await renderRoute('/')
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'))
  expect(sidebar).toContain('href="/holdings"')
  expect(sidebar).toContain('href="/activities"')
  expect(sidebar).not.toContain('href="/health"')
  expect(sidebar).not.toContain('href="/logs"')
  expect(sidebar).not.toContain('href="/settings/design"')
  expect(sidebar).not.toContain('Hide values')
  expect(sidebar).toContain('href="#settings"')
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
  const appearance = await renderSettings()
  expect(appearance).toMatch(/role="combobox"[^>]*aria-label="Theme"/)
  expect(appearance).toMatch(/role="combobox"[^>]*aria-label="Live price updates"/)
  expect(appearance).toContain('Live price updates')
  expect(appearance).toContain('Financial backup')
  expect(appearance).toContain('Figures in a backup are unencrypted')
})

it('renders every selector rail as one shared scrollable row', async () => {
  for (const route of ['/analytics', '/spending']) {
    const html = await renderRoute(route)
    expect(html).toMatch(
      /<nav[^>]*class="account-switcher"[^>]*><div class="account-switcher-grid">/,
    )
  }
})

it('points Diagnostics back to Settings', async () => {
  const html = await renderRoute('/logs')
  const header = html.slice(html.indexOf('<header class="page-header'), html.indexOf('</header>'))
  expect(header).toMatch(/href="#settings\/advanced"[^>]*>Settings<\/a>/)
  expect(header).toContain('aria-current="page">Diagnostics')
})

it('opens the design reference from Advanced Settings with isolated sample controls', async () => {
  const settings = await renderSettings()
  expect(settings).toContain('href="/settings/design"')
  const html = await renderRoute('/settings/design')
  expect(html).toContain('aria-current="page">Design')
  expect(html).toContain('href="#settings/advanced"')
  expect(html).toContain('Data colors')
  expect(html).toContain('--data-7')
  expect(html).toContain('aria-label="Sample date range"')
  expect(html).toContain('Example reimbursement')
})

it('shows the coming activity native insights chose and no needs list on Home', async () => {
  scenario.insights = {
    today: '2026-09-16',
    expected: [
      expectedEvent({
        id: 'series:rent',
        title: 'Example rent',
        date: '2026-09-18',
        amount: -1200,
      }),
      expectedEvent({ id: 'series:later', title: 'Example later bill', date: '2026-10-08' }),
    ],
    // Rust decides what needs action and which events are expected.
    brief: {
      needs: [
        {
          id: 'cash',
          severity: 'attention',
          kind: 'cash-low',
          date: '2026-11-20',
          expected: -280,
          low: -440,
        },
      ],
    },
  }
  const html = await renderRoute('/')
  expect(html).toContain('<h1>Home</h1>')
  // Needs are for notifications; Home shows the period's totals and what's coming.
  expect(html).not.toContain('needs-you')
  expect(html).toContain('class="period-totals"')
  // Coming up is the same list Accounts shows: every event in Rust's 30-day forecast.
  const comingUp = html.slice(html.indexOf('expected-activity-card'))
  expect(comingUp).toContain('Example rent')
  expect(comingUp).toContain('−$1,200.00')
  expect(comingUp).toContain('Example later bill')
  expect(html).not.toContain('home-summary')
  expect(html).not.toContain('Heads up')
})

// Every control outside a list row names the registered command that does the same thing, so

// the palette, the menu bar, and the keyboard reach everything the pointer can. Rows are

// covered by the list commands (move, open, copy).

it('maps every control to a registered command', async () => {
  // A person's page has controls of its own.
  scenario.people = [{ id: 'person:a', name: 'Example Person A' }]
  // A holding's page has controls of its own.
  scenario.holdings = [
    {
      ticker: 'NVDA',
      name: 'Synthetic chip maker',
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
  ]
  const { readFileSync, readdirSync } = await import('node:fs')
  const sources = (directory: string): string[] =>
    readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory()
        ? sources(`${directory}/${entry.name}`)
        : /\.tsx?$/.test(entry.name) && !entry.name.includes('.test.')
          ? [readFileSync(`${directory}/${entry.name}`, 'utf8')]
          : [],
    )
  const code = sources('src').join('\n')
  const ids = [...code.matchAll(/id: '([^']+)'/g)].map((match) => match[1])
  const families = [...code.matchAll(/id: `([^`$]+)\$\{/g)].map((match) => match[1])
  const registered = (command: string) =>
    ids.includes(command) ||
    families.some((family) => command.startsWith(family) || family.startsWith(command))
  const voids = new Set([
    'area',
    'br',
    'col',
    'embed',
    'hr',
    'img',
    'input',
    'link',
    'meta',
    'source',
    'wbr',
  ])
  const missing: string[] = []
  for (const route of [
    '/',
    '/accounts',
    '/spending',
    '/analytics?chart=income',
    '/holdings',
    '/holdings/NVDA',
    '/activities',
    '/activities?review=true',
    '/insights',
    '/insights?view=people',
    '/insights?view=people&person=person:a',
    '/analytics?chart=amex-credits',
    '/taxes',
    '/calendar',
    '/documents',
  ]) {
    const html = await renderRoute(route)
    const open: Array<{ tag: string; row: boolean }> = []
    for (const [, closing, tag, attributes, selfClosing] of html.matchAll(
      /<(\/?)([a-zA-Z][\w-]*)([^>]*?)(\/?)>/g,
    )) {
      if (closing) {
        while (open.length && open.pop()?.tag !== tag);
        continue
      }
      const inRow = open.at(-1)?.row ?? false
      const row = inRow || /\sdata-keyboard-row="/.test(attributes)
      const control = tag === 'button' || (tag === 'a' && /\shref="/.test(attributes))
      if (control && !row) {
        const command = /\sdata-command="([^"]+)"/.exec(attributes)?.[1]
        if (!command || !registered(command)) {
          const name =
            /aria-label="([^"]*)"/.exec(attributes)?.[1] ?? /href="([^"]*)"/.exec(attributes)?.[1]
          missing.push(
            `${route}: <${tag}> ${name ?? attributes.slice(0, 60)}${command ? ` (${command} isn't registered)` : ''}`,
          )
        }
      }
      if (!selfClosing && !voids.has(tag)) open.push({ tag, row })
    }
  }
  expect(missing).toEqual([])
})

it('lists what needs action on Home, each opening where to act', async () => {
  scenario.insights = {
    brief: {
      needs: [
        {
          id: 'duplicate:a:b',
          severity: 'attention',
          kind: 'possible-duplicate',
          accountId: 'selected',
          merchant: 'Example Hardware',
          amount: -54.2,
          date: '2026-09-12',
          transactionIds: ['a', 'b'],
        },
        {
          id: 'wash-window:sale',
          severity: 'attention',
          kind: 'wash-window',
          ticker: 'BBB',
          accountId: 'selected',
          soldOn: '2026-09-10',
          until: '2026-10-10',
          loss: -50,
        },
      ],
    },
  }
  const html = (await renderRoute('/')).replaceAll('<!-- -->', '')
  expect(html).toContain('Needs you')
  expect(html).toContain('Possible duplicate at Example Hardware')
  expect(html).toContain('href="/activities?open=b"')
  expect(html).toContain('Don’t buy BBB until Oct 11')
  expect(html).toContain('href="/taxes"')
})
