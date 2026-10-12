import { formatProseCurrency } from '../lib/format'
import type { Trip } from '../lib/schema'
import { formatListDate } from '../lib/spending'
import { Plane } from './icons'
import { Card, SectionHeading } from './ui'

/**
 * Trips away from home that overlap the page's period, from native insights. Choosing one
 * highlights its charges in the activity beside it; nothing renders without a trip.
 */
export function TripsCard({
  trips,
  referenceIso,
  selectedId,
  onSelect,
}: {
  trips: Trip[]
  referenceIso: string
  selectedId?: string
  onSelect: (trip: Trip | null) => void
}) {
  if (!trips.length) return null
  return (
    <Card className="spending-trips-card">
      <SectionHeading title="Trips" />
      <ul className="trips-list" aria-label="Trips">
        {trips.map((trip) => {
          const selected = trip.id === selectedId
          return (
            <li key={trip.id}>
              <button
                type="button"
                className="trips-row"
                data-keyboard-row
                aria-pressed={selected}
                onClick={() => onSelect(selected ? null : trip)}
              >
                <span className="transaction-mark" aria-hidden="true">
                  <Plane size={15} />
                </span>
                <span className="trips-copy">
                  <strong>
                    {trip.destination}
                    {trip.ongoing ? <small> · now</small> : null}
                  </strong>
                  <small>
                    {formatListDate(trip.start, referenceIso)}
                    {trip.end !== trip.start ? ` – ${formatListDate(trip.end, referenceIso)}` : ''}
                    {trip.categories.length
                      ? ` · ${trip.categories.map(({ name }) => name).join(', ')}`
                      : ''}
                  </small>
                </span>
                <strong className="sensitive-value">
                  {formatProseCurrency(Math.abs(trip.total))}
                </strong>
              </button>
            </li>
          )
        })}
      </ul>
    </Card>
  )
}
