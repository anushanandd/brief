// Holdings and Taxes.

import { expect, it } from 'vitest'

import { formatUpdatedAt } from '../lib/format'
import { renderRoute, redirectOf } from './routes.harness'
import { scenario } from './routes.scenario'

it('gives each holding its own page, and lists every security once on the overview', async () => {
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
  scenario.insights = {
    securities: [
      {
        ticker: 'AAA',
        name: 'Alpha fund',
        accounts: 1,
        value: 100,
        percent: 71.4,
        gain: 20,
        gainPercent: 25,
      },
      {
        ticker: 'BBB',
        name: 'Beta asset',
        accounts: 1,
        value: 40,
        percent: 28.6,
        gain: -10,
        gainPercent: -20,
      },
    ],
  }
  const html = await renderRoute('/holdings/AAA')
  const heading = html.slice(html.indexOf('<h1'), html.indexOf('</h1>'))
  expect(heading).toContain('href="/holdings"')
  expect(heading).toContain('aria-current="page">AAA')
  // Links from before holdings had their own pages still arrive at the holding.
  expect(redirectOf('/holdings', { search: { ticker: 'BBB' } })).toMatchObject({
    options: { to: '/holdings/$ticker', params: { ticker: 'BBB' } },
  })
  expect(redirectOf('/holdings', { search: {} })).toBeUndefined()
  const selected = await renderRoute('/holdings/BBB')
  expect(selected.slice(selected.indexOf('<h1'), selected.indexOf('</h1>'))).toContain(
    'aria-current="page">BBB',
  )
  // The page's one period is the app's chart range, chosen in the price chart's card.
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  expect(html).not.toContain('aria-label="Security price range"')
  const holdingChart = html.slice(html.indexOf('holding-chart-card'))
  const holdingSummary = holdingChart.slice(
    holdingChart.indexOf('chart-summary-row'),
    holdingChart.indexOf('</header>'),
  )
  expect(holdingSummary).not.toContain('chart-range-select')
  expect(holdingSummary).toContain('Change unavailable')
  expect(holdingChart.slice(0, holdingChart.indexOf('chart-summary-row'))).toContain(
    'aria-label="Price chart details"',
  )
  expect(html).not.toContain('aria-label="Chart style"')
  // Your position is a card beside the price it is worth; where it's held sits beside the news.
  const position = html.slice(html.indexOf('position-card'))
  expect(position).toContain('<h2>Your position</h2>')
  expect(position).toContain('71.4%')
  expect(selected).toContain('28.6%')
  expect(html).toContain('<h2>Held in</h2>')
  expect(html).toContain('+25.00%')
  expect(selected).toContain('−20.00%')
  expect(html).toContain('<span>Next earnings</span>')
  expect(html).toContain('<strong>Bought AAA</strong>')
  expect(selected).toContain('<strong>Sold BBB</strong>')
  expect(html).toContain('aria-label="AAA activity"')
  expect(html).toContain('−$37.00')
  expect(html).not.toContain('$29.00')
  expect(selected).toContain('$29.00')
  expect(selected).not.toContain('-$37.00')
  // The holding page is about one security; the list of all of them lives on the overview.
  expect(html).not.toContain('aria-label="Held securities"')
  expect(html).not.toContain('holdings-table')
  expect(await renderRoute('/holdings/ZZZ')).toContain('ZZZ isn’t held')

  // The holding's summary sits under its price chart, beside the position, before the news.
  expect(html.indexOf('holding-chart-card')).toBeLessThan(html.indexOf('investment-summary-card'))
  expect(html.indexOf('investment-summary-card')).toBeLessThan(html.indexOf('position-card'))
  expect(html.indexOf('position-card')).toBeLessThan(html.indexOf('holding-news-card'))
  const summaryCard = html.slice(html.indexOf('investment-summary-card'))
  expect(summaryCard).toContain('<h2>Summary</h2>')
  // It needs the desktop app, and the preview says so instead of inventing a period.
  expect(summaryCard).toMatch(/Needs the desktop app\.|No return history for this period\./)
  // The strip heading the page is the selector: the portfolio, then each holding by value.
  const rail = html.slice(
    html.indexOf('aria-label="Holdings"'),
    html.indexOf('</nav>', html.indexOf('aria-label="Holdings"')),
  )
  expect(rail.indexOf('href="/holdings"')).toBeLessThan(rail.indexOf('href="/holdings/AAA"'))
  expect(rail.indexOf('href="/holdings/AAA"')).toBeLessThan(rail.indexOf('href="/holdings/BBB"'))
  expect(rail.match(/href="\/holdings\/AAA"/g)).toHaveLength(1)
  expect(rail).toMatch(
    /aria-current="page"[^>]*href="\/holdings\/AAA"|href="\/holdings\/AAA"[^>]*aria-current="page"/,
  )
  // The period is chosen in the price chart's card.
  expect(holdingChart).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  // Taxes open on request, not on every look at a holding.
  expect(html).not.toContain('<h2>Taxes</h2>')
  expect(html).toMatch(/aria-haspopup="dialog"[^>]*>(<svg[\s\S]*?<\/svg>)?Taxes<\/button>/)
  // What needs attention belongs to the whole portfolio, so only the overview shows it.
  expect(html).not.toContain('<h2>Needs you</h2>')
  const overview = await renderRoute('/holdings')
  const overviewRail = overview.slice(overview.indexOf('aria-label="Holdings"'))
  expect(overviewRail).toMatch(
    /aria-current="page"[^>]*href="\/holdings"|href="\/holdings"[^>]*aria-current="page"/,
  )
  expect(overview).not.toContain('holding-chart-card')
  expect(overview).not.toContain('position-card')
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

it('heads the page with its holdings, puts the summary under the chart and the period beside them, then every holding, then exposure beside performance', async () => {
  scenario.insights = {
    investmentAttention: {
      flags: [{ kind: 'concentrated', ticker: 'AAA', percent: 40 }],
      earningsWatch: [],
    },
  }
  const overview = await renderRoute('/holdings')
  const order = [
    'investments-portfolio-card',
    'investment-summary-card',
    'investments-side-card',
    '<h2>Needs you</h2>',
    'holdings-table-card',
    '<h2>Exposure</h2>',
    '<h2>Performance</h2>',
  ]
  const positions = order.map((marker) => overview.indexOf(marker))
  expect(Object.fromEntries(order.map((marker, index) => [marker, positions[index] > -1]))).toEqual(
    Object.fromEntries(order.map((marker) => [marker, true])),
  )
  expect(positions).toEqual(positions.toSorted((a, b) => a - b))
  // The holdings strip replaces the reports strip; the period card opens the reports.
  expect(overview).toContain('aria-label="Holdings"')
  expect(overview).not.toContain('aria-label="Overview and reports"')
  // The summary has no button: it is made in the background.
  expect(overview).not.toContain('Summarize the news')
  // The period card opens each report for the page's period; returns aren't repeated.
  expect(overview).not.toContain('investment-returns')
  const period = overview.slice(overview.indexOf('investments-side-card'))
  expect(period).toContain('aria-label="Reports"')
  expect(period).toMatch(/href="\/analytics\?chart=realized&amp;[^"]*"/)
  // One period, the app's chart range chosen in the portfolio card, decides every card.
  const portfolio = overview.slice(overview.indexOf('investments-portfolio-card'))
  expect(portfolio).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  expect(overview.match(/aria-label="Chart range"/g)).toHaveLength(1)
  expect(overview).not.toContain('aria-label="Investment account"')
  expect(overview).not.toContain('aria-label="Summary period"')
  // The page that briefly split this one off now leads back here.
  expect(redirectOf('/holdings/performance', {})).toMatchObject({ options: { to: '/holdings' } })
})

const taxYear2026 = {
  year: 2026,
  interest: 18.42,
  dividends: { total: 128.19, qualified: 34.03, nonQualified: 0, unclassified: 94.16 },
  realized: {
    shortTerm: -412.3,
    longTerm: 620.4,
    washDisallowed: 0,
    sales: 2,
    unestimatedSales: 0,
    unestimatedProceeds: 0,
  },
  washSales: [],
  tablesAvailable: true,
  partial: false,
}

it('previews the 1099s and asks for a tax profile before estimating', async () => {
  scenario.insights = {
    taxes: {
      years: [taxYear2026],
      harvest: [{ ticker: 'AAA', accountId: 'selected', shortTermLoss: -600, longTermLoss: 0 }],
      washWindows: [
        {
          saleId: 'sale',
          ticker: 'BBB',
          accountId: 'selected',
          soldOn: '2026-09-10',
          until: '2026-10-10',
          loss: -50,
        },
      ],
      hasProfile: false,
      tablesVersion: '2026-10-07',
      sources: ['Rev. Proc. 2025-32'],
    },
  }
  const html = (await renderRoute('/taxes')).replaceAll('<!-- -->', '')
  expect(html).toContain('1099 preview')
  expect(html).toContain('Add tax profile')
  expect(html).toContain('href="#settings/tax-filing-status"')
  expect(html).toContain('Don’t buy BBB through Oct 10')
  expect(html).toContain('href="/holdings/AAA"')
  expect(html).toContain('Estimated tax · 2026')
})

it('shows an estimate as a range while dividends are unclassified', async () => {
  scenario.insights = {
    taxes: {
      years: [
        {
          ...taxYear2026,
          estimate: {
            low: 1_170,
            high: 1_510,
            ordinary: 660,
            capitalGains: 525,
            niit: 0,
            state: 325,
            marginalRate: 27,
            capitalGainsRate: 20,
            lossDeduction: 0,
            lossCarryover: 0,
          },
        },
      ],
      harvest: [],
      washWindows: [],
      hasProfile: true,
      tablesVersion: '2026-10-07',
      sources: [],
    },
  }
  const html = (await renderRoute('/taxes')).replaceAll('<!-- -->', '')
  expect(html).toContain('$1,170.00 – $1,510.00')
  expect(html).toContain('27% on interest, 20% on')
  expect(html).toContain('A range until dividends are classified.')
  expect(html).not.toContain('Add tax profile')
})

it('opens Retirement from the strip with each limit, what waits for a choice, and what is unavailable', async () => {
  scenario.insights = {
    retirement: {
      year: 2026,
      workplace: {
        accountIds: ['selected'],
        limit: 24_500,
        deferred: 6_000,
        unlabeled: 0,
        employer: 1_200,
        through: '2026-08-28',
        paychecksLeft: 9,
        projected: 10_500,
        perPaycheckToMax: 2_055.55,
      },
      ira: [
        { taxYear: 2025, contributed: 7_000, limit: 7_000, deadline: '2026-04-15' },
        { taxYear: 2026, contributed: 3_500, deadline: '2027-04-15' },
      ],
      deposits: [
        {
          transactionId: 'roth-in',
          accountId: 'other',
          date: '2026-02-10',
          amount: 1_000,
          flow: 'contribution',
          years: [2025, 2026],
          chosen: false,
          stated: false,
          waiting: true,
        },
        {
          transactionId: 'roth-may',
          accountId: 'other',
          date: '2026-05-12',
          amount: 2_500,
          flow: 'contribution',
          years: [2026],
          countsFor: 2026,
          chosen: false,
          stated: false,
          waiting: false,
        },
      ],
      buckets: { preTax: 6_000, taxFree: 9_000, taxable: 0, complete: false },
      rothBasis: [
        { accountId: 'other', since: '2025-06-01', contributed: 4_500, withdrawn: 0, basis: 4_500 },
      ],
      savings: [
        {
          year: 2026,
          from: '2026-01-01',
          through: '2026-09-03',
          partial: false,
          takeHome: 40_000,
          spending: 20_000,
          deferred: 6_000,
          espp: 0,
          rate: 56.52,
        },
      ],
      uncounted: [],
    },
  }
  const html = (await renderRoute('/holdings/retirement')).replaceAll('<!-- -->', '')
  expect(html).toContain('href="/holdings/retirement"')
  expect(html).toContain('Selected card · 2026')
  expect(html).toContain('of $24,500.00')
  expect(html).toContain('$2,055.55 a paycheck to max, 9 left')
  expect(html).toContain('$1,200.00 from your employer, outside the limit')
  expect(html).toContain('2026 limits aren’t published yet')
  expect(html).toContain('Until Apr 15, 2027')
  expect(html).toContain('Can count for 2025 or 2026')
  expect(html).toContain('aria-label="Where the Feb 10 deposit counts"')
  // A deposit Brief placed reads as where it counts, and can still be changed.
  expect(html).toContain('aria-label="Where the May 12 deposit counts"')
  expect(html).toContain('>Counts for 2026<')
  expect(html).toContain('56.5%')
  expect(html).toContain('href="#settings/retirement-earlier-contributions"')
  expect(html).toContain('An account has no value, so these are incomplete.')
  expect(html).toContain('Needs a full year of spending history.')
})
