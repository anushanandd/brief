import { countOf, formatCurrency } from '../lib/format'
import type { Trip } from '../lib/schema'
import { formatListDate } from '../lib/spending'
import { monthLengths, tripDates, tripDayCount, tripDaysInYear, tripTitle } from '../lib/trips'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

const pad = (value: number) => String(value).padStart(2, '0')

/**
 * The year as twelve wall-calendar months, each trip a band in its color across the days it
 * covered, broken only at the ends of weeks. Days still to come stay dim so a quiet year and an
 * unfinished one don't look alike. Each day away opens its trip; pointing at one picks its trip
 * out on the calendar and in the list beside it.
 */
export function TripsCalendar({
  year,
  today,
  trips,
  colors,
  hovered,
  onHover,
  onOpen,
  referenceIso,
}: {
  year: number
  today: string
  trips: Trip[]
  colors: Map<string, string>
  hovered?: string
  onHover: (id: string | undefined) => void
  onOpen: (id: string) => void
  referenceIso: string
}) {
  const away = tripDaysInYear(trips, year)
  const lengths = monthLengths(year)
  return (
    <figure
      className={`trips-calendar${hovered ? ' is-picking' : ''}`}
      aria-label={`Days away in ${year}`}
    >
      {months.map((month, index) => {
        const weekday = new Date(Date.UTC(year, index, 1)).getUTCDay()
        return (
          <div key={month} className="trips-calendar-month">
            <span className="trips-calendar-label">{month}</span>
            <div className="trips-calendar-days">
              {Array.from({ length: lengths[index] }, (_, day) => {
                const iso = `${year}-${pad(index + 1)}-${pad(day + 1)}`
                const column = (weekday + day) % 7
                const trip = away.get(iso)
                const place = day === 0 ? { gridColumnStart: weekday + 1 } : undefined
                const state = iso === today ? ' is-today' : iso > today ? ' is-future' : ''
                if (!trip)
                  return (
                    <span
                      key={iso}
                      className={`trips-day${state}`}
                      style={place}
                      aria-hidden="true"
                    >
                      {day + 1}
                    </span>
                  )
                const ends = [
                  iso === trip.start || column === 0 || day === 0 ? ' is-run-start' : '',
                  iso === trip.end || column === 6 || day === lengths[index] - 1
                    ? ' is-run-end'
                    : '',
                  hovered === trip.id ? ' is-picked' : '',
                ].join('')
                return (
                  <Tooltip key={iso}>
                    <TooltipTrigger
                      render={
                        <button
                          type="button"
                          className={`trips-day is-away${state}${ends}`}
                          style={{ ...place, color: colors.get(trip.id) }}
                          data-command={`trips.open.${trip.id}`}
                          aria-label={`${formatListDate(iso, referenceIso)}: ${tripTitle(trip)}`}
                          onClick={() => onOpen(trip.id)}
                          onPointerEnter={() => onHover(trip.id)}
                          onPointerLeave={() => onHover(undefined)}
                        >
                          <span>{day + 1}</span>
                        </button>
                      }
                    />
                    <TooltipContent className="treemap-tooltip" sideOffset={8}>
                      <div className="treemap-tooltip-heading">
                        <strong>{tripTitle(trip)}</strong>
                        <span className="sensitive-value">
                          {formatCurrency(Math.abs(trip.total))}
                        </span>
                      </div>
                      <dl className="treemap-tooltip-metrics">
                        <div>
                          <dt>{tripDates(trip, referenceIso)}</dt>
                          <dd>{countOf(tripDayCount(trip), 'day')}</dd>
                        </div>
                      </dl>
                    </TooltipContent>
                  </Tooltip>
                )
              })}
            </div>
          </div>
        )
      })}
    </figure>
  )
}
