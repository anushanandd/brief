import { expect, it } from 'vitest'

import {
  agendaLabel,
  attendingPeople,
  calendarSearch,
  entriesOn,
  monthGrid,
  shiftMonth,
} from './calendar'
import type { AgendaItem } from './schema'

const item = (overrides: Partial<AgendaItem>): AgendaItem => ({
  id: 'item',
  kind: 'bill',
  date: '2026-03-10',
  estimated: false,
  ...overrides,
})

it('shows six weeks from the Sunday on or before the first', () => {
  const days = monthGrid('2026-03')
  expect(days).toHaveLength(42)
  expect(days[0]).toBe('2026-03-01')
  expect(monthGrid('2026-10')[0]).toBe('2026-09-27')
  expect(shiftMonth('2026-01', -1)).toBe('2025-12')
  expect(shiftMonth('2026-12', 1)).toBe('2027-01')
})

it('keeps only real months and days from the search', () => {
  expect(calendarSearch({ month: '2026-13', day: '2026-02-30' })).toEqual({})
  expect(calendarSearch({ month: '2026-02', day: '2026-02-28', item: 'x' })).toEqual({
    month: '2026-02',
    day: '2026-02-28',
  })
})

it('puts a day’s Brief items before its events, and spans trips across their days', () => {
  const trip = item({ id: 'trip', kind: 'trip', date: '2026-03-09', end: '2026-03-11' })
  const event = {
    id: 'e',
    calendar: 'Home',
    title: 'Dinner',
    start: '2026-03-10T19:00:00-07:00',
    end: '2026-03-10T21:00:00-07:00',
    allDay: false,
    attendees: [],
  }
  const entries = entriesOn('2026-03-10', [trip, item({})], [], [event])
  expect(entries.map(({ type, key }) => `${type}:${key}`)).toEqual([
    'agenda:trip',
    'agenda:item',
    'event:event:e:2026-03-10',
  ])
  expect(entriesOn('2026-03-12', [trip], [], [event])).toEqual([])
})

it('offers the people invited to a trip’s events who aren’t its companions yet', () => {
  const event = {
    id: 'e',
    calendar: 'Home',
    title: 'Flight',
    start: '2026-03-10',
    end: '2026-03-10',
    allDay: true,
    attendees: [
      { name: 'Example Person A', contactId: 'card-a' },
      { name: 'example person b' },
      { name: 'Stranger' },
    ],
  }
  const people = [
    { id: 'person:a', name: 'Renamed A', contactId: 'card-a' },
    { id: 'person:b', name: 'Example Person B' },
    { id: 'person:c', name: 'Example Person C' },
  ]
  expect(attendingPeople([event], people, []).map(({ id }) => id)).toEqual(['person:a', 'person:b'])
  expect(attendingPeople([event], people, ['person:a']).map(({ id }) => id)).toEqual(['person:b'])
})

const names = (id: string) => (id === 'card' ? 'Travel card' : 'Card')

it('names accounts as the user did and reads descriptors as words', () => {
  expect(agendaLabel(item({ kind: 'paymentDue', accountId: 'card' }), names)).toBe(
    'Travel card payment due',
  )
  expect(agendaLabel(item({ title: 'ACH RENT PAYMENT' }), names)).toBe('ACH Rent Payment due')
  expect(agendaLabel(item({ kind: 'washSaleEnds', ticker: 'VTI' }), names)).toBe(
    'VTI wash-sale window ends',
  )
})
