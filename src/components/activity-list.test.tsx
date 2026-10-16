import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import type { ActivityItem } from '../lib/activity'
import {
  ActivityList,
  GroupedActivityList,
  PagedActivityList,
  activityGridWidths,
  activityStatus,
} from './activity-list'

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
    account: index >= 50 ? 'A longer synthetic account' : 'Card',
  }))
  const firstBatch = activityGridWidths(activities.slice(0, 50), '2026-09-20', measureText)
  const allRows = activityGridWidths(activities, '2026-09-20', measureText)
  expect(allRows[2]).toBeGreaterThan(firstBatch[2])
})

it('fits every rare long value in full except the flexible description', () => {
  const activities: ActivityItem[] = Array.from({ length: 40 }, (_, index) => ({
    id: `synthetic-${index}`,
    kind: 'spending',
    category: 'Dining',
    title: 'Coffee',
    detail: '',
    date: '2026-09-20',
    amount: index === 0 ? -123_456.78 : -5,
    account: index === 0 ? 'Individual brokerage' : 'Card',
    location: { city: index === 0 ? 'An Unusually Long Synthetic Town' : 'Oakland', region: 'CA' },
  }))
  const [, , account, , owner, location, amount] = activityGridWidths(
    activities,
    '2026-09-20',
    measureText,
  )
  const typical = activityGridWidths(activities.slice(1), '2026-09-20', measureText)
  // A rare long place, account, and amount still show in full.
  expect(location).toBeGreaterThan(typical[5])
  expect(account).toBeGreaterThan(typical[2])
  expect(amount).toBeGreaterThan(typical[6])
  // So do a rare long subcategory and street address; only the description gives way.
  const named = activities.map((activity, index) =>
    index
      ? activity
      : {
          ...activity,
          title: 'An unusually long synthetic merchant name',
          subcategory: 'An unusually long synthetic subcategory',
          location: {
            city: 'Oakland',
            region: 'CA',
            address: '1234 An Unusually Long Synthetic Street',
          },
        },
  )
  const [, description, , category, , address] = activityGridWidths(
    named,
    '2026-09-20',
    measureText,
  )
  expect(description).toBe(typical[1])
  expect(category).toBeGreaterThan(typical[3])
  expect(address).toBeGreaterThan(typical[5])
  // Me or Other: the same width on every ledger.
  expect(owner).toBe(typical[4])
})

