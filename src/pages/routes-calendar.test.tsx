import { expect, it } from 'vitest'

import { renderRoute } from './routes.harness'
import { scenario } from './routes.scenario'

it('lays out a month of dated items and the picked day', async () => {
  scenario.insights = {
    agenda: [
      {
        id: 'due:selected:2026-10-14',
        kind: 'paymentDue',
        date: '2026-10-14',
        accountId: 'selected',
        amount: -250,
        estimated: false,
      },
      {
        id: 'bill:rent:2026-10-14',
        kind: 'bill',
        date: '2026-10-14',
        title: 'EXAMPLE RENT',
        accountId: 'selected',
        amount: -1200,
        amountLow: -1100,
        amountHigh: -1300,
        frequency: 'Monthly',
        estimated: true,
      },
      {
        id: 'trip:trip-a:2026-10-20',
        kind: 'trip',
        date: '2026-10-20',
        end: '2026-10-22',
        title: 'Example City',
        amount: -300,
        estimated: false,
      },
    ],
  }
  const html = await renderRoute('/calendar?day=2026-10-14')
  expect(html).toContain('October 2026')
  // Six weeks from the Sunday before the first.
  expect(html).toContain('aria-label="Sunday, September 27"')
  expect(html).toContain('aria-label="Saturday, November 7"')
  expect(html).toMatch(/aria-selected="true"[^>]*aria-label="Wednesday, October 14"/)
  expect(html).toContain('<h2>Wednesday, October 14</h2>')
  // Signed amounts read red or green.
  expect(html).toMatch(/calendar-entry-amount sensitive-value negative"[^>]*>−\$250\.00/)
  // The picked day lists Brief's items with their pages, naming accounts and descriptors.
  expect(html).toContain('Selected card payment due')
  expect(html).toContain('Example Rent due')
  // Each item says its account, how often it repeats, and the range it has posted in.
  expect(html).toContain('Bill · Selected card · Monthly · $1,100.00–$1,300.00 · estimated')
  expect(html).toContain('href="/accounts?account=selected"')
  // A trip is one band across its days, labeled where it starts.
  expect(html.match(/calendar-band/g)).toHaveLength(3)
  expect(html).toMatch(/calendar-band is-start"[^>]*>Trip: Example City/)
  // The colors are keyed beneath the month.
  expect(html).toContain('aria-label="Colors"')
  // The user's own calendars are an addition, and need the app.
  expect(html).toMatch(/<button[^>]* disabled=""[^>]*data-command="calendar.list.toggle"/)
  expect(html).toContain('title="Your calendars show in the app."')
})

it('shows each day’s activity by default, in the month and the day’s list', async () => {
  const html = await renderRoute('/calendar?day=2026-09-01')
  // Purchases wear their category's color in the month.
  expect(html).toMatch(/calendar-chip is-activity" style="--chip-color:var\(--/)
  expect(html).toMatch(/Resy credit \d/)
  // The day's activity uses the shared rows, which open the activity's details.
  expect(html).toContain('financial-activity-row is-openable')
  expect(html).toContain('href="/activities?from=2026-09-01&amp;to=2026-09-01"')
})
