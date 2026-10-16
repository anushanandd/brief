import { expect, it } from 'vitest'

import {
  identities,
  initials,
  monthlyMoments,
  personColor,
  timeAgo,
  withIdentity,
  withTripCompanions,
  withoutPerson,
} from './people'
import { financeSettingsSchema } from './schema'

const today = '2026-09-16'

const card = (identifier: string, name: string, fit: 'likely' | 'possible') => ({
  identifier,
  name,
  hasPhoto: false,
  fit,
})

it('gives each person one data color of their own', () => {
  expect(personColor('person:a')).toMatch(/^var\(--data-[1-7]\)$/)
  expect(personColor('person:a')).toBe(personColor('person:a'))
})

it('names how long ago in words', () => {
  expect(timeAgo('2026-09-16', today)).toBe('today')
  expect(timeAgo('2026-09-15', today)).toBe('yesterday')
  expect(timeAgo('2026-09-10', today)).toBe('6 days ago')
  expect(timeAgo('2026-09-02', today)).toBe('2 weeks ago')
  expect(timeAgo('2026-06-16', today)).toBe('3 months ago')
  expect(timeAgo('2025-06-01', today)).toBe('a year ago')
  expect(timeAgo('2023-09-01', today)).toBe('3 years ago')
})

it('offers who a name in payments could be, saved people and Contacts cards, likeliest first', () => {
  const people = [
    { id: 'person:a', name: 'Example Person', matchNames: ['EXAMPLE PERSON'] },
    { id: 'person:b', name: 'Other Person', contactId: 'card-b' },
  ]
  const suggestion = {
    name: 'Example',
    matchName: 'EXAMPLE',
    transactionIds: ['t'],
    matches: [{ id: 'person:a', fit: 'possible' as const }],
  }
  const found = identities(suggestion, people, [
    card('card-c', 'Example Third', 'possible'),
    // Already someone saved, so it's that person, not a second option.
    card('card-b', 'Other Person', 'likely'),
  ])
  expect(
    found.map((item) => (item.kind === 'person' ? item.person.id : item.contact.identifier)),
  ).toEqual(['person:b', 'person:a', 'card-c'])
})

it('makes someone match a name in payments, adding them from Contacts when needed', () => {
  const settings = financeSettingsSchema.parse({
    people: [{ id: 'person:a', name: 'Example Person', matchNames: ['EXAMPLE PERSON'] }],
  })
  const known = withIdentity(settings, 'EXAMPLE', {
    kind: 'person',
    person: settings.people[0],
    fit: 'possible',
  })
  expect(known.people[0].matchNames).toEqual(['EXAMPLE PERSON', 'EXAMPLE'])
  const added = withIdentity(settings, 'OTHER', {
    kind: 'contact',
    contact: { identifier: 'card', name: 'Other Person', hasPhoto: true, fit: 'possible' },
    fit: 'possible',
  })
  expect(added.people[1]).toMatchObject({
    name: 'Other Person',
    contactId: 'card',
    matchNames: ['OTHER'],
  })
  const typed = withIdentity(settings, 'NEW NAME', { kind: 'new', name: 'New Name' })
  expect(typed.people[1]).toMatchObject({ name: 'New Name', matchNames: ['NEW NAME'] })
})

it('counts tagged transactions in each of the last twelve months', () => {
  const months = monthlyMoments(['2026-09-02', '2026-09-10', '2025-10-31', '2025-09-30'], today)
  expect(months).toHaveLength(12)
  expect(months[0]).toEqual({ key: '2025-10', label: 'Oct', count: 1 })
  expect(months.at(-1)).toEqual({ key: '2026-09', label: 'Sep', count: 2 })
  expect(months.reduce((sum, { count }) => sum + count, 0)).toBe(3)
})

it('takes initials from the first and last names', () => {
  expect(initials('example person a')).toBe('EA')
  expect(initials('Kaya')).toBe('K')
})

it('replaces a trip’s companions wherever it shares a transaction, and removes people whole', () => {
  const settings = financeSettingsSchema.parse({
    people: [
      { id: 'person:a', name: 'A' },
      { id: 'person:b', name: 'B' },
    ],
    tripCompanions: [
      { transactionIds: ['t1'], people: ['person:a'] },
      { transactionIds: ['t9'], people: ['person:a', 'person:b'] },
    ],
  })
  const next = withTripCompanions(settings, { transactionIds: ['t1', 't2'] }, ['person:b'])
  expect(next.tripCompanions).toEqual([
    { transactionIds: ['t9'], people: ['person:a', 'person:b'] },
    { transactionIds: ['t1', 't2'], people: ['person:b'] },
  ])
  const without = withoutPerson(next, 'person:b')
  expect(without.people.map(({ id }) => id)).toEqual(['person:a'])
  expect(without.tripCompanions).toEqual([{ transactionIds: ['t9'], people: ['person:a'] }])
})
