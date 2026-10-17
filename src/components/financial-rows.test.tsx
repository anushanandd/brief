import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { benefitActivity, insights } from '../data/fixtures/insights'
import { buildActivities } from '../lib/activity'
import type { FinanceSnapshot } from '../lib/schema'
import { ActivityList } from './activity-list'
import { PositionTable } from './position-table'
import { SalesTable } from './sales-table'

const position: FinanceSnapshot['holdings'][number] = {
  accountId: 'synthetic',
  ticker: 'TEST',
  name: 'Synthetic security',
  shares: 2.125,
  price: null,
  value: null,
  costBasis: 50,
  unrealizedGain: null,
  dailyChangePct: 0,
  weeklyChangePct: null,
  weeklyReferenceDate: null,
  weeklyReferencePrice: null,
  totalChangePct: 0,
  color: '#fff',
  valuationNote: 'USD valuation unavailable',
}

/** Security links are router links, so tables render inside a router. */
async function renderWithRouter(view: ReactNode) {
  const router = createRouter({
    routeTree: createRootRoute({ component: () => view }),
    history: createMemoryHistory(),
  })
  await router.load()
  return renderToStaticMarkup(<RouterProvider router={router} />)
}

it.each(['positions', 'market', 'summary'] as const)(
  'keeps security links, coverage and neutral zero changes in the %s view',
  async (view) => {
    const html = await renderWithRouter(
      <PositionTable positions={[position]} view={view} externalLogosEnabled={false} />,
    )
    expect(html).toContain('href="/holdings/TEST"')
    expect(html).toContain('USD valuation unavailable')
    expect(html).toContain('data-keyboard-row="true"')
    expect(html).toContain('data-keyboard-open="true"')
    expect(html).not.toContain(' positive')
    expect(html).toContain('muted')
    expect(html).toContain('—')
  },
)

it('marks public security prices and market changes separately from position values', async () => {
  const holding = {
    ...position,
    price: 25,
    value: 53.13,
    dailyChangePct: 1,
    weeklyChangePct: 2,
  }
  const market = await renderWithRouter(
    <PositionTable positions={[holding]} view="market" externalLogosEnabled={false} />,
  )
  const positions = await renderWithRouter(
    <PositionTable positions={[holding]} view="positions" externalLogosEnabled={false} />,
  )
  expect(market.match(/class="public-market-value"/g)).toHaveLength(3)
  expect(market).toContain('class="public-market-value">$25.00</td>')
  expect(market).toContain('<td>$53.13</td>')
  expect(positions.match(/class="public-market-value"/g)).toHaveLength(1)
})

it('keeps unmatched sales visible without inventing profit or a current-year date', async () => {
  const html = await renderWithRouter(
    <SalesTable
      externalLogosEnabled={false}
      referenceIso="2026-09-03T12:00:00Z"
      sales={[
        {
          id: 'sale',
          accountId: 'synthetic',
          account: 'Synthetic account',
          ticker: 'TEST',
          type: 'SELL',
          date: '2025-08-03',
          amount: 100,
          units: 2.125,
          price: 40,
          realizedCostBasis: null,
          estimatedRealizedGain: null,
          estimatedRealizedGainPct: null,
        },
      ]}
    />,
  )
  expect(html).toContain('Aug 3, 2025')
  expect(html).toContain('Lot match unavailable')
  expect(html).toContain('2.125')
  expect(html).toContain('$100.00')
  expect(html).toContain('href="/holdings/TEST"')
  expect(html).not.toContain(' positive')
})

it('preserves pending status, merchant links, categories and descriptions across activity views', () => {
  const referenceIso = '2026-09-03T12:00:00Z'
  const activities = buildActivities({
    updatedAt: referenceIso,
    trades: [],
    transactions: [
      {
        id: 'pending',
        merchant: 'Synthetic merchant',
        category: 'Dining',
        date: '2025-12-31',
        account: 'Synthetic card',
        accountId: 'card',
        amount: -12,
        classification: classification('expense'),
        pending: true,
        website: 'https://example.com/',
        description: 'Original provider description',
      },
    ],
  })
  const html = renderToStaticMarkup(
    <ActivityList activities={activities} referenceIso={referenceIso} showDescriptions />,
  )
  // Pending is a badge beside the merchant, not a suffix on the date.
  expect(html).toContain('<span class="activity-badge" title="Pending">Pending</span>')
  expect(html).toContain('<small>Dec 31, 2025</small>')
  expect(html).toContain('Synthetic card · Dining')
  expect(html).toContain('Original provider description')
  // A merchant's website is never a link out of the row.
  expect(html).not.toContain('href=')
  expect(html).not.toContain('Verified')
})

it('shows the whole activity name and leaves cutting it to the column width', () => {
  const activity = {
    id: 'example',
    kind: 'credit' as const,
    category: 'Credit',
    title: 'Example airline fee reimbursement for a synthetic purchase',
    detail: 'Example card · Credit',
    date: '2026-09-20',
    amount: 25,
    classification: classification('other'),
  }
  const html = renderToStaticMarkup(
    <ActivityList activities={[activity]} referenceIso="2026-09-20" />,
  )
  expect(html).toContain(
    '<strong>Example airline fee reimbursement for a synthetic purchase</strong>',
  )
  expect(activity.title).toBe('Example airline fee reimbursement for a synthetic purchase')
})

it('notes native benefit credits on purchase and credit rows and marks linked rows', () => {
  const referenceIso = '2026-08-20T12:00:00Z'
  const row = {
    account: 'Synthetic card',
    accountId: 'card',
    category: 'Entertainment',
    pending: false,
  }
  const activities = buildActivities({
    updatedAt: referenceIso,
    trades: [],
    transactions: [
      {
        ...row,
        id: 'purchase',
        merchant: 'Synthetic stream',
        date: '2026-08-15',
        amount: -13.99,
        classification: classification('expense'),
      },
      {
        ...row,
        id: 'credit',
        merchant: 'Synthetic credit',
        date: '2026-08-17',
        amount: 13.99,
        classification: classification('other', { credit: true }),
      },
    ],
    insights: insights({
      benefits: {
        accountId: 'card',
        product: 'amex-platinum',
        productName: 'American Express Platinum',
        windows: [],
        useFirst: [],
        history: [],
        review: [],
        activity: [
          benefitActivity({
            id: 'purchase',
            kind: 'purchase',
            merchant: 'Synthetic stream',
            date: '2026-08-15',
            amount: -13.99,
            benefitName: 'Digital entertainment',
            pairedId: 'credit',
            credit: { state: 'credited', amount: 13.99 },
          }),
          benefitActivity({
            id: 'credit',
            date: '2026-08-17',
            amount: 13.99,
            benefitName: 'Digital entertainment',
            pairedId: 'purchase',
          }),
        ],
      },
    }),
  })
  const html = renderToStaticMarkup(
    <ActivityList
      activities={activities}
      referenceIso={referenceIso}
      highlightedIds={new Set(['spending:credit'])}
    />,
  )
  // A short badge beside the merchant, the full note behind it.
  expect(html).toContain(
    'title="Digital entertainment · Credited $13.99">Credited<span class="sr-only">',
  )
  expect(html).toContain(
    'title="Digital entertainment credit for Synthetic stream on Aug 15">Benefit credit',
  )
  expect(html.match(/data-highlighted="true"/g)).toHaveLength(1)
})
