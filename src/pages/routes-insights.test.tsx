import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { transaction } from '../data/fixtures/records'
import { renderRoute } from './routes.harness'
import { scenario } from './routes.scenario'

/** The cards of Insights' strip marked as the page shown. */
function currentTabs(html: string) {
  const start = html.indexOf('<nav class="account-switcher" aria-label="Insights"')
  const strip = html.slice(start, html.indexOf('</nav>', start))
  return [...strip.matchAll(/<a [^>]*aria-current="page"[^>]*>.*?title="([^"]+)"/g)].map(
    (match) => match[1],
  )
}

/** Where the synthetic trips leave from. */
const home = {
  city: 'Oakland',
  region: 'CA',
  country: 'US',
  lat: 37.8,
  lon: -122.27,
  start: '2025-01-01',
  end: '2026-09-30',
}

/** A synthetic trip, flown or driven. */
const trip = (id: string, start: string, end: string, flew: boolean) => ({
  id,
  start,
  end,
  days: [
    { date: start, spent: -80, purchases: 2 },
    ...(end === start ? [] : [{ date: end, spent: -40, purchases: 1 }]),
  ],
  perDay: end === start ? -120 : -40,
  destination: `Example ${id}`,
  abroad: true,
  total: -120,
  gettingThere: 0,
  staying: 0,
  onTheGround: -120,
  categories: [{ name: 'Dining', amount: -120, share: 100 }],
  rest: 0,
  restShare: 0,
  transactionIds: ['transaction-0'],
  bookings: [],
  ongoing: false,
  flew,
  stops: [
    { city: 'Lisbon', region: '14', country: 'PT', purchases: 3, first: start },
    { city: 'Madrid', region: '29', country: 'ES', purchases: 2, first: end },
  ],
})

it('opens Trips as a year: its calendar, a row per trip, and places', async () => {
  scenario.insights = {
    trips: {
      trips: [
        trip('a', '2026-08-10', '2026-08-12', true),
        trip('b', '2026-06-01', '2026-06-02', false),
      ],
      years: [{ year: 2026, count: 2, days: 5, spent: 240, abroad: 2, flown: 1, longest: 'a' }],
      bases: [home],
    },
  }
  const html = await renderRoute('/insights')
  // Trips is one of the cards in Activity's strip, the current one here.
  expect(currentTabs(html)).toEqual(['Trips'])
  expect(html).toContain('Spent away in 2026')
  expect(html).toMatch(/<span>Days away<\/span><strong>5<\/strong>/)
  expect(html).toMatch(/<span>Flown<\/span><strong>1<\/strong>/)
  // The year is a calendar: each day away opens its trip, a band from its first day to its last.
  expect(html).toContain('aria-label="Days away in 2026"')
  expect(html).toContain('aria-label="Aug 10: Portugal &amp; Spain"')
  expect(html).toContain('data-command="trips.open.a"')
  expect(html).toContain('trips-day is-away is-run-start')
  expect(html).toContain('class="trips-day is-future"')
  // A row per trip: named and flagged by its countries, its days as bars, its cost.
  expect(html).toContain('aria-label="Trips in 2026"')
  expect(html).toContain('href="/insights?trip=a"')
  expect(html).toContain('🇵🇹🇪🇸')
  expect(html).toContain('Aug 10 – Aug 12 · 3 days · Lisbon, Madrid · Flown')
  expect(html).toContain('aria-label="Portugal &amp; Spain day by day"')
  expect(html).toContain('$40.00 a day')
  // Places count each city once per trip.
  expect(html).toContain('aria-label="Places in 2026"')
  expect(html).toContain('2 trips · 6 purchases')
  expect(html).not.toContain('Lounge')
  // Home is named from the bases Rust found for the year.
  expect(html).toContain('Home in 2026: Oakland, CA.')
  // Money's strip no longer carries trips.
  expect(await renderRoute('/spending')).not.toContain('>Trips<')
})

