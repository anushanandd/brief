import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useMemo, useRef, useState, type RefObject } from 'react'
import { toast } from 'sonner'

import { ActivityList } from '../components/activity-list'
import { CarFront, House, Plane, Plus } from '../components/icons'
import { PeoplePicker } from '../components/people-picker'
import { PersonAvatar } from '../components/person-avatar'
import { ShareLine, type ShareSegment } from '../components/share-line'
import { TransactionDialog } from '../components/transaction-dialog'
import { TripDaysChart } from '../components/trip-days-chart'
import { TripsCalendar } from '../components/trips-calendar'
import {
  AnimatedCurrency,
  Button,
  Card,
  ChartRangeSelect,
  EmptyState,
  Metric,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { useContactPhotos } from '../hooks/use-contact-photos'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ActivityItem } from '../lib/activity'
import { isTauri } from '../lib/api'
import {
  attendingPeople,
  calendarEvents,
  calendarList,
  calendarStatus,
  eventTime,
} from '../lib/calendar'
import { addDays } from '../lib/dates'
import { errorMessage } from '../lib/errors'
import { countOf, formatCurrency, formatShortDay } from '../lib/format'
import { peopleById, withTripCompanions } from '../lib/people'
import { countryFlag, countryName } from '../lib/places'
import type { FinanceSnapshot, Trip } from '../lib/schema'
import { formatListDate } from '../lib/spending'
import {
  tripCities,
  tripColors,
  tripDates,
  tripDayCount,
  tripFlags,
  tripNeighbours,
  tripPeakDay,
  tripPlaces,
  tripStopLabel,
  tripTitle,
  tripYears,
  tripYearTotals,
  type TripPlace,
} from '../lib/trips'

const inYear = (option: number) => (trip: Trip) => Number(trip.start.slice(0, 4)) === option

const longDay = new Intl.DateTimeFormat('en-US', {
  weekday: 'long',
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})

const tripSearch = (trip: Trip) => ({ trip: trip.id })

/** The trip a search names, when it exists, for the page's breadcrumb. */
export function tripFromSearch(data: FinanceSnapshot, id: string | undefined) {
  return id ? data.insights?.trips.trips.find((trip) => trip.id === id) : undefined
}

/**
 * Insights' Trips view: trips away from home, found natively from in-store purchases. The year
 * is the way in: a wall calendar of the days away, a row per trip, and the places reached. Each
 * trip is then its own view: its route, the money day by day, where it went, and every purchase.
 */
export function TripsView({ data, activities }: { data: FinanceSnapshot; activities: Activities }) {
  const search = useSearch({ from: '/insights' })
  const trips = data.insights?.trips.trips ?? []
  const [openTransaction, setOpenTransaction] = useState<string>()
  const trip = tripFromSearch(data, search.trip)
  return (
    <>
      {trip ? (
        <TripView
          data={data}
          trip={trip}
          trips={trips}
          activities={activities}
          onOpenTransaction={setOpenTransaction}
        />
      ) : (
        <YearView data={data} trips={trips} />
      )}
      <TransactionDialog
        activity={openTransaction ? activities.get(openTransaction) : undefined}
        data={data}
        onClose={() => setOpenTransaction(undefined)}
      />
    </>
  )
}

type Activities = Map<string, ActivityItem>

