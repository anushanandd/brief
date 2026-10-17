import { countryFlag, countryName, placeLabel } from './places'
import type { Trip, Trips } from './schema'
import { formatListDate } from './spending'

const dayMs = 86_400_000

const parseDay = (date: string) => Date.parse(`${date}T00:00:00Z`)
const isoDay = (time: number) => new Date(time).toISOString().slice(0, 10)

/** Years with trips, newest first; this year always. */
export function tripYears(trips: Trip[], today: string) {
  return [
    ...new Set([Number(today.slice(0, 4)), ...trips.map(({ start }) => Number(start.slice(0, 4)))]),
  ].toSorted((left, right) => right - left)
}

/** How many calendar days a trip covered, counting both ends. */
export const tripDayCount = (trip: Pick<Trip, 'start' | 'end'>) =>
  Math.round((parseDay(trip.end) - parseDay(trip.start)) / dayMs) + 1

/** A year's trips at a glance, as Rust summed them; nothing for a year without trips. */
export function tripYearTotals(trips: Pick<Trips, 'trips' | 'years'> | undefined, year: number) {
  const totals = trips?.years.find((entry) => entry.year === year)
  return {
    count: totals?.count ?? 0,
    days: totals?.days ?? 0,
    spent: totals?.spent ?? 0,
    abroad: totals?.abroad ?? 0,
    flown: totals?.flown ?? 0,
    longest: trips?.trips.find(({ id }) => id === totals?.longest),
  }
}

const dataColors = Array.from({ length: 7 }, (_, index) => `var(--data-${index + 1})`)

/**
 * One data color per trip, assigned in date order so a trip keeps its color as the year fills
 * in and neighbours on the calendar never share one.
 */
export function tripColors(trips: Trip[]) {
  const ordered = trips.toSorted((left, right) => left.start.localeCompare(right.start))
  return new Map(ordered.map((trip, index) => [trip.id, dataColors[index % dataColors.length]]))
}

/** Every day of the year spent on one of the trips, keyed by ISO date. */
export function tripDaysInYear(trips: Trip[], year: number) {
  const days = new Map<string, Trip>()
  const first = parseDay(`${year}-01-01`)
  const last = parseDay(`${year}-12-31`)
  for (const trip of trips) {
    const start = Math.max(parseDay(trip.start), first)
    const end = Math.min(parseDay(trip.end), last)
    for (let time = start; time <= end; time += dayMs) days.set(isoDay(time), trip)
  }
  return days
}

/** The months of a year as their day counts, for a calendar. */
export function monthLengths(year: number) {
  return Array.from({ length: 12 }, (_, month) =>
    new Date(Date.UTC(year, month + 1, 0)).getUTCDate(),
  )
}

/** The day a trip spent the most on, or none when nothing was bought. */
export function tripPeakDay(trip: Pick<Trip, 'days'>) {
  return trip.days.reduce<Trip['days'][number] | undefined>(
    (peak, day) => (Math.abs(day.spent) > Math.abs(peak?.spent ?? 0) ? day : peak),
    undefined,
  )
}

/** The trips either side of one in time, within the same list: earlier and later. */
export function tripNeighbours(trips: Trip[], id: string) {
  const ordered = trips.toSorted((left, right) => left.start.localeCompare(right.start))
  const index = ordered.findIndex((trip) => trip.id === id)
  if (index < 0) return { earlier: undefined, later: undefined }
  return { earlier: ordered[index - 1], later: ordered[index + 1] }
}

/** When a trip was, in one line, naming the year only when it isn't the snapshot's. */
export function tripDates(trip: Pick<Trip, 'start' | 'end'>, referenceIso: string) {
  return trip.end === trip.start
    ? formatListDate(trip.start, referenceIso)
    : `${formatListDate(trip.start, referenceIso)} – ${formatListDate(trip.end, referenceIso)}`
}

type Stop = Trip['stops'][number]

/** Where a stop was, in one line, as a purchase's place reads. */
export const tripStopLabel = (stop: Stop) =>
  placeLabel({ ...stop, source: 'descriptor', precision: stop.city ? 'city' : 'region' })

/**
 * What a trip is called: the countries of a trip that crossed borders, otherwise the city with
 * the most purchases. Rust found the stops; this only words them.
 */
export function tripTitle(trip: Trip) {
  const countries = [...new Set(trip.stops.map(({ country }) => country))]
  if (trip.abroad && countries.length > 1) return countries.map(countryName).join(' & ')
  return trip.stops[0] ? tripStopLabel(trip.stops[0]) : trip.destination
}

/** The countries a trip abroad reached, as flags in the order of its stops; none at home. */
export function tripFlags(trip: Pick<Trip, 'abroad' | 'stops'>) {
  if (!trip.abroad) return ''
  return [...new Set(trip.stops.map(({ country }) => country))].map(countryFlag).join('')
}

/** The cities a trip went to, most purchases first, the rest counted. */
export function tripCities(trip: Trip, shown = 3) {
  const cities = trip.stops.flatMap(({ city }) => (city ? [city] : []))
  if (cities.length < 2) return ''
  const rest = cities.length - shown
  return `${cities.slice(0, shown).join(', ')}${rest > 0 ? ` +${rest}` : ''}`
}

export type TripPlace = {
  key: string
  label: string
  country: string
  abroad: boolean
  trips: number
  purchases: number
}

/** The places a year's trips stopped in, most purchases first, each counted once per trip. */
export function tripPlaces(trips: Trip[]): TripPlace[] {
  const places = new Map<string, TripPlace>()
  for (const trip of trips) {
    const seen = new Set<string>()
    for (const stop of trip.stops) {
      const key = `${stop.city ?? ''}|${stop.region ?? ''}|${stop.country}`
      const place = places.get(key) ?? {
        key,
        label: tripStopLabel(stop),
        country: stop.country,
        abroad: trip.abroad,
        trips: 0,
        purchases: 0,
      }
      place.purchases += stop.purchases
      if (!seen.has(key)) place.trips += 1
      seen.add(key)
      places.set(key, place)
    }
  }
  return [...places.values()].toSorted(
    (left, right) => right.purchases - left.purchases || left.label.localeCompare(right.label),
  )
}
