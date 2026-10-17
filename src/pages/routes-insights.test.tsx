import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { evidenceFixture } from '../data/fixtures/evidence'
import { transaction } from '../data/fixtures/records'
import { renderRoute } from './routes.harness'
import { scenario } from './routes.scenario'

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
  categories: [{ name: 'Dining', amount: -120, share: 100 }],
  rest: 0,
  restShare: 0,
  transactionIds: ['transaction-0'],
  ongoing: false,
  flew,
  stops: [
    { city: 'Zürich', region: 'ZH', country: 'CH', purchases: 3, first: start },
    { city: 'Berlin', region: '16', country: 'DE', purchases: 2, first: end },
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
      home: ['CA, US'],
    },
  }
  const html = await renderRoute('/insights')
  expect(html).toContain('aria-current="page">Trips')
  expect(html).toContain('aria-label="Insights"')
  expect(html).toContain('Spent away in 2026')
  expect(html).toMatch(/<span>Days away<\/span><strong>5<\/strong>/)
  expect(html).toMatch(/<span>Flown<\/span><strong>1<\/strong>/)
  // The year is a calendar: each day away opens its trip, a band from its first day to its last.
  expect(html).toContain('aria-label="Days away in 2026"')
  expect(html).toContain('aria-label="Aug 10: Switzerland &amp; Germany"')
  expect(html).toContain('data-command="trips.open.a"')
  expect(html).toContain('trips-day is-away is-run-start')
  expect(html).toContain('class="trips-day is-future"')
  // A row per trip: named and flagged by its countries, its days as bars, its cost.
  expect(html).toContain('aria-label="Trips in 2026"')
  expect(html).toContain('href="/insights?trip=a"')
  expect(html).toContain('🇨🇭🇩🇪')
  expect(html).toContain('Aug 10 – Aug 12 · 3 days · Zürich, Berlin · Flown')
  expect(html).toContain('aria-label="Switzerland &amp; Germany day by day"')
  expect(html).toContain('$40.00 a day')
  // Places count each city once per trip.
  expect(html).toContain('aria-label="Places in 2026"')
  expect(html).toContain('2 trips · 6 purchases')
  expect(html).not.toContain('Lounge')
  // Money's strip no longer carries trips.
  expect(await renderRoute('/spending')).not.toContain('>Trips<')
})

it('opens one trip: its money day by day, purchases, route, and split, stepping between trips', async () => {
  scenario.insights = {
    trips: {
      trips: [
        trip('a', '2026-08-10', '2026-08-12', true),
        trip('b', '2026-06-01', '2026-06-02', false),
      ],
      years: [{ year: 2026, count: 2, days: 5, spent: 240, abroad: 2, flown: 1, longest: 'a' }],
      home: ['CA, US'],
    },
  }
  const html = await renderRoute('/insights?trip=a')
  expect(html).toContain('aria-current="page">Switzerland &amp; Germany')
  expect(html).toContain('href="/insights">Trips')
  expect(html).toContain('3 days · $40.00 a day · 1 purchase · Flown · Abroad')
  // Rust summed each day; the biggest one is named with its purchases.
  expect(html).toContain(
    'aria-label="Switzerland &amp; Germany day by day: most on Mon, Aug 10, $80.00"',
  )
  expect(html).toContain('Most on Monday, Aug 10: $80.00 over 2 purchases.')
  expect(html).toContain('>Purchases</h2>')
  // The route out from home in the order it was reached, and where the money went.
  expect(html).toContain('aria-label="Stops"')
  expect(html).toContain('Zürich, Switzerland')
  expect(html).toContain('aria-label="Switzerland &amp; Germany by category"')
  expect(html).not.toContain('airport')
  // Earlier and later trips are a step away; this is the latest.
  expect(html).toContain('data-command="trips.earlier"')
  expect(html).toContain('aria-label="Earlier: Switzerland &amp; Germany"')
  expect(html).toMatch(/data-command="trips.later"[^>]*disabled=""/)
})

it('shows the trip under way ahead of the year', async () => {
  scenario.insights = {
    trips: {
      trips: [{ ...trip('now', '2026-09-28', '2026-09-30', false), ongoing: true }],
      years: [{ year: 2026, count: 1, days: 3, spent: 120, abroad: 1, flown: 0, longest: 'now' }],
      home: ['CA, US'],
    },
  }
  const html = await renderRoute('/insights')
  expect(html).toContain('Away now')
  expect(html).toContain('3 days so far · since Sep 28 · Zürich, Berlin')
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
  // Everyone as one table: when you were last together, what's tagged, and what went each
  // way on a shared centre line. Return on a row opens its person.
  const roster = html.slice(html.indexOf('<table class="people-roster"'))
  expect(roster).toContain('aria-label="People"')
  expect(roster).toMatch(/<th scope="col">Last together<\/th>/)
  expect(roster).toContain('data-keyboard-open')
  expect(roster).toContain('2 weeks ago')
  expect(roster).toContain('<td class="is-number">2</td><td class="is-number">1</td>')
  expect(roster).toMatch(/<td class="is-sent">.*?\$40\.10/)
  expect(roster).toMatch(/<td class="is-received">.*?\$12\.05/)
  // Someone with nothing tagged is still listed, without figures.
  expect(roster).toContain('Example Person B')
  expect(roster).toContain('Nothing tagged yet')
  expect(roster).toContain('<td class="is-number">0</td><td class="is-number"></td>')
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
      home: ['CA, US'],
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

it('shows who went on a trip, with P to change them', async () => {
  scenario.people = people
  scenario.insights = {
    trips: {
      trips: [{ ...trip('a', '2026-08-10', '2026-08-12', true), companions: ['person:a'] }],
      years: [],
      home: ['CA, US'],
    },
  }
  const html = await renderRoute('/insights?trip=a')
  expect(html).toContain('aria-label="Who went"')
  expect(html).toContain('Example Person A')
  expect(html).toContain('data-command="trips.people"')
  expect(html).toContain('aria-keyshortcuts="P"')
})

it('tells chapters as a timeline and a list, each opening what defines it', async () => {
  scenario.insights = { evidence: evidenceFixture }
  const html = await renderRoute('/insights?view=chapters')
  expect(html).toContain('aria-current="page">Chapters')
  expect(html).toContain('Example Employer · California · month 6')
  expect(html).toContain('aria-label="Chapters over time"')
  expect(html).toContain('data-command="chapters.open.work:example"')
  expect(html).toContain('data-command="chapters.plan"')
  // A detected chapter is a possibility until confirmed, and says so in words.
  expect(html).toContain('>Possible<')

  const detail = await renderRoute('/insights?view=chapters&chapter=work:example')
  expect(detail).toContain('Pay statements from Example Employer')
  expect(detail).toContain('Example fee waiver')
  expect(detail).toContain('Check the final paycheck.')
  expect(detail).toContain('data-command="chapter.confirm"')
  expect(detail).toContain('data-command="chapter.dismiss"')
})

it('shows what rules explain, with tiers and the Why chain', async () => {
  scenario.insights = { evidence: evidenceFixture }
  const html = await renderRoute('/insights?view=rules')
  expect(html).toContain('The $100 bonus offsets the card’s $100 fee')
  expect(html).toContain('>Explained<')
  expect(html).toContain('>Possible<')
  expect(html).toContain('Paperless statements')
  expect(html).toContain('href="https://bank.example/terms"')
  expect(html).toContain('Deposits were short in September 2026.')
  expect(html).toContain('Example fee waiver')
})
