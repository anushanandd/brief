import { describe, expect, it } from 'vitest'

import {
  activityQuerySuggestions,
  amountMatches,
  completeActivityQuery,
  hasStructuredQuery,
  parseActivityQuery,
  settledActivityQuery,
} from './activity-query'

const vocabulary = {
  accounts: [
    { id: 'amex', label: 'Platinum Card' },
    { id: 'chase', label: 'Chase Checking' },
    { id: 'savings', label: 'Chase Savings' },
  ],
  categories: ['Dining', 'Home Improvement', 'Groceries', 'Travel'],
  methods: ['Online', 'In store'],
}
const today = '2026-09-16'

describe('parseActivityQuery', () => {
  it('reads filters the way they are said and keeps the rest as text', () => {
    expect(
      parseActivityQuery('dining over $50 last month on platinum blue bottle', vocabulary, today),
    ).toEqual({
      accounts: ['amex'],
      categories: ['Dining'],
      methods: [],
      from: '2026-08-01',
      to: '2026-08-31',
      min: 50,
      text: 'blue bottle',
    })
  })

  it('handles ranges, multi-word names, relative days, and named months', () => {
    expect(
      parseActivityQuery('home improvement between 20 and $1,200 past 30 days', vocabulary, today),
    ).toMatchObject({
      categories: ['Home Improvement'],
      min: 20,
      max: 1_200,
      from: '2026-08-18',
      to: today,
      text: '',
    })
    expect(parseActivityQuery('travel in november', vocabulary, today)).toMatchObject({
      categories: ['Travel'],
      from: '2025-11-01',
      to: '2025-11-30',
    })
    expect(parseActivityQuery('under 15 online yesterday', vocabulary, today)).toMatchObject({
      methods: ['Online'],
      max: 15,
      from: '2026-09-15',
      to: '2026-09-15',
    })
  })

  it('only treats a single word as a name when it points at one entity', () => {
    // "chase" names two accounts, so it stays search text; "savings" is unique.
    expect(parseActivityQuery('chase', vocabulary, today)).toMatchObject({
      accounts: [],
      text: 'chase',
    })
    expect(parseActivityQuery('savings', vocabulary, today).accounts).toEqual(['savings'])
    expect(hasStructuredQuery(parseActivityQuery('coffee', vocabulary, today))).toBe(false)
  })
})

describe('activityQuerySuggestions', () => {
  it('completes the word being typed across facets and offers amount bounds for numbers', () => {
    expect(activityQuerySuggestions('lunch at din', vocabulary).map(({ id }) => id)).toEqual([
      'category:Dining',
    ])
    expect(activityQuerySuggestions('40', vocabulary).map(({ label }) => label)).toEqual([
      'Over $40',
      'Under $40',
    ])
    expect(activityQuerySuggestions('last m', vocabulary).map(({ label }) => label)).toContain(
      'Last month',
    )
    expect(activityQuerySuggestions('', vocabulary)).toEqual([])
  })

  it('replaces only the fragment it completes', () => {
    const [dining] = activityQuerySuggestions('coffee din', vocabulary)
    expect(completeActivityQuery('coffee din', dining)).toBe('coffee Dining ')
    const lastMonth = activityQuerySuggestions('coffee last m', vocabulary).find(
      ({ label }) => label === 'Last month',
    )!
    expect(completeActivityQuery('coffee last m', lastMonth)).toBe('coffee Last month ')
  })
})

it('compares amounts by size so purchases and refunds both match', () => {
  expect(amountMatches(-60, 50)).toBe(true)
  expect(amountMatches(60, undefined, 50)).toBe(false)
  expect(amountMatches(-20, 10, 30)).toBe(true)
})

it('leaves an unfinished word out of the live preview while a suggestion completes it', () => {
  const suggestions = activityQuerySuggestions('over 50 last mon', vocabulary)
  expect(settledActivityQuery('over 50 last mon', suggestions)).toBe('over 50 ')
  expect(settledActivityQuery('coffee ', suggestions)).toBe('coffee ')
  expect(settledActivityQuery('coffee', [])).toBe('coffee')
})

it('reads explicit ranges, open-ended dates, and abbreviated months', () => {
  expect(parseActivityQuery('aug 1 to aug 15 dining', vocabulary, today)).toMatchObject({
    categories: ['Dining'],
    from: '2026-08-01',
    to: '2026-08-15',
    text: '',
  })
  expect(parseActivityQuery('since june', vocabulary, today)).toMatchObject({
    from: '2026-06-01',
    to: today,
  })
  expect(parseActivityQuery('before sept 1', vocabulary, today)).toMatchObject({
    to: '2026-08-31',
  })
  expect(parseActivityQuery('before sept 1', vocabulary, today).from).toBeUndefined()
  expect(parseActivityQuery('nov 3 travel', vocabulary, today)).toMatchObject({
    categories: ['Travel'],
    from: '2025-11-03',
    to: '2025-11-03',
  })
  expect(parseActivityQuery('dec 2025 through feb', vocabulary, today)).toMatchObject({
    from: '2025-12-01',
    to: '2026-02-28',
  })
})

it('reads typed dates in search as a range', () => {
  const dates = (input: string) => {
    const { from, to, text } = parseActivityQuery(input, vocabulary, today)
    return { from, to, text }
  }
  expect(dates('from 8/1 to 9/2')).toEqual({ from: '2026-08-01', to: '2026-09-02', text: '' })
  expect(dates('aug 1 - 15 dining')).toMatchObject({ from: '2026-08-01', to: '2026-08-15' })
  expect(dates('aug 1 - sep 2')).toMatchObject({ from: '2026-08-01', to: '2026-09-02' })
  expect(dates('since 6/1')).toMatchObject({ from: '2026-06-01', to: today })
  expect(dates('12/25')).toMatchObject({ from: '2025-12-25', to: '2025-12-25' })
  expect(dates('2026-02-03 to 3/4/26')).toMatchObject({ from: '2026-02-03', to: '2026-03-04' })
  expect(dates('2/31')).toEqual({ from: undefined, to: undefined, text: '2/31' })
})

it('reads whose money a search asks for', () => {
  const other = parseActivityQuery('dining for other in june', vocabulary, today)
  expect(other.owner).toBe('other')
  expect(other.categories).toEqual(['Dining'])
  expect(other.text).toBe('')
  expect(hasStructuredQuery(other)).toBe(true)
  expect(parseActivityQuery('for me', vocabulary, today).owner).toBe('me')
  expect(parseActivityQuery('for others', vocabulary, today).owner).toBe('other')
  // "for" on its own is still a filler word, not a filter.
  expect(parseActivityQuery('coffee for', vocabulary, today).owner).toBeUndefined()
  expect(activityQuerySuggestions('for', vocabulary).map(({ label }) => label)).toEqual([
    'For other',
    'For me',
  ])
})
