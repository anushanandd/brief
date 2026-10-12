import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { insights } from '../data/fixtures/insights'
import seedInsights from '../data/seed-insights.json'
import seed from '../data/seed.json'
import {
  financeSnapshotSchema,
  investmentSummarySchema,
  type Insights,
  type PortfolioToday,
  type Statement,
} from '../lib/schema'
import { HoldingsTable } from './holdings-table'
import {
  InvestmentsAttention,
  InvestmentsComingUp,
  InvestmentsPerformance,
  PeriodCard,
  PeriodChange,
} from './investments-summary'
import { PeriodTotals } from './period-totals'
import { PortfolioExposure } from './portfolio-exposure'

async function render(
  card: (data: ReturnType<typeof snapshot>) => ReactNode,
  overrides: Partial<Insights>,
) {
  const data = snapshot(overrides)
  const client = new QueryClient()
  const router = createRouter({
    routeTree: createRootRoute({
      component: () => <QueryClientProvider client={client}>{card(data)}</QueryClientProvider>,
    }),
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  await router.load()
  return renderToStaticMarkup(<RouterProvider router={router} />)
}

function snapshot(overrides: Partial<Insights>) {
  const data = financeSnapshotSchema.parse(seed)
  data.insights = insights({ today: '2026-09-16', ...overrides })
  return data
}

const slice = (key: string, label: string, value: number, percent: number) => ({
  key,
  label,
  value,
  percent,
})

it('ranks exposure by holding and notes what the total leaves out', async () => {
  const html = await render((data) => <PortfolioExposure data={data} />, {
    allocation: {
      total: 10000,
      incomplete: true,
      cashUnknown: true,
      kinds: [slice('etf', 'ETFs', 7000, 70), slice('equity', 'Stocks', 3000, 30)],
      securities: [slice('VTI', 'Vanguard Total', 7000, 70), slice('AAPL', 'Apple', 3000, 30)],
      accounts: [],
      largestPercent: 70,
      topFivePercent: 100,
      concentrated: slice('VTI', 'Vanguard Total', 7000, 70),
    },
  })
  // The dominant holding is flagged under Needs you, not repeated here.
  expect(html).not.toContain('VTI is 70.0%')
  // By holding is the portfolio's list of what you own, largest first.
  expect(html).toContain('aria-label="Investments by holding"')
  expect(html.indexOf('<span>VTI</span>')).toBeLessThan(html.indexOf('<span>AAPL</span>'))
  expect(html).toContain('Excludes positions without a USD value. Excludes unknown cash.')
  expect(await render((data) => <PortfolioExposure data={data} />, {})).toContain(
    'No positions with a USD value.',
  )
})

it('shows spent and income for the chart period with maskable figures and links', async () => {
  const period = { start: '2026-09-10', end: '2026-09-16' }
  const recap = {
    window: 'week' as const,
    ...period,
    partial: false,
    spending: 300,
    previousSpending: 200,
    income: 1000,
    net: 700,
    categories: [],
  }
  const html = await render((data) => <PeriodTotals data={data} window="week" />, {
    recap: [recap],
  })
  expect(html).toContain('Past week')
  expect(html).toContain('$300.00')
  // Spending rose 50%, which is unfavorable.
  expect(html).toContain('<small class="negative" aria-label="+50.00% versus the period before">')
  expect(html).toContain('href="/analytics?chart=income&amp;from=2026-09-10&amp;to=2026-09-16"')
  expect(html).not.toContain('transaction')
  // Without a recap for the window, the figures are unavailable rather than zero.
  const empty = await render((data) => <PeriodTotals data={data} window="month" />, {})
  expect(empty).toContain('Past month')
  expect(empty).not.toContain('$0.00')
})

// A synthetic year, reconciled: 200,000 + 14,200 added + 38,100 earned = 252,300.
const yearWindow: Statement = {
  window: 'year',
  start: '2025-09-16',
  end: '2026-09-16',
  startValue: 200000,
  endValue: 252300,
  deposits: 14200,
  withdrawals: 0,
  netFlows: 14200,
  investmentGain: 38100,
  timeWeightedPct: 14.2,
  moneyWeightedPct: 12.9,
  annualized: { timeWeightedPct: 14.2, moneyWeightedPct: 12.9 },
  benchmarkPct: 11,
  activePct: 3.2,
  waterfall: {
    start: 200000,
    deposits: 14200,
    withdrawals: 0,
    realized: null,
    unrealized: null,
    priceGain: 37000,
    dividends: 1100,
    interest: 0,
    fees: 0,
    taxes: 0,
    end: 252300,
  },
  risk: { drawdownPct: null, volatilityPct: null, beta: null },
}
const position = (ticker: string, changePct: number, change: number) => ({
  ticker,
  name: ticker,
  changePct,
  change,
  sector: null,
})
const notableDay: PortfolioToday = {
  change: 2803,
  changePct: 0.68,
  market: { symbol: 'SPY', name: 'S&P 500', changePct: 0.4 },
  vsMarket: 'ahead',
  incomplete: false,
  positions: [position('BIG', 3.1, 2910), position('SMALL', -0.4, -107)],
  notable: true,
  asOf: '2026-09-16T18:00:00Z',
}

it('states the period as its market gain and return, like every chart', async () => {
  const year = await render(() => <PeriodChange span={yearWindow} />, {})
  expect(year).toMatch(/\+\$38,100(\.00)?/)
  expect(year).toContain('(14.20%)')
  expect(await render(() => <PeriodChange />, {})).toContain('Change unavailable')
})

it('backs the period with its evidence and what it paid, never inventing either', async () => {
  const year = await render((data) => <PeriodCard data={data} span={yearWindow} range="year" />, {})
  expect(year).toContain('<h2>The past year</h2>')
  // The figures, whether the card shows them whole or to the cent.
  expect(year).toMatch(/<span class="sensitive-value">\+\$38,100(\.00)?<\/span>/)
  expect(year).toContain('<span>Added</span>')
  expect(year).toMatch(/<span class="sensitive-value">\$14,200(\.00)?<\/span>/)
  expect(year).toMatch(/\+12\.90?%/)
  expect(year).toMatch(/\+11\.00?%/)
  // Each report opens for the same days: the returns window's flows start the day after it.
  expect(year).toContain('href="/analytics?chart=dividends&amp;from=2025-09-17&amp;to=2026-09-16"')
  expect(year).toContain('href="/analytics?chart=realized&amp;from=2025-09-17&amp;to=2026-09-16"')
  expect(year).toContain('Realized P/L')
  const unknown = await render(
    (data) => (
      <PeriodCard
        data={{
          ...data,
          trades: data.trades.map((trade) => ({ ...trade, estimatedRealizedGain: undefined })),
        }}
        span={{ ...yearWindow, moneyWeightedPct: null, benchmarkPct: null }}
        range="year"
      />
    ),
    {},
  )
  expect(unknown.match(/>—</g)).toHaveLength(2)
  // A sale without a known cost leaves realized P/L unavailable, not zero.
  expect(unknown).toContain('Unavailable')
  // Without a returns window the reports still open for the period's days.
  const week = await render((data) => <PeriodCard data={data} range="week" />, {})
  expect(week).toContain('href="/analytics?chart=dividends&amp;from=2026-09-10&amp;to=2026-09-16"')
  // Interest is a Money report, not an investment figure.
  expect(week).not.toContain('chart=interest')
})

it('lists only decisions, and shows nothing when there are none', async () => {
  const html = await render((data) => <InvestmentsAttention data={data} />, {
    investmentAttention: {
      flags: [
        { kind: 'concentrated', ticker: 'BIG', percent: 23.3 },
        {
          kind: 'longTermSoon',
          ticker: 'BIG',
          accountId: 'fidelity',
          date: '2026-10-08',
          units: 40,
          gain: 3900,
        },
        {
          kind: 'harvestableLoss',
          ticker: 'DOWN',
          accountId: 'fidelity',
          loss: -1300,
          percent: -14.9,
        },
      ],
      earningsWatch: [],
    },
  })
  expect(html.indexOf('BIG is 23.3% of investments')).toBeLessThan(
    html.indexOf('40 BIG shares go long-term Oct 8'),
  )
  expect(html).toContain('<span class="sensitive-value">$3,900</span> gain')
  expect(html).toContain('DOWN is 14.9% below cost')
  expect(html).toContain('href="/holdings/DOWN"')
  expect(await render((data) => <InvestmentsAttention data={data} />, {})).not.toContain(
    'Needs you',
  )
})

it('lists every holding largest first with today, gain, and sortable columns', async () => {
  const html = await render(
    (data) => <HoldingsTable data={data} range="week" today={notableDay} />,
    {
      securities: [
        {
          ticker: 'SMALL',
          name: 'Small Co',
          accounts: 1,
          value: 900,
          percent: 9,
          gain: -40,
          gainPercent: -4.26,
        },
        {
          ticker: 'BIG',
          name: 'Big Co Class A',
          accounts: 2,
          value: 9100,
          percent: 91,
          gain: 2100,
          gainPercent: 30,
        },
      ],
    },
  )
  expect(html.indexOf('href="/holdings/BIG"')).toBeLessThan(html.indexOf('href="/holdings/SMALL"'))
  // Names drop their share class; today's move comes from live prices.
  expect(html).toContain('Big Co</small>')
  expect(html).toContain('+3.10%')
  expect(html).toContain('91.0%')
  // Every column sorts through a registered command.
  for (const column of ['holding', 'today', 'period', 'value', 'weight', 'gain', 'news'])
    expect(html).toContain(`data-command="investments.sort.${column}"`)
  expect(html).toContain('aria-label="Sort by Value, descending"')
  // Without the desktop app there is no period change or news, shown as unavailable.
  expect(html).toContain('<span class="muted">—</span>')
})

it('compares every period with the S&P 500 and marks the page’s', async () => {
  const html = await render((data) => <InvestmentsPerformance data={data} range="year" />, {
    // Rust states each period's active return; the card never subtracts.
    periods: [
      { ...yearWindow, window: 'week', timeWeightedPct: -1.2, benchmarkPct: 0.3, activePct: -1.5 },
      yearWindow,
    ],
  })
  expect(html).toContain('<h2>Performance</h2>')
  expect(html).toContain('−1.50 pts')
  expect(html).toContain('+3.20 pts')
  expect(html).toMatch(/aria-pressed="true"[^>]*aria-label="Show year"/)
  expect(html).not.toContain('All time')
})

const dividend = (exDate: string, payableDate?: string) => ({
  ticker: 'BIG',
  exDate,
  payableDate,
  rate: 0.25,
  amount: 10,
  special: false,
})

it('lists what’s coming in the next month and nothing when there is none', async () => {
  const html = await render(
    (data) => <InvestmentsComingUp data={data} earningsConnected={false} />,
    {
      dividends: {
        securities: [],
        annualIncome: 0,
        upcoming: [dividend('2026-09-20', '2026-09-30'), dividend('2026-12-20', '2026-12-30')],
      },
    },
  )
  expect(html).toContain('<h2>What’s ahead</h2>')
  expect(html).toContain('BIG dividend <small>Confirmed</small>')
  expect(html).toContain('Sep 30')
  expect(html).not.toContain('Dec 30')
  expect(
    await render((data) => <InvestmentsComingUp data={data} earningsConnected={false} />, {}),
  ).not.toContain('What’s ahead')
  // The market's own dates come from the summary, each with its status, and open its source.
  const ahead = await render(
    (data) => (
      <InvestmentsComingUp
        data={data}
        earningsConnected={false}
        summary={{
          ...investmentSummarySchema.parse(seedInsights.summary),
          ahead: [
            {
              date: '2026-09-16',
              kind: 'fomc',
              ticker: null,
              title: 'Federal Reserve rate decision',
              status: 'scheduled',
              amount: null,
              url: 'https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm',
            },
          ],
        }}
      />
    ),
    {},
  )
  expect(ahead).toContain('Federal Reserve rate decision <small>Scheduled</small>')
  expect(ahead).toContain('href="https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm"')
})

it('puts gains, the next long-term date, and lots in the Taxes inspector', async () => {
  const { SecurityTaxes } = await import('./holding-details')
  const lots = [
    {
      accountId: 'fidelity',
      ticker: 'VTI',
      coverage: 'partial' as const,
      taxDeferred: false,
      untrackedUnits: 243,
      since: '2025-03-10',
      moneyWeightedPct: 12.5,
      shortTermGain: 0,
      longTermGain: 500,
      lots: [
        {
          acquired: '2025-03-10',
          units: 5,
          cost: 1000,
          value: 1500,
          gain: 500,
          longTerm: true,
          longTermOn: '2026-03-11',
        },
        { acquired: '2026-08-30', units: 2, cost: 700, longTerm: false, longTermOn: '2027-08-31' },
      ],
    },
  ]
  const html = await render((data) => <SecurityTaxes data={data} ticker="VTI" />, { lots })
  expect(html).toContain('Long-term gain')
  expect(html).toContain('+$500.00')
  expect(html).toContain('Next long-term')
  expect(html).toContain('Mar 10, 2025')
  expect(html).toContain('<td class="sensitive-value positive">$500.00</td>')
  expect(html).toContain('Long')
  expect(html).toContain('Short · long Aug 31, 2027')
  expect(html).toContain('Unavailable')
  expect(html).toContain('243 shares untracked; figures cover the rest.')
  expect(html).toContain('your broker may differ')
  expect(await render((data) => <SecurityTaxes data={data} ticker="AAPL" />, { lots })).toContain(
    'Needs this holding’s purchases in your activity.',
  )
  const ira = await render((data) => <SecurityTaxes data={data} ticker="VTI" />, {
    lots: lots.map((lot) => ({ ...lot, taxDeferred: true })),
  })
  expect(ira).toContain('Retirement account: no tax on sales inside it.')
  expect(ira).not.toContain('Short-term gain')
})