/** One calendar year: what it cost, the days away, a row per trip, and the places. */
function YearView({ data, trips }: { data: FinanceSnapshot; trips: Trip[] }) {
  const search = useSearch({ from: '/insights' })
  const navigate = useNavigate({ from: '/insights' })
  const [hovered, setHovered] = useState<string>()
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const currentYear = Number(today.slice(0, 4))
  const home = data.insights?.trips.home ?? []
  const years = tripYears(trips, today)
  const year = search.year && years.includes(search.year) ? search.year : currentYear
  const shown = trips.filter(inYear(year))
  const totals = tripYearTotals(shown)
  const lastYear = tripYearTotals(trips.filter(inYear(year - 1)))
  const colors = useMemo(() => tripColors(shown), [shown])
  const longest = totals.longest ? tripDayCount(totals.longest) : 1
  const now = trips.find(({ ongoing }) => ongoing)
  const setYear = useCallback(
    (next: number) => void navigate({ search: next === currentYear ? {} : { year: next } }),
    [currentYear, navigate],
  )
  const openTrip = useCallback((id: string) => void navigate({ search: { trip: id } }), [navigate])
  const yearIndex = years.indexOf(year)

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        ...years.map((option) => ({
          id: `trips.year.${option}`,
          title: option === currentYear ? 'Show this year’s trips' : `Show ${option} trips`,
          search: `trips travel year ${option}`,
          group: 'Here',
          scope: 'page' as const,
          run: () => setYear(option),
        })),
        {
          id: 'trips.year.previous',
          title: 'Previous year',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowLeft'], label: '←' }],
          disabled: yearIndex >= years.length - 1,
          disabledReason: 'No earlier year with trips',
          repeatable: true,
          run: () => setYear(years[Math.min(years.length - 1, yearIndex + 1)]),
        },
        {
          id: 'trips.year.next',
          title: 'Next year',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowRight'], label: '→' }],
          disabled: yearIndex <= 0,
          disabledReason: 'Already showing this year',
          repeatable: true,
          run: () => setYear(years[Math.max(0, yearIndex - 1)]),
        },
        ...shown.map((trip) => ({
          id: `trips.open.${trip.id}`,
          title: `Open ${tripTitle(trip)}`,
          search: `trip ${tripTitle(trip)} ${tripCities(trip, 6)} ${trip.start}`,
          group: 'Here',
          scope: 'page' as const,
          run: () => openTrip(trip.id),
        })),
      ],
      [currentYear, openTrip, setYear, shown, yearIndex, years],
    ),
  )

  const comparison = lastYear.count
    ? `${year - 1}: ${countOf(lastYear.count, 'trip')}, ${countOf(lastYear.days, 'day')}, ${formatCurrency(lastYear.spent)}`
    : null

  return (
    <div className="trips-page">
      {now ? <AwayNow trip={now} color={colors.get(now.id)} referenceIso={data.updatedAt} /> : null}
      <Card className="trips-year-card">
        <div className="trips-year-top">
          <header className="trips-year-summary">
            <h2 className="balance-label">{`Spent away in ${year}`}</h2>
            <AnimatedCurrency className="hero-number sensitive-value" value={totals.spent} />
            {comparison ? <p className="trips-year-line sensitive-value">{comparison}</p> : null}
          </header>
          <div className="trips-year-stats">
            <Metric label="Trips" value={String(totals.count)} />
            <Metric label="Days away" value={String(totals.days)} />
            <Metric label="Abroad" value={String(totals.abroad)} />
            <Metric label="Flown" value={String(totals.flown)} />
          </div>
          <ChartRangeSelect
            label="Trips year"
            command="trips.year."
            options={years.map((option) => ({
              value: option,
              label: option === currentYear ? 'This year' : String(option),
              accessibleLabel: option === currentYear ? 'This year' : String(option),
            }))}
            value={year}
            onValueChange={setYear}
          />
        </div>
        <TripsCalendar
          year={year}
          today={today}
          trips={shown}
          colors={colors}
          hovered={hovered}
          onHover={setHovered}
          onOpen={openTrip}
          referenceIso={data.updatedAt}
        />
        <p className="trips-year-note">
          {home.length
            ? `Away from ${home.join(' and ')}: in-store purchases somewhere else, two or more no more than a few days apart.`
            : 'Trips need a few months of located in-store purchases to know home.'}
        </p>
      </Card>
      <div className="trips-grid">
        <Card className="trips-card">
          <SectionHeading title="Trips" />
          {shown.length ? (
            <ol className="trips-journeys" aria-label={`Trips in ${year}`}>
              {shown.map((trip) => (
                <li key={trip.id}>
                  <Journey
                    trip={trip}
                    color={colors.get(trip.id)}
                    longest={longest}
                    picked={hovered === trip.id}
                    onHover={setHovered}
                    referenceIso={data.updatedAt}
                  />
                </li>
              ))}
            </ol>
          ) : (
            <EmptyState>
              {trips.length || home.length
                ? `No trips in ${year}.`
                : 'Trips need a few months of located in-store purchases to know home.'}
            </EmptyState>
          )}
        </Card>
        <Places places={tripPlaces(shown)} year={year} />
      </div>
    </div>
  )
}

