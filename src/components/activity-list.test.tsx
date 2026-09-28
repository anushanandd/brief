import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import { ActivityList, GroupedActivityList, activityGridWidths } from './activity-list'

const measureText = (value: string) => value.length * 8

it('renders an initial batch and keeps a keyboard-accessible way to load the rest', () => {
  const activities: ActivityItem[] = Array.from({ length: 1000 }, (_, i) => ({
    id: `synthetic-${i}`,
    kind: 'income',
    category: 'Income',
    title: `Synthetic ${i}`,
    detail: 'Synthetic account',
    date: '2026-09-20',
    amount: 10,
  }))
  const html = renderToStaticMarkup(
    <GroupedActivityList activities={activities} referenceIso="2026-09-20" />,
  )
  expect(html.match(/data-keyboard-row/g)).toHaveLength(60)
  expect(html).toContain('Load more activities')
  const filtered = renderToStaticMarkup(
    <GroupedActivityList activities={activities.slice(-2)} referenceIso="2026-09-20" />,
  )
  expect(filtered).toContain('Synthetic 999')
  expect(filtered).not.toContain('Load more activities')
  expect(activities).toHaveLength(1000)
})

it('sizes ledger columns using every filtered row, including later batches', () => {
  const activities: ActivityItem[] = Array.from({ length: 61 }, (_, index) => ({
    id: `synthetic-${index}`,
    kind: 'spending',
    category: 'Dining',
    title: 'Coffee',
    detail: '',
    date: '2026-09-20',
    amount: -5,
    account: index === 60 ? 'A much longer synthetic account display name' : 'Card',
  }))
  const firstBatch = activityGridWidths(activities.slice(0, 60), '2026-09-20', measureText)
  const allRows = activityGridWidths(activities, '2026-09-20', measureText)
  expect(allRows[1]).toBeGreaterThan(firstBatch[1])
})

it('leaves description flexible while sizing the other columns to their contents', () => {
  const activity: ActivityItem = {
    id: 'long-description',
    kind: 'spending',
    category: 'A very long synthetic spending category',
    title: 'A considerably longer synthetic merchant description',
    location: {
      city: 'An Exceptionally Long Synthetic City Name',
      region: 'CA',
      address: '123 Example Street',
    },
    paymentChannel: 'in store',
    detail: '',
    date: '2026-09-20',
    amount: -10,
  }
  const [descriptionWidth, , categoryWidth, locationWidth] = activityGridWidths(
    [activity],
    '2026-09-20',
    measureText,
  )
  expect(descriptionWidth).toBe(180)
  expect(categoryWidth).toBe(188)
  expect(locationWidth).toBe(232)
  const [shortDescriptionWidth, , shortCategoryWidth, shortLocationWidth] = activityGridWidths(
    [
      {
        ...activity,
        title: 'Coffee',
        category: 'Dining',
        location: { city: 'Palo Alto', region: 'CA' },
      },
    ],
    '2026-09-20',
    measureText,
  )
  expect(shortDescriptionWidth).toBe(descriptionWidth)
  expect(shortCategoryWidth).toBeLessThan(categoryWidth)
  expect(shortLocationWidth).toBeLessThan(locationWidth)
})

it('renders ledger columns without changing the shared compact rows', () => {
  const activity: ActivityItem = {
    id: 'coffee',
    kind: 'spending',
    accountId: 'everyday',
    account: 'Everyday card',
    category: 'Dining',
    title: 'Coffee shop',
    detail: 'Everyday card · Dining',
    date: '2026-09-20',
    amount: -5,
  }
  const ledger = renderToStaticMarkup(
    <GroupedActivityList
      activities={[activity]}
      referenceIso="2026-09-20"
      showColumns
      accountTypes={{ everyday: 'credit' }}
      sort={{ column: 'date', direction: 'desc' }}
      onSort={() => undefined}
    />,
  )
  expect(ledger).toContain('<span>Account</span>')
  expect(ledger).toContain('<span>Category</span>')
  expect(ledger).toContain('aria-label="Sort by Date, descending"')
  expect(ledger).toContain('aria-pressed="true"')
  expect(ledger).toContain('Everyday card')
  expect(ledger).toContain('Dining')
  expect(ledger).toContain('account-mark-credit')
  expect(ledger).toContain('category-mark')
  expect(ledger).toContain('var(--spending-food)')
  expect(ledger).toContain(
    '<span class="financial-activity-amount negative" aria-label="Amount: -$5.00"',
  )
  expect(ledger).toContain('aria-label="Filter by date: Sep 20, 2026"')
  expect(ledger).not.toContain('activity-date-mark')
  expect(ledger).toContain('<time dateTime="2026-09-20">Sep 20</time>')
  expect(ledger).not.toContain('>Today</h3>')

  const compact = renderToStaticMarkup(
    <GroupedActivityList activities={[activity]} referenceIso="2026-09-20" />,
  )
  expect(compact).not.toContain('financial-activity-header')
  expect(compact).toContain('Everyday card · Dining')
  expect(compact).toContain('class="negative">-$5.00</strong>')
  expect(compact).toContain('Sep 20, 2026')
})

