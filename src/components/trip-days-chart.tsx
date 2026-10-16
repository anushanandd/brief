import type { ActivityItem } from '../lib/activity'
import { countOf, formatActivityName, formatCurrency } from '../lib/format'
import type { Trip } from '../lib/schema'
import { tripPeakDay, tripTitle } from '../lib/trips'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const weekday = new Intl.DateTimeFormat('en-US', { weekday: 'narrow', timeZone: 'UTC' })
const dayOfMonth = new Intl.DateTimeFormat('en-US', { day: 'numeric', timeZone: 'UTC' })
const longDay = new Intl.DateTimeFormat('en-US', {
  weekday: 'short',
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})

const at = (date: string) => new Date(`${date}T12:00:00Z`)

/**
 * A trip day by day: one bar per calendar day, the biggest day set apart, and the purchases
 * behind each day on hover. Rust summed the days; this only draws them. The mini size heads a
 * trip's tile and carries no labels or tooltips.
 */
export function TripDaysChart({
  trip,
  color,
  activities,
  mini = false,
}: {
  trip: Trip
  color?: string
  activities?: Map<string, ActivityItem>
  mini?: boolean
}) {
  const days = trip.days
  if (!days.length) return null
  const peak = tripPeakDay(trip)
  const maximum = Math.max(...days.map(({ spent }) => Math.abs(spent)), 0.01)
  const labelEvery = days.length > 16 ? Math.ceil(days.length / 8) : 1
  const byDay = new Map<string, ActivityItem[]>()
  if (activities)
    for (const id of trip.transactionIds) {
      const item = activities.get(`spending:${id}`)
      if (!item) continue
      const key = item.date.slice(0, 10)
      byDay.set(key, [...(byDay.get(key) ?? []), item])
    }
  return (
    <div
      className={`trip-days${mini ? ' is-mini' : ''}`}
      role="img"
      aria-label={
        mini
          ? `${tripTitle(trip)} day by day`
          : `${tripTitle(trip)} day by day: ${peak ? `most on ${longDay.format(at(peak.date))}, ${formatCurrency(Math.abs(peak.spent))}` : 'nothing bought'}`
      }
      style={{ color }}
    >
      {days.map((day, index) => {
        const height = `${(Math.abs(day.spent) / maximum) * 100}%`
        const isPeak = peak?.date === day.date && day.spent !== 0
        const bar = (
          <span
            className={`trip-day${isPeak ? ' is-peak' : ''}${day.spent === 0 ? ' is-empty' : ''}`}
          >
            <span className="trip-day-bar" style={{ height }} />
            {!mini && index % labelEvery === 0 ? (
              <span className="trip-day-label">
                <small>{weekday.format(at(day.date))}</small>
                {dayOfMonth.format(at(day.date))}
              </span>
            ) : !mini ? (
              <span className="trip-day-label" />
            ) : null}
          </span>
        )
        if (mini)
          return (
            <span key={day.date} className="trip-day-slot">
              {bar}
            </span>
          )
        const items = byDay.get(day.date) ?? []
        return (
          <Tooltip key={day.date}>
            <TooltipTrigger render={<span className="trip-day-slot" />}>{bar}</TooltipTrigger>
            <TooltipContent className="treemap-tooltip" sideOffset={8}>
              <div className="treemap-tooltip-heading">
                <strong>{longDay.format(at(day.date))}</strong>
                <span className="sensitive-value">{formatCurrency(Math.abs(day.spent))}</span>
              </div>
              {items.length ? (
                <div className="treemap-tooltip-composition">
                  <ul>
                    {items.slice(0, 6).map((item) => (
                      <li key={item.id}>
                        <span>{formatActivityName(item.title)}</span>
                        <strong className="sensitive-value">{formatCurrency(item.amount)}</strong>
                      </li>
                    ))}
                    {items.length > 6 ? (
                      <li className="muted">
                        <span>{`And ${countOf(items.length - 6, 'more')}`}</span>
                      </li>
                    ) : null}
                  </ul>
                </div>
              ) : (
                <div className="treemap-tooltip-composition">
                  <span>
                    {day.purchases ? countOf(day.purchases, 'purchase') : 'Nothing bought'}
                  </span>
                </div>
              )}
            </TooltipContent>
          </Tooltip>
        )
      })}
    </div>
  )
}