/** The trip under way, ahead of the year: how long, what it has cost, and its days so far. */
function AwayNow({
  trip,
  color,
  referenceIso,
}: {
  trip: Trip
  color?: string
  referenceIso: string
}) {
  return (
    <Card className="trips-now-card" style={{ '--trip-color': color }}>
      <div className="trips-now">
        <div className="trips-now-copy">
          <span className="trips-eyebrow">Away now</span>
          <h2 className="trips-now-title">
            {tripFlags(trip) ? <span className="trips-flags">{tripFlags(trip)} </span> : null}
            {tripTitle(trip)}
          </h2>
          <p className="trips-year-line">
            {[
              `${countOf(tripDayCount(trip), 'day')} so far`,
              `since ${formatListDate(trip.start, referenceIso)}`,
              tripCities(trip),
            ]
              .filter(Boolean)
              .join(' · ')}
          </p>
        </div>
        <TripDaysChart trip={trip} color={color} mini />
        <div className="trips-now-figures">
          <Metric
            label="Spent so far"
            value={<span className="sensitive-value">{formatCurrency(Math.abs(trip.total))}</span>}
          />
          <Metric
            label="A day"
            value={<span className="sensitive-value">{formatCurrency(Math.abs(trip.perDay))}</span>}
          />
        </div>
        <Link
          to="/insights"
          search={tripSearch(trip)}
          className="button-base button-secondary button-compact trips-now-open"
          data-command={`trips.open.${trip.id}`}
        >
          Open this trip
        </Link>
      </div>
    </Card>
  )
}

/**
 * A trip in the year's list: how it went, where, and its days as bars drawn to the year's
 * longest trip, so lengths compare down the list. Pointing at it picks it out on the calendar.
 */
function Journey({
  trip,
  color,
  longest,
  picked,
  onHover,
  referenceIso,
}: {
  trip: Trip
  color?: string
  longest: number
  picked: boolean
  onHover: (id: string | undefined) => void
  referenceIso: string
}) {
  const Vehicle = trip.flew ? Plane : CarFront
  const days = tripDayCount(trip)
  const flags = tripFlags(trip)
  return (
    <Link
      to="/insights"
      search={tripSearch(trip)}
      className={`trips-journey${picked ? ' is-picked' : ''}`}
      data-command={`trips.open.${trip.id}`}
      style={{ '--trip-color': color }}
      onPointerEnter={() => onHover(trip.id)}
      onPointerLeave={() => onHover(undefined)}
    >
      <span className="trips-journey-mark">
        <Vehicle size={15} />
      </span>
      <span className="trips-journey-copy">
        <strong>
          {flags ? <span className="trips-flags">{`${flags} `}</span> : null}
          {tripTitle(trip)}
          {trip.ongoing ? <small> · now</small> : null}
        </strong>
        <small>
          {[
            tripDates(trip, referenceIso),
            countOf(days, 'day'),
            tripCities(trip),
            trip.flew ? 'Flown' : null,
          ]
            .filter(Boolean)
            .join(' · ')}
        </small>
      </span>
      <span className="trips-journey-span">
        <span style={{ width: `${(days / longest) * 100}%` }}>
          <TripDaysChart trip={trip} color={color} mini />
        </span>
      </span>
      <span className="trips-journey-figures">
        <strong className="sensitive-value">{formatCurrency(Math.abs(trip.total))}</strong>
        <small className="sensitive-value">{`${formatCurrency(Math.abs(trip.perDay))} a day`}</small>
      </span>
    </Link>
  )
}