it('shows a supplied location and leaves unknown locations unavailable', () => {
  const located: ActivityItem = {
    id: 'located',
    kind: 'spending',
    category: 'Dining',
    title: 'Synthetic purchase',
    detail: 'Synthetic card',
    date: '2026-09-20',
    location: {
      address: '123 Example St',
      city: 'San Francisco',
      region: 'CA',
      postalCode: '94103',
    },
    paymentChannel: 'in store',
    amount: -5,
  }
  const withLocation = renderToStaticMarkup(
    <ActivityList activities={[located]} referenceIso="2026-09-20" showColumns />,
  )
  expect(withLocation).toContain(
    'aria-label="Full address: 123 Example St · San Francisco, CA · 94103"',
  )
  expect(withLocation).toContain('aria-label="Filter by location: San Francisco, CA"')
  expect(withLocation).not.toContain('financial-activity-method')
  expect(withLocation).toMatch(
    /financial-activity-location-label[^>]*aria-label="Filter by location: San Francisco, CA"[^>]*>.*?activity-method-mark[^>]*><svg.*?<\/svg><\/span><span>San Francisco, CA<\/span>/s,
  )
  expect(withLocation).not.toContain('<small>123 Example St</small>')
  expect(withLocation).toContain('<time dateTime="2026-09-20">Sep 20</time>')

  const linked = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...located, website: 'https://example.com' }]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  expect(linked).toContain('123 Example St · San Francisco, CA · 94103')
  expect(linked).toContain('aria-label="Open Synthetic purchase website"')
  expect(linked.match(/<span[^>]*class="financial-activity-address-trigger"[^>]*>/)?.[0]).toContain(
    'tabindex="0"',
  )

  const channelOnly = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...located, location: undefined, paymentChannel: 'online' }]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  expect(channelOnly).toMatch(
    /financial-activity-location-label[^>]*aria-label="Filter by method: Online"[^>]*>.*?activity-method-mark[^>]*><svg.*?<\/svg><\/span><span>Online<\/span>/s,
  )
  expect(channelOnly).toContain('>Online</span>')
  expect(channelOnly).not.toContain('123 Example St')

  const other = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...located, location: undefined, paymentChannel: 'other' }]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  expect(other).toMatch(/activity-method-mark[^>]*><svg.*?<\/svg><\/span><span>Other<\/span>/s)
  expect(other).toContain('>Other</span>')

  const unavailable = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...located, location: undefined, paymentChannel: undefined }]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  expect(unavailable).toContain('aria-label="Location and method unavailable"')
})

it('keeps the full description in the accessible name without a tooltip and labels brokerage trades', () => {
  const html = renderToStaticMarkup(
    <ActivityList
      activities={[
        {
          id: 'trade',
          kind: 'trade',
          category: 'Trade',
          title: 'Bought a synthetic security in a brokerage account',
          detail: '',
          date: '2026-09-20',
          amount: -5,
        },
      ]}
      referenceIso="2026-09-20"
      showColumns
      onFilter={() => undefined}
    />,
  )
  expect(html).toContain(
    'aria-label="Filter by description: Bought a synthetic security in a brokerage account"',
  )
  expect(html).toContain('Bought a synthetic security in…')
  expect(html).not.toContain('data-base-ui-tooltip-trigger')
  expect(html).toContain('aria-label="Filter by method: Brokerage"')
  expect(html).toMatch(/activity-method-mark[^>]*><svg.*?<\/svg><\/span><span>Brokerage<\/span>/s)
})

it('uses signed gain colors only in the account activity preview', () => {
  const activities: ActivityItem[] = [
    {
      id: 'gain',
      kind: 'trade',
      account: 'Brokerage',
      category: 'Trade',
      title: 'Sold ABC',
      detail: 'Brokerage · 2 shares · Estimated FIFO P/L $5.00',
      date: '2026-09-20',
      amount: 20,
      estimatedRealizedGain: 5,
    },
    {
      id: 'loss',
      kind: 'trade',
      account: 'Brokerage',
      category: 'Trade',
      title: 'Sold XYZ',
      detail: 'Brokerage · 1 shares · Estimated FIFO P/L -$3.00',
      date: '2026-09-20',
      amount: 10,
      estimatedRealizedGain: -3,
    },
  ]
  const preview = renderToStaticMarkup(
    <ActivityList activities={activities} referenceIso="2026-09-20" conciseTradeGain />,
  )
  expect(preview).toContain('Brokerage · <span class="positive">+$5.00</span>')
  expect(preview).toContain('Brokerage · <span class="negative">-$3.00</span>')
  expect(preview).not.toContain('shares · Estimated FIFO P/L')

  const ledger = renderToStaticMarkup(
    <ActivityList activities={activities} referenceIso="2026-09-20" />,
  )
  expect(ledger).toContain('2 shares · Estimated FIFO P/L $5.00')
})