it('opens one trip: its money day by day, purchases, route, and split, stepping between trips', async () => {
  scenario.insights = {
    trips: {
      trips: [
        {
          ...trip('a', '2026-08-10', '2026-08-12', true),
          gettingThere: -300,
          bookings: [{ transactionId: 'flight', date: '2026-07-20', confidence: 0.78 }],
        },
        trip('b', '2026-06-01', '2026-06-02', false),
      ],
      years: [{ year: 2026, count: 2, days: 5, spent: 240, abroad: 2, flown: 1, longest: 'a' }],
      bases: [home],
    },
  }
  const html = await renderRoute('/insights?trip=a')
  // A flight booked ahead is named with how likely it was for this trip, and the money splits
  // into getting there, staying, and on the ground.
  expect(html).toContain('78% likely for this trip')
  expect(html).toMatch(/<span>Getting there<\/span><strong><span class="sensitive-value">\$300\.00/)
  expect(html).toContain('aria-current="page">Portugal &amp; Spain')
  expect(html).toContain('href="/insights">Trips')
  expect(html).toContain('3 days · $40.00 a day · 1 purchase · Flown · Abroad')
  // Rust summed each day; the biggest one is named with its purchases.
  expect(html).toContain(
    'aria-label="Portugal &amp; Spain day by day: most on Mon, Aug 10, $80.00"',
  )
  expect(html).toContain('Most on Monday, Aug 10: $80.00 over 2 purchases.')
  expect(html).toContain('>Purchases</h2>')
  // The route out from home in the order it was reached, and where the money went.
  expect(html).toContain('aria-label="Stops"')
  expect(html).toContain('Lisbon, Portugal')
  expect(html).toContain('aria-label="Portugal &amp; Spain by category"')
  expect(html).not.toContain('airport')
  // Earlier and later trips are a step away; this is the latest.
  expect(html).toContain('data-command="trips.earlier"')
  expect(html).toContain('aria-label="Earlier: Portugal &amp; Spain"')
  expect(html).toMatch(/data-command="trips.later"[^>]*disabled=""/)
})

it('shows the trip under way ahead of the year', async () => {
  scenario.insights = {
    trips: {
      trips: [{ ...trip('now', '2026-09-28', '2026-09-30', false), ongoing: true }],
      years: [{ year: 2026, count: 1, days: 3, spent: 120, abroad: 1, flown: 0, longest: 'now' }],
      bases: [home],
    },
  }
  const html = await renderRoute('/insights')
  expect(html).toContain('Away now')
  expect(html).toContain('3 days so far · since Sep 28 · Lisbon, Madrid')
  expect(html).toContain('<span>Spent so far</span>')
  expect(html).toContain('href="/insights?trip=now"')
})

const people = [
  { id: 'person:a', name: 'Example Person A', matchNames: ['EXAMPLE PERSON A'] },
  { id: 'person:b', name: 'Example Person B' },
]

const tagged = transaction({
  id: 'zelle-a',
  merchant: 'Zelle payment to EXAMPLE PERSON A',
  category: 'Payments To People',
  date: '2026-09-02',
  amount: -40.1,
  account: 'Selected card',
  accountId: 'selected',
  people: ['person:a'],
  peopleSource: 'matched' as const,
  classification: classification('expense', { zelle: true }),
  pending: false,
})

it('lists people with what was sent and received, and names payments carry', async () => {
  scenario.people = people
  scenario.insights = {
    people: {
      people: [
        {
          id: 'person:a',
          transactionCount: 2,
          sent: -40.1,
          received: 12.05,
          first: '2026-08-01',
          last: '2026-09-02',
          tripIds: ['a'],
        },
        { id: 'person:b', transactionCount: 0, sent: 0, received: 0, tripIds: [] },
      ],
      sent: 40.1,
      suggestions: [
        {
          name: 'Example',
          matchName: 'EXAMPLE',
          transactionIds: ['zelle-a', 'y'],
          matches: [{ id: 'person:a', fit: 'possible' }],
        },
        {
          name: 'Example Person C',
          matchName: 'EXAMPLE PERSON C',
          transactionIds: ['x'],
          matches: [],
        },
      ],
    },
  }
  scenario.extraTransactions = [tagged]
  const html = await renderRoute('/insights?view=people')
  expect(html).toContain('aria-current="page">People')
  // Only People is the current tab: Trips, whose search People's holds, is not.
  expect(currentTabs(html)).toEqual(['People'])
  // A page of Insights reads as Insights', and Calendar, a page of its own, has no strip.
  const taxes = await renderRoute('/taxes')
  expect(taxes).toMatch(/<a[^>]*href="\/insights"[^>]*>Insights<\/a>.*?aria-current="page">Taxes/)
  expect(currentTabs(taxes)).toEqual(['Taxes'])
  const calendar = await renderRoute('/calendar')
  expect(calendar).not.toContain('class="account-switcher"')
  expect(html).toContain('aria-label="Find or add a person"')
  expect(html).toContain('href="/insights?view=people&amp;person=person%3Aa"')
  // A name in payments comes with who it could be, and Y takes the best guess.
  expect(html).toContain('>Who’s this?</h2>')
  expect(html).toContain('aria-label="Who Example is"')
  expect(html).toMatch(
    /2 Zelle payments(<!-- -->)? · last sent (<!-- -->)?<span class="sensitive-value">\$40\.10/,
  )
  expect(html).toContain('In People · Maybe')
  expect(html).toContain('Example is Example Person A')
  // A name that fits nobody can still become someone new, or someone else.
  expect(html).toContain('aria-label="Who Example Person C is"')
  expect(html).toContain('New person')
  expect(html).toContain('aria-label="Someone else is Example Person C"')
  // The page leads with what went each way, then everyone with history around you, each their
  // avatar, named underneath, and a link to them.
  expect(html).toContain('<h2 class="balance-label">Sent to people</h2>')
  expect(html).toMatch(/\$12\.05(<!-- -->)? came back from (<!-- -->)?1 person\./)
  const orbit = html.slice(html.indexOf('<div class="people-orbit"'))
  expect(orbit).toContain('aria-label="People by how recently and how much"')
  expect(orbit).toContain('href="/insights?view=people&amp;person=person%3Aa"')
  expect(orbit).toMatch(/class="people-orbit-label"[^>]*>Example</)
  // Beside it, a table of what was received and sent, searched with / and sorted by its
  // columns; Return on a row opens its person.
  expect(html).toContain('aria-label="Search people"')
  const table = html.slice(html.indexOf('<div class="people-table"'))
  expect(table).toContain('aria-label="Sort by Received"')
  expect(table).toContain('data-command="people.sort.sent"')
  const rows = table.slice(table.indexOf('<ol class="people-table-body"'))
  expect(rows).toContain('data-keyboard-open')
  expect(rows).toMatch(/Example Person A<\/strong><\/span><span class="sensitive-value">\$12\.05/)
  expect(rows).toMatch(/\$12\.05<\/span><span class="sensitive-value">\$40\.10/)
  // Someone with nothing tagged has nothing to show, so only Find or add reaches them.
  expect(table).not.toContain('Example Person B')
  expect(orbit).not.toContain('Example Person B')
})

