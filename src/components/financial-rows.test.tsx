import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
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

it.each(['positions', 'market', 'summary'] as const)(
  'keeps security links, coverage and neutral zero changes in the %s view',
  (view) => {
    const html = renderToStaticMarkup(
      <PositionTable positions={[position]} view={view} externalLogosEnabled={false} />,
    )
    expect(html).toContain('href="https://finance.yahoo.com/quote/TEST/"')
    expect(html).toContain('USD valuation unavailable')
    expect(html).toContain('data-keyboard-row="true"')
    expect(html).toContain('data-keyboard-open="true"')
    expect(html).not.toContain(' positive')
    expect(html).toContain('muted')
    expect(html).toContain('—')
  },
)

it('marks public security prices and market changes separately from position values', () => {
  const holding = {
    ...position,
    price: 25,
    value: 53.13,
    dailyChangePct: 1,
    weeklyChangePct: 2,
  }
  const market = renderToStaticMarkup(
    <PositionTable positions={[holding]} view="market" externalLogosEnabled={false} />,
  )
  const positions = renderToStaticMarkup(
    <PositionTable positions={[holding]} view="positions" externalLogosEnabled={false} />,
  )
  expect(market.match(/class="public-market-value"/g)).toHaveLength(3)
  expect(market).toContain('class="public-market-value">$25.00</td>')
  expect(market).toContain('<td>$53.13</td>')
  expect(positions.match(/class="public-market-value"/g)).toHaveLength(1)
})

it('keeps unmatched sales visible without inventing profit or a current-year date', () => {
  const html = renderToStaticMarkup(
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
  expect(html).toContain('href="https://finance.yahoo.com/quote/TEST/"')
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
  for (const compact of [false, true]) {
    const html = renderToStaticMarkup(
      <ActivityList
        activities={activities}
        referenceIso={referenceIso}
        compact={compact}
        showDescriptions
      />,
    )
    expect(html).toContain('Dec 31, 2025 · Pending')
    expect(html).toContain('Synthetic card · Dining')
    expect(html).toContain('Original provider description')
    expect(html).toContain('href="https://example.com/"')
    expect(html).not.toContain('Verified')
  }
  const unsafe = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...activities[0], website: 'javascript:alert(1)' }]}
      referenceIso={referenceIso}
    />,
  )
  expect(unsafe).not.toContain('href=')
})

it('limits displayed activity names to five words without changing the source title', () => {
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
  expect(html).toContain('<strong>Example airline fee reimbursement for…</strong>')
  expect(activity.title).toBe('Example airline fee reimbursement for a synthetic purchase')
})
