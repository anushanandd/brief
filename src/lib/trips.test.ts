import { describe, expect, it } from 'vitest'

import type { Trip } from './schema'
import {
  monthLengths,
  tripColors,
  tripDayCount,
  tripDaysInYear,
  tripNeighbours,
  tripPeakDay,
  tripPlaces,
  tripYearTotals,
} from './trips'

const trip = (id: string, start: string, end: string, extra: Partial<Trip> = {}): Trip => ({
  id,
  start,
  end,
  days: [],
  perDay: 0,
  destination: id,
  abroad: false,
  total: -100,
  categories: [],
  rest: 0,
  restShare: 0,
  transactionIds: [],
  ongoing: false,
  flew: false,
  stops: [],
  ...extra,
})

describe('trip days', () => {
  it('counts both ends of a trip', () => {
    expect(tripDayCount(trip('a', '2026-03-01', '2026-03-01'))).toBe(1)
    expect(tripDayCount(trip('a', '2026-03-01', '2026-03-04'))).toBe(4)
  })

  it('maps each day of the year to its trip, clipped to the year', () => {
    const days = tripDaysInYear(
      [trip('a', '2025-12-30', '2026-01-02'), trip('b', '2026-06-10', '2026-06-11')],
      2026,
    )
    expect([...days.keys()]).toEqual(['2026-01-01', '2026-01-02', '2026-06-10', '2026-06-11'])
    expect(days.get('2026-01-01')?.id).toBe('a')
  })

  it('knows a leap year', () => {
    expect(monthLengths(2024)[1]).toBe(29)
    expect(monthLengths(2026)[1]).toBe(28)
  })
})

describe('trip year', () => {
  it("reads Rust's year totals and its longest trip, nothing for a year without trips", () => {
    const longest = trip('a', '2026-03-01', '2026-03-04')
    const trips = {
      trips: [trip('b', '2026-05-01', '2026-05-02'), longest],
      years: [{ year: 2026, count: 2, days: 6, spent: 30.3, abroad: 1, flown: 1, longest: 'a' }],
    }
    expect(tripYearTotals(trips, 2026)).toEqual({
      count: 2,
      days: 6,
      spent: 30.3,
      abroad: 1,
      flown: 1,
      longest,
    })
    expect(tripYearTotals(trips, 2025)).toMatchObject({ count: 0, spent: 0, longest: undefined })
    expect(tripYearTotals(undefined, 2026).count).toBe(0)
  })

  it('colors trips in date order so neighbours differ', () => {
    const colors = tripColors([
      trip('later', '2026-05-01', '2026-05-02'),
      trip('earlier', '2026-03-01', '2026-03-02'),
    ])
    expect(colors.get('earlier')).toBe('var(--data-1)')
    expect(colors.get('later')).toBe('var(--data-2)')
  })
})

describe('trip days and neighbours', () => {
  it('names the day with the most spending, or none when nothing was bought', () => {
    const days = [
      { date: '2026-03-01', spent: -10, purchases: 1 },
      { date: '2026-03-02', spent: -42.5, purchases: 3 },
      { date: '2026-03-03', spent: 0, purchases: 0 },
    ]
    expect(tripPeakDay({ days })?.date).toBe('2026-03-02')
    expect(tripPeakDay({ days: [{ date: '2026-03-03', spent: 0, purchases: 0 }] })).toBeUndefined()
  })

  it('steps to the trips either side in time, whatever order the list is in', () => {
    const trips = [
      trip('c', '2026-07-01', '2026-07-02'),
      trip('a', '2026-03-01', '2026-03-02'),
      trip('b', '2026-05-01', '2026-05-02'),
    ]
    expect(tripNeighbours(trips, 'b')).toMatchObject({ earlier: { id: 'a' }, later: { id: 'c' } })
    expect(tripNeighbours(trips, 'a').earlier).toBeUndefined()
    expect(tripNeighbours(trips, 'missing')).toEqual({ earlier: undefined, later: undefined })
  })
})

describe('trip places', () => {
  it('counts a city once per trip and ranks by purchases', () => {
    const zurich = {
      city: 'Zürich',
      region: 'ZH',
      country: 'CH',
      purchases: 3,
      first: '2026-03-01',
    }
    const austin = {
      city: 'Austin',
      region: 'TX',
      country: 'US',
      purchases: 5,
      first: '2026-05-01',
    }
    const places = tripPlaces([
      trip('a', '2026-03-01', '2026-03-02', { abroad: true, stops: [zurich] }),
      trip('b', '2026-04-01', '2026-04-02', {
        abroad: true,
        stops: [zurich, { ...zurich, purchases: 1 }],
      }),
      trip('c', '2026-05-01', '2026-05-02', { stops: [austin] }),
    ])
    expect(places.map(({ label, trips, purchases }) => [label, trips, purchases])).toEqual([
      ['Zürich, Switzerland', 2, 7],
      ['Austin, TX', 1, 5],
    ])
    expect(places[0].abroad).toBe(true)
  })
})
