import { useRef, useState, type PointerEvent } from 'react'

import type { ActivityItem } from '../lib/activity'
import { addDays, daysBetween } from '../lib/dates'
import {
  countOf,
  formatActivityName,
  formatCompactCurrency,
  formatCurrency,
  formatShortDay,
} from '../lib/format'
import { ActivityMark } from './activity-list'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const width = 1000
const height = 220

function niceMaximum(value: number) {
  if (value <= 0) return 100
  const magnitude = 10 ** Math.floor(Math.log10(value))
  const step = [1, 2, 2.5, 5, 10].find((candidate) => candidate * magnitude >= value / 4)!
  return Math.ceil(value / (step * magnitude)) * step * magnitude
}

function DayTransactions({
  items,
  activities,
}: {
  items: Array<{ id: string; value: number }>
  activities: Map<string, ActivityItem>
}) {
  if (!items.length) return null
  return (
    <div className="treemap-tooltip-composition">
      <ul>
        {items.slice(0, 8).flatMap((item) => {
          const activity = activities.get(item.id)
          if (!activity) return []
          return [
            <li key={item.id}>
              <ActivityMark activity={activity} />
              <span>
                {formatActivityName(activity.title)}
                {activity.subcategory ? (
                  <small className="spending-bar-tooltip-label">
                    {' · '}
                    {activity.subcategory}
                  </small>
                ) : null}
              </span>
              <strong className="sensitive-value">{formatCurrency(item.value)}</strong>
            </li>,
          ]
        })}
      </ul>
      {items.length > 8 ? (
        <small className="spending-bar-tooltip-label">{items.length - 8} more</small>
      ) : null}
    </div>
  )
}

const linePath = (points: Array<[number, number]>) =>
  points.map(([x, y], index) => `${index ? 'L' : 'M'} ${x.toFixed(1)} ${y.toFixed(1)}`).join(' ')

/**
 * Running spend for the selected period against the whole previous period. The projection is
 * dotted and labeled so it never reads as observed spending.
 */
/**
 * The inclusive dates a drag covers, earliest first, or nothing for a click on one day.
 * `start` is the first day of the plot and the indexes count days from it.
 */
export function dragRange(start: string, fromIndex: number, toIndex: number) {
  if (fromIndex === toIndex) return undefined
  return {
    from: addDays(start, Math.min(fromIndex, toIndex)),
    to: addDays(start, Math.max(fromIndex, toIndex)),
  }
}