it('sizes every column to its contents, capping only the description', () => {
  const activity: ActivityItem = {
    id: 'long-description',
    kind: 'spending',
    category: 'Dining',
    subcategory: 'A very long synthetic spending subcategory',
    title: 'A considerably longer synthetic merchant description',
    location: {
      city: 'Palo Alto',
      region: 'CA',
      address: '123 An Exceptionally Long Synthetic Street Name',
    },
    paymentChannel: 'in store',
    detail: '',
    date: '2026-09-20',
    amount: -10,
  }
  const [, descriptionWidth, , categoryWidth, , locationWidth] = activityGridWidths(
    [activity],
    '2026-09-20',
    measureText,
  )
  // The description is capped; everything else shows in full, however long.
  expect(descriptionWidth).toBe(280)
  expect(categoryWidth).toBeGreaterThan(280)
  expect(locationWidth).toBeGreaterThan(280)
  const [, shortDescriptionWidth, , shortCategoryWidth, , shortLocationWidth] = activityGridWidths(
    [
      {
        ...activity,
        title: 'Coffee',
        subcategory: undefined,
        location: { city: 'Palo Alto', region: 'CA' },
      },
    ],
    '2026-09-20',
    measureText,
  )
  expect(shortDescriptionWidth).toBeLessThan(descriptionWidth)
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
    '<span class="financial-activity-amount negative" aria-label="Amount: −$5.00"',
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
  expect(compact).toContain('class="negative" data-copy-amount="true">−$5.00</strong>')
  // Lists name the year only when it isn't the snapshot's.
  expect(compact).toContain('<small>Sep 20</small>')
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
  // The place leads; the street sits beneath it, and the full address moves to the detail panel.
  expect(withLocation).toContain('aria-label="Filter by location: San Francisco, CA"')
  expect(withLocation).toMatch(
    /place-place[^>]*>.*?activity-method-mark.*?<span class="financial-activity-cell-text"><span>San Francisco, CA<\/span><small>123 Example St<\/small><\/span>/s,
  )
  expect(withLocation).toContain('<time dateTime="2026-09-20">Sep 20</time>')

  const linked = renderToStaticMarkup(
    <ActivityList
      activities={[
        {
          ...located,
          location: undefined,
          paymentChannel: 'online',
          website: 'https://www.example.com',
        },
      ]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  // The website only names the host; nothing links to it.
  expect(linked).not.toContain('href=')
  expect(linked).toContain('aria-label="Filter by method: Online"')
  expect(linked).toContain('<span>Online</span><small>example.com</small>')

  const other = renderToStaticMarkup(
    <ActivityList
      activities={[{ ...located, location: undefined, paymentChannel: 'other' }]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  expect(other).toContain('aria-label="Location unavailable"')
  expect(other).toMatch(/place-none[^>]*><button[^>]*disabled/)
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
  expect(html).toContain('<strong>Bought a synthetic security in a brokerage account</strong>')
  expect(html).not.toContain('data-base-ui-tooltip-trigger')
  expect(html).toContain('aria-label="Filter by method: Brokerage"')
  expect(html).toMatch(
    /place-brokerage[^>]*>.*?<span class="financial-activity-cell-text"><span>Brokerage<\/span><\/span>/s,
  )

  const opened: string[] = []
  const openable = renderToStaticMarkup(
    <ActivityList
      activities={[
        {
          id: 'trade',
          kind: 'trade',
          category: 'Trade',
          title: 'Bought ABC',
          detail: '',
          date: '2026-09-20',
          amount: -5,
        },
      ]}
      referenceIso="2026-09-20"
      showColumns
      selectedId="trade"
      onOpen={({ id }) => opened.push(id)}
    />,
  )
  expect(openable).toMatch(/aria-haspopup="dialog">Open details: (<!-- -->)?Bought ABC<\/button>/)
  expect(openable).toMatch(/financial-activity-row-columns is-openable"[^>]*data-selected="true"/)
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
  expect(preview).toContain('Brokerage · <span class="negative">−$3.00</span>')
  expect(preview).not.toContain('shares · Estimated FIFO P/L')

  const ledger = renderToStaticMarkup(
    <ActivityList activities={activities} referenceIso="2026-09-20" />,
  )
  expect(ledger).toContain('2 shares · Estimated FIFO P/L $5.00')
})

it('pages a compact list and keeps rows that must stay visible', () => {
  const activities: ActivityItem[] = Array.from({ length: 150 }, (_, i) => ({
    id: `synthetic-${i}`,
    kind: 'spending',
    category: 'Dining',
    title: `Synthetic ${i}`,
    detail: 'Synthetic account',
    date: '2026-09-20',
    amount: -5,
  }))
  const paged = renderToStaticMarkup(
    <PagedActivityList activities={activities} referenceIso="2026-09-20" />,
  )
  expect(paged).toContain('Synthetic 59')
  expect(paged).not.toContain('Synthetic 60<')
  expect(paged).toContain('Load more activities')
  const highlighted = renderToStaticMarkup(
    <PagedActivityList activities={activities} referenceIso="2026-09-20" minimumCount={100} />,
  )
  expect(highlighted).toContain('Synthetic 99<')
  expect(highlighted).not.toContain('Synthetic 100<')
  const all = renderToStaticMarkup(
    <PagedActivityList activities={activities} referenceIso="2026-09-20" minimumCount={150} />,
  )
  expect(all).toContain('Synthetic 149<')
  expect(all).not.toContain('Load more activities')
})

const row = (patch: Partial<ActivityItem>): ActivityItem => ({
  id: 'synthetic',
  kind: 'spending',
  category: 'Dining',
  title: 'Synthetic cafe',
  detail: '',
  date: '2026-09-18',
  amount: -12,
  ...patch,
})

it('marks a pending or flagged row with a badge beside its merchant', () => {
  expect(activityStatus({ pending: true, flag: 'duplicate' })?.label).toBe('Pending')
  expect(activityStatus({ flag: 'duplicate' })).toMatchObject({
    label: 'Possible duplicate',
    warning: true,
  })
  expect(activityStatus({ flag: 'unusual' })?.label).toBe('Unusual amount')
  expect(activityStatus({ flag: 'conflict' })).toMatchObject({ badge: 'Check', warning: true })
  expect(activityStatus({})).toBeUndefined()
  const html = renderToStaticMarkup(
    <ActivityList
      activities={[row({ flag: 'unusual' }), row({ id: 'plain' })]}
      referenceIso="2026-09-20"
      showColumns
    />,
  )
  // The badge follows the merchant, named in full on hover; the date stands alone.
  expect(html).toContain(
    'Synthetic cafe</strong></button><span class="activity-badge is-warning" title="Unusual amount">Unusual</span>',
  )
  expect(html).toContain('aria-label="Filter by date: Sep 18, 2026" disabled=""><time')
  expect(html.match(/activity-badge/g)).toHaveLength(1)
})