/** One trip: its route and money day by day, every purchase, and beside them the split. */
function TripView({
  data,
  trip,
  trips,
  activities,
  onOpenTransaction,
}: {
  data: FinanceSnapshot
  trip: Trip
  trips: Trip[]
  activities: Activities
  onOpenTransaction: (id: string) => void
}) {
  const navigate = useNavigate({ from: '/insights' })
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const currentYear = Number(today.slice(0, 4))
  const year = Number(trip.start.slice(0, 4))
  const siblings = trips.filter(inYear(year))
  const colors = useMemo(() => tripColors(siblings), [siblings])
  const color = colors.get(trip.id)
  const { earlier, later } = tripNeighbours(trips, trip.id)
  const peak = tripPeakDay(trip)
  const rows = trip.transactionIds
    .flatMap((id) => activities.get(`spending:${id}`) ?? [])
    .toSorted((left, right) => right.date.localeCompare(left.date))
  const go = useCallback(
    (next?: Trip) => {
      if (next) void navigate({ search: tripSearch(next) })
    },
    [navigate],
  )
  const back = useCallback(
    () => void navigate({ search: year === currentYear ? {} : { year } }),
    [currentYear, navigate, year],
  )
  const [editingPeople, setEditingPeople] = useState(false)
  const peopleInput = useRef<HTMLInputElement>(null)

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'trips.earlier',
          title: 'Earlier trip',
          detail: earlier ? tripTitle(earlier) : undefined,
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowLeft'], label: '←' }],
          disabled: !earlier,
          disabledReason: 'This is the earliest trip',
          run: () => go(earlier),
        },
        {
          id: 'trips.later',
          title: 'Later trip',
          detail: later ? tripTitle(later) : undefined,
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowRight'], label: '→' }],
          disabled: !later,
          disabledReason: 'This is the latest trip',
          run: () => go(later),
        },
        {
          id: 'trips.back',
          title: `Back to ${year}`,
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['Escape'], label: 'Esc' }],
          run: back,
        },
        {
          id: 'trips.people',
          title: 'Who went on this trip',
          search: 'trip people companions friends who went with',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['p'], label: 'P' }],
          run: () => {
            setEditingPeople(true)
            requestAnimationFrame(() => peopleInput.current?.focus())
          },
        },
      ],
      [back, earlier, go, later, year],
    ),
  )

  const line = [
    countOf(tripDayCount(trip), 'day'),
    `${formatCurrency(Math.abs(trip.perDay))} a day`,
    countOf(trip.transactionIds.length, 'purchase'),
    trip.flew ? 'Flown' : 'Not flown',
    trip.abroad ? 'Abroad' : null,
    trip.ongoing ? 'Still away' : null,
  ]
    .filter(Boolean)
    .join(' · ')

  return (
    <div className="trips-grid">
      <div className="trips-main">
        <Card className="trips-hero-card" style={{ '--trip-color': color }}>
          <div className="trips-year-top">
            <header className="trips-year-summary">
              <h2 className="balance-label">{tripDates(trip, data.updatedAt)}</h2>
              <AnimatedCurrency
                className="hero-number sensitive-value"
                value={Math.abs(trip.total)}
              />
              <p className="trips-year-line sensitive-value">{line}</p>
            </header>
            <div className="trips-steps">
              <Button
                size="compact"
                data-command="trips.earlier"
                disabled={!earlier}
                title={earlier ? `Earlier: ${tripTitle(earlier)}` : 'This is the earliest trip'}
                onClick={() => go(earlier)}
              >
                Earlier
              </Button>
              <Button
                size="compact"
                data-command="trips.later"
                disabled={!later}
                title={later ? `Later: ${tripTitle(later)}` : 'This is the latest trip'}
                onClick={() => go(later)}
              >
                Later
              </Button>
            </div>
          </div>
          <Companions
            data={data}
            trip={trip}
            editing={editingPeople}
            inputRef={peopleInput}
            onEdit={() => {
              setEditingPeople((open) => !open)
              requestAnimationFrame(() => peopleInput.current?.focus())
            }}
          />
          <Route trip={trip} referenceIso={data.updatedAt} />
          <SectionHeading title="Day by day" />
          <TripDaysChart trip={trip} color={color} activities={activities} />
          <p className="trips-year-note sensitive-value">
            {peak && peak.spent !== 0
              ? `Most on ${longDay.format(new Date(`${peak.date}T12:00:00Z`))}: ${formatCurrency(Math.abs(peak.spent))} over ${countOf(peak.purchases, 'purchase')}.`
              : 'Nothing was bought on this trip.'}
          </p>
        </Card>
        <Card className="trips-card">
          <SectionHeading title="Purchases" />
          {rows.length ? (
            <ActivityList
              activities={rows}
              referenceIso={data.updatedAt}
              onOpen={(activity) => onOpenTransaction(activity.id)}
            />
          ) : (
            <EmptyState>No purchases on this trip are in the snapshot.</EmptyState>
          )}
        </Card>
      </div>
      <div className="trips-main">
        <Card className="trips-places-card">
          <SectionHeading title="Where it went" />
          <Split trip={trip} />
        </Card>
        <TripCalendar data={data} trip={trip} />
      </div>
    </div>
  )
}