export function SpendingPaceChart({
  pace,
  currentLabel,
  previousLabel,
  activities,
  color,
  highlightedIds,
  onScrubDay,
  onSelectRange,
}: {
  pace: {
    start: string
    end: string
    current: number[]
    previous: number[]
    projected: number | null
    days: Array<{
      index: number
      date: string
      total: number
      activities: Array<{ id: string; value: number }>
    }>
  }
  currentLabel: string
  previousLabel: string
  activities: Map<string, ActivityItem>
  /** The series color, such as a focused category's. */
  color?: string
  /** Activity to mark on the line, such as a row hovered in a linked list. */
  highlightedIds?: ReadonlySet<string>
  /** The activity ids of the day under the pointer, as it moves from day to day; null on leave. */
  onScrubDay?: (ids: string[] | null) => void
  /** Called with inclusive dates after a drag across two or more days. */
  onSelectRange?: (from: string, to: string) => void
}) {
  const periodDays = daysBetween(pace.start, pace.end) + 1
  const days = Math.max(periodDays, pace.previous.length, pace.current.length, 2)
  const spent = pace.current.at(-1) ?? 0
  const previousTotal = pace.previous.at(-1) ?? null
  const previousAtPoint = pace.current.length
    ? (pace.previous[Math.min(pace.current.length, pace.previous.length) - 1] ?? null)
    : null
  const maximum = niceMaximum(
    Math.max(spent, previousTotal ?? 0, pace.projected ?? 0, ...pace.previous),
  )
  const x = (index: number) => (index / (days - 1)) * width
  const y = (value: number) => height - (value / maximum) * height
  const current = pace.current.map((value, index): [number, number] => [x(index), y(value)])
  const previous = pace.previous.map((value, index): [number, number] => [x(index), y(value)])
  const today = current.at(-1)
  const projection: Array<[number, number]> | null =
    today && pace.projected != null ? [today, [x(periodDays - 1), y(pace.projected)]] : null
  const axis = [maximum, (maximum * 3) / 4, maximum / 2, maximum / 4, 0]
  const summary = [
    `${currentLabel}: ${formatCurrency(spent)} spent`,
    previousAtPoint != null
      ? `${previousLabel} at this point: ${formatCurrency(previousAtPoint)}`
      : null,
    previousTotal != null ? `${previousLabel} total: ${formatCurrency(previousTotal)}` : null,
    pace.projected != null ? `projected ${formatCurrency(pace.projected)}` : null,
  ]
    .filter(Boolean)
    .join('; ')
  // A continuous position (in days) so the crosshair glides with the pointer; the readout
  // describes the nearest day. It stays put while fading out after the pointer leaves.
  const [scrubState, setScrubState] = useState<{ position: number; visible: boolean } | null>(null)
  const reportedDay = useRef<number | null>(null)
  const daysByIndex = new Map(pace.days.map((day) => [day.index, day]))
  const lastIndex = Math.max(pace.current.length, pace.previous.length) - 1
  const dateAt = (index: number) => formatShortDay(addDays(pace.start, index))
  // A drag across the plot selects those days; releasing opens the rows behind them.
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null)
  const dayAt = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width))
    return Math.min(lastIndex, ratio * (days - 1))
  }
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!onSelectRange || lastIndex < 1 || event.button !== 0) return
    const index = Math.round(dayAt(event))
    event.currentTarget.setPointerCapture?.(event.pointerId)
    setDrag({ from: index, to: index })
  }
  const onPointerUp = () => {
    const range = drag && dragRange(pace.start, drag.from, drag.to)
    setDrag(null)
    if (range) onSelectRange?.(range.from, range.to)
  }
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (lastIndex < 0) return
    const position = dayAt(event)
    setScrubState({ position, visible: true })
    const index = Math.round(position)
    if (drag && drag.to !== index) setDrag({ ...drag, to: index })
    if (reportedDay.current !== index) {
      reportedDay.current = index
      onScrubDay?.(daysByIndex.get(index)?.activities.map(({ id }) => id) ?? [])
    }
  }
  const onPointerLeave = () => {
    setDrag(null)
    setScrubState((state) => state && { ...state, visible: false })
    reportedDay.current = null
    onScrubDay?.(null)
  }
  const lineValueAt = (position: number) => {
    if (position > pace.current.length - 1) return null
    const lower = Math.floor(position)
    const upper = Math.min(lower + 1, pace.current.length - 1)
    const fraction = position - lower
    return pace.current[lower] + (pace.current[upper] - pace.current[lower]) * fraction
  }
  const scrub = scrubState
    ? (() => {
        const index = Math.round(scrubState.position)
        return {
          index,
          visible: scrubState.visible,
          left: (x(scrubState.position) / width) * 100,
          lineValue: lineValueAt(scrubState.position),
          running: pace.current[index] ?? null,
          prior: pace.previous[index] ?? null,
          items: daysByIndex.get(index)?.activities ?? [],
        }
      })()
    : null
  const visibleClass = scrub?.visible ? ' is-visible' : ''

  return (
    <figure className="pace-chart" style={color ? { '--pace-accent': color } : undefined}>
      <div
        className="pace-chart-plot"
        role="group"
        aria-label={summary}
        onPointerMove={onPointerMove}
        onPointerLeave={onPointerLeave}
        onPointerDown={onPointerDown}
        onPointerUp={onPointerUp}
        onPointerCancel={() => setDrag(null)}
        data-selectable={onSelectRange ? true : undefined}
      >
        {drag && drag.from !== drag.to ? (
          <span
            className="pace-chart-selection"
            aria-hidden="true"
            style={{
              left: `${(x(Math.min(drag.from, drag.to)) / width) * 100}%`,
              width: `${(Math.abs(x(drag.to) - x(drag.from)) / width) * 100}%`,
            }}
          />
        ) : null}
        <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
          {axis.map((value) => (
            <line
              key={value}
              className="pace-chart-grid"
              x1="0"
              x2={width}
              y1={y(value)}
              y2={y(value)}
              vectorEffect="non-scaling-stroke"
            />
          ))}
          {previous.length > 1 ? (
            <path
              className="pace-chart-previous"
              d={linePath(previous)}
              vectorEffect="non-scaling-stroke"
            />
          ) : null}
          {current.length > 1 ? (
            <path
              className="pace-chart-area"
              d={`${linePath(current)} L ${current.at(-1)![0]} ${height} L 0 ${height} Z`}
            />
          ) : null}
          {projection ? (
            <path
              className="pace-chart-projection"
              d={linePath(projection)}
              vectorEffect="non-scaling-stroke"
            />
          ) : null}
          {current.length > 1 ? (
            <path
              className="pace-chart-current"
              d={linePath(current)}
              vectorEffect="non-scaling-stroke"
            />
          ) : null}
        </svg>
        {scrub ? (
          <span
            className={`pace-chart-crosshair${visibleClass}`}
            aria-hidden="true"
            style={{ left: `${scrub.left}%` }}
          />
        ) : null}
        {pace.days.map((day) => {
          const running = pace.current[day.index]
          if (running == null) return null
          const date = dateAt(day.index)
          const dayTotal = day.total
          const count = day.activities.length
          return (
            <Tooltip key={day.date}>
              <TooltipTrigger
                render={
                  <span
                    role="img"
                    tabIndex={0}
                    className={`pace-chart-transaction${
                      (scrub?.visible && scrub.index === day.index) ||
                      day.activities.some(({ id }) => highlightedIds?.has(id))
                        ? ' is-active'
                        : ''
                    }`}
                    aria-label={`${date}: ${countOf(count, 'transaction')}, ${formatCurrency(dayTotal)}`}
                    style={{
                      left: `${(x(day.index) / width) * 100}%`,
                      top: `${(y(running) / height) * 100}%`,
                    }}
                  />
                }
              />
              <TooltipContent
                className="treemap-tooltip sankey-tooltip spending-bar-tooltip"
                sideOffset={10}
              >
                <div className="treemap-tooltip-heading">
                  <strong>{date}</strong>
                  <span className="sensitive-value">{formatCurrency(dayTotal)}</span>
                </div>
                <DayTransactions items={day.activities} activities={activities} />
              </TooltipContent>
            </Tooltip>
          )
        })}
        {scrub?.lineValue != null ? (
          <span
            className={`pace-chart-scrub-dot${visibleClass}`}
            aria-hidden="true"
            style={{ left: `${scrub.left}%`, top: `${(y(scrub.lineValue) / height) * 100}%` }}
          />
        ) : null}
        {scrub ? (
          <div
            className={`tooltip-content treemap-tooltip sankey-tooltip spending-bar-tooltip pace-chart-readout${visibleClass}${scrub.left > 55 ? ' is-flipped' : ''}`}
            aria-hidden="true"
            style={{ left: `${scrub.left}%` }}
          >
            <div className="treemap-tooltip-heading">
              <strong>{dateAt(scrub.index)}</strong>
              {scrub.running != null ? (
                <span className="sensitive-value">{formatCurrency(scrub.running)}</span>
              ) : null}
            </div>
            {scrub.prior != null ? (
              <small className="spending-bar-tooltip-label">
                {previousLabel}{' '}
                <span className="sensitive-value">{formatCurrency(scrub.prior)}</span>
              </small>
            ) : null}
            {scrub.running != null ? (
              scrub.items.length ? (
                <DayTransactions items={scrub.items} activities={activities} />
              ) : (
                <small className="spending-bar-tooltip-label">No spending</small>
              )
            ) : null}
          </div>
        ) : null}
        {today ? (
          <span
            className="pace-chart-dot"
            aria-hidden="true"
            style={{ left: `${(today[0] / width) * 100}%`, top: `${(today[1] / height) * 100}%` }}
          />
        ) : null}
        <div className="pace-chart-axis" aria-hidden="true">
          {axis.map((value) => (
            <span key={value} style={{ top: `${(y(value) / height) * 100}%` }}>
              {formatCompactCurrency(value)}
            </span>
          ))}
        </div>
      </div>
      <div className="pace-chart-dates" aria-hidden="true">
        <span>{formatShortDay(pace.start)}</span>
        <span>{formatShortDay(pace.end)}</span>
      </div>
      <figcaption className="pace-chart-legend">
        <span>
          <i className="pace-key pace-key-current" aria-hidden="true" />
          {currentLabel}
          <strong className="sensitive-value">{formatCurrency(spent)}</strong>
        </span>
        {previousAtPoint != null ? (
          <span>
            <i className="pace-key pace-key-previous" aria-hidden="true" />
            {previousLabel} at this point
            <strong className="sensitive-value">{formatCurrency(previousAtPoint)}</strong>
            {previousTotal != null && pace.current.length < pace.previous.length ? (
              <small className="sensitive-value">of {formatCurrency(previousTotal)}</small>
            ) : null}
          </span>
        ) : null}
        {pace.projected != null ? (
          <span>
            <i className="pace-key pace-key-projection" aria-hidden="true" />
            Projected
            <strong className="sensitive-value">{formatCurrency(pace.projected)}</strong>
          </span>
        ) : null}
      </figcaption>
    </figure>
  )
}