it('explains People before anyone is added', async () => {
  const html = await renderRoute('/insights?view=people')
  expect(html).toContain('Add someone you pay or travel with.')
})

it('opens a person: their figures, tagged activity, trips, and details behind a click', async () => {
  scenario.people = people
  scenario.extraTransactions = [tagged]
  scenario.insights = {
    trips: {
      trips: [{ ...trip('a', '2026-08-10', '2026-08-12', true), companions: ['person:a'] }],
      years: [],
      bases: [home],
    },
    people: {
      people: [
        {
          id: 'person:a',
          transactionCount: 1,
          sent: -40.1,
          received: 0,
          first: '2026-09-02',
          last: '2026-09-02',
          tripIds: ['a'],
        },
      ],
      sent: 40.1,
      suggestions: [],
    },
  }
  const html = await renderRoute('/insights?view=people&person=person:a')
  expect(html).toContain('>Example Person A</h2>')
  expect(html).toContain('Together since Sep 2 · last 2 weeks ago')
  expect(html).toMatch(/<span>Transactions<\/span><strong[^>]*>1</)
  expect(html).toMatch(/<span>Sent<\/span><strong[^>]*><span class="sensitive-value">\$40\.10/)
  // A year of moments: September has the one tagged payment; August, the trip.
  expect(html).toContain('aria-label="Example Person A, the last twelve months. Sep: 1"')
  expect(html).toContain('Zelle payment to EXAMPLE PERSON A')
  expect(html).toContain('aria-label="Trips with Example Person A"')
  expect(html).toContain('data-command="people.details"')
  expect(html).toContain('aria-expanded="false"')
  // The breadcrumb leads back to everyone.
  expect(html).toContain('href="/insights?view=people"')
})

it("leaves a person's sent and received unknown, not $0, before Rust summarizes them", async () => {
  scenario.people = people
  const html = await renderRoute('/insights?view=people&person=person:a')
  expect(html).toContain('>Example Person A</h2>')
  expect(html).toMatch(/<span>Sent<\/span><strong[^>]*><span class="sensitive-value">—</)
  expect(html).toMatch(/<span>Received<\/span><strong[^>]*><span class="sensitive-value">—</)
  expect(html).not.toMatch(/<span>(Sent|Received)<\/span><strong[^>]*><span[^>]*>\$0\.00/)
})

it('shows who went on a trip, with P to change them', async () => {
  scenario.people = people
  scenario.insights = {
    trips: {
      trips: [{ ...trip('a', '2026-08-10', '2026-08-12', true), companions: ['person:a'] }],
      years: [],
      bases: [home],
    },
  }
  const html = await renderRoute('/insights?trip=a')
  expect(html).toContain('aria-label="Who went"')
  expect(html).toContain('Example Person A')
  expect(html).toContain('data-command="trips.people"')
  expect(html).toContain('aria-keyshortcuts="P"')
})