/**
 * The user's own events over the trip, a day either side, and anyone they keep in People who
 * was invited and isn't listed as going yet. Shown only once Brief may read Calendar.
 */
function TripCalendar({ data, trip }: { data: FinanceSnapshot; trip: Trip }) {
  const { updateSettings } = useFinanceEdits()
  const native = typeof window !== 'undefined' && isTauri()
  const status = useQuery({
    queryKey: ['calendar-status'],
    queryFn: calendarStatus,
    enabled: native,
  })
  const [start, end] = [addDays(trip.start, -1), addDays(trip.end, 1)]
  // A trip reads every calendar: what the Calendar page shows is a display choice.
  const calendars = useQuery({
    queryKey: ['calendar-list'],
    queryFn: calendarList,
    enabled: status.data?.access === 'allowed',
  })
  const ids = (calendars.data ?? []).map(({ id }) => id)
  const events = useQuery({
    queryKey: ['calendar-events', start, end, ids],
    queryFn: () => calendarEvents(start, end, ids),
    enabled: ids.length > 0,
    staleTime: 60 * 1000,
  })
  if (!events.data?.length) return null
  const chosen = trip.companions ?? []
  const invited = attendingPeople(events.data, data.settings?.people ?? [], chosen)
  return (
    <Card className="trips-places-card">
      <SectionHeading title="Your calendar" />
      <ul className="calendar-day-list">
        {events.data.map((event) => (
          <li key={event.id} data-keyboard-row>
            <div className="calendar-entry">
              <span className="calendar-entry-title">{event.title || 'Busy'}</span>
              <span className="calendar-entry-meta">
                {formatShortDay(event.start.slice(0, 10))}
              </span>
              <span className="calendar-entry-detail">
                {[eventTime(event), event.location].filter(Boolean).join(' · ')}
              </span>
            </div>
          </li>
        ))}
        {invited.map((person) => (
          <li key={person.id} data-keyboard-row>
            <Button
              size="compact"
              variant="ghost"
              icon={Plus}
              onClick={() =>
                void updateSettings((settings) =>
                  withTripCompanions(settings, trip, [...chosen, person.id]),
                ).catch((error) =>
                  toast.error('Could not save who went', { description: errorMessage(error) }),
                )
              }
            >
              {`${person.name} went`}
            </Button>
          </li>
        ))}
      </ul>
    </Card>
  )
}

/** Who went: their faces and names, and with P, a picker to change them. */
function Companions({
  data,
  trip,
  editing,
  inputRef,
  onEdit,
}: {
  data: FinanceSnapshot
  trip: Trip
  editing: boolean
  inputRef: RefObject<HTMLInputElement | null>
  onEdit: () => void
}) {
  const { updateSettings } = useFinanceEdits()
  const people = data.settings?.people ?? []
  const photos = useContactPhotos(people)
  const byId = peopleById({ people })
  const companions = (trip.companions ?? []).flatMap((id) => byId.get(id) ?? [])
  return (
    <div className="trips-companions">
      {editing ? (
        <PeoplePicker
          label="Add someone who went"
          chosen={trip.companions ?? []}
          people={people}
          photos={photos}
          pending={false}
          inputRef={inputRef}
          onChange={(next) =>
            updateSettings((settings) => withTripCompanions(settings, trip, next))
          }
        />
      ) : companions.length ? (
        <ul className="trips-companion-list" aria-label="Who went">
          {companions.map((person) => (
            <li key={person.id}>
              <PersonAvatar person={person} photos={photos} />
              {person.name}
            </li>
          ))}
        </ul>
      ) : null}
      <Button
        size="compact"
        variant="ghost"
        data-command="trips.people"
        aria-expanded={editing}
        aria-keyshortcuts="P"
        onClick={onEdit}
      >
        {editing ? 'Done' : 'Who went'}
      </Button>
    </div>
  )
}

/**
 * The route a trip took, left to right: out from home, each city in the order it was first
 * reached with its purchases, and home again once the trip is over.
 */
function Route({ trip, referenceIso }: { trip: Trip; referenceIso: string }) {
  const stops = trip.stops.toSorted((left, right) => left.first.localeCompare(right.first))
  if (!stops.length) return <EmptyState>No located purchases.</EmptyState>
  const Vehicle = trip.flew ? Plane : CarFront
  return (
    <ol className="trips-route" aria-label="Stops">
      <li className="is-home">
        <span className="trips-route-node" aria-hidden="true">
          <Vehicle size={12} />
        </span>
        <span className="trips-route-copy">
          <strong>Home</strong>
          <small>{formatListDate(trip.start, referenceIso)}</small>
        </span>
      </li>
      {stops.map((stop, index) => (
        <li key={`${stop.city ?? ''}|${stop.region ?? ''}|${stop.country}|${index}`}>
          <span className="trips-route-node" aria-hidden="true" />
          <span className="trips-route-copy">
            <strong>
              {trip.abroad ? (
                <span className="trips-flags">{`${countryFlag(stop.country)} `}</span>
              ) : null}
              {tripStopLabel(stop)}
            </strong>
            <small>
              {`${formatListDate(stop.first, referenceIso)} · ${countOf(stop.purchases, 'purchase')}`}
            </small>
          </span>
        </li>
      ))}
      {trip.ongoing ? null : (
        <li className="is-home">
          <span className="trips-route-node" aria-hidden="true">
            <House size={12} />
          </span>
          <span className="trips-route-copy">
            <strong>Home</strong>
            <small>{formatListDate(trip.end, referenceIso)}</small>
          </span>
        </li>
      )}
    </ol>
  )
}

/** Where a trip's money went: Rust ranked the top categories; the rest closes the line. */
function Split({ trip }: { trip: Trip }) {
  const total = Math.abs(trip.total)
  if (!total || !trip.categories.length) return <EmptyState>Nothing bought.</EmptyState>
  const segments: ShareSegment[] = trip.categories.map(({ name, amount }) => ({
    key: name,
    label: name,
    value: Math.abs(amount),
    percent: (Math.abs(amount) / total) * 100,
  }))
  const ranked = Math.round(segments.reduce((sum, { value }) => sum + value * 100, 0)) / 100
  const rest = Math.round((total - ranked) * 100) / 100
  const tail =
    rest > 0
      ? { key: 'rest', label: 'Everything else', value: rest, percent: (rest / total) * 100 }
      : undefined
  const legend = [
    ...segments.map((segment, index) => ({ ...segment, color: `var(--data-${index + 1})` })),
    ...(tail ? [{ ...tail, color: 'var(--border-strong)' }] : []),
  ]
  return (
    <div className="trips-split">
      <ShareLine label={`${tripTitle(trip)} by category`} segments={segments} rest={tail} />
      <ul className="trips-split-legend">
        {legend.map((segment) => (
          <li key={segment.key}>
            <span
              className="trips-split-swatch"
              style={{ background: segment.color }}
              aria-hidden="true"
            />
            <span>{segment.label}</span>
            <strong className="sensitive-value">{formatCurrency(segment.value)}</strong>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** The cities and countries a year's trips reached, most purchases first. */
function Places({ places, year }: { places: TripPlace[]; year: number }) {
  const countries = new Set(places.map(({ country }) => country))
  return (
    <ScrollCueCard className="trips-places-card" scrollSelector=".trips-places">
      <SectionHeading title="Places" />
      {places.length ? (
        <>
          <div className="benefit-metrics">
            <Metric label="Cities" value={String(places.length)} />
            <Metric label="Countries" value={String(countries.size)} />
          </div>
          <ul className="trips-places" aria-label={`Places in ${year}`} data-keyboard-region>
            {places.map((place) => (
              <li key={place.key} data-keyboard-row tabIndex={-1}>
                <span className="trips-place-mark" aria-hidden="true">
                  {countryFlag(place.country)}
                </span>
                <span className="trips-place-copy">
                  <strong>{place.label}</strong>
                  <small>{countryName(place.country)}</small>
                </span>
                <small>
                  {`${place.trips > 1 ? `${countOf(place.trips, 'trip')} · ` : ''}${countOf(place.purchases, 'purchase')}`}
                </small>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <EmptyState>{`No places away in ${year}.`}</EmptyState>
      )}
    </ScrollCueCard>
  )
}
