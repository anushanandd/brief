import { useId, useState, type ReactNode } from 'react'

import { formatCurrency, valueTone } from '../lib/format'
import { ChevronRight } from './icons'

export type RankedBar = {
  name: string
  value: number
  color: string
  /** Change against the comparable previous window; omitted when there is none. */
  change?: number | null
  /** What `change` compares with, marked on the track; null or omitted when unknown. */
  mark?: number | null
  /** Shown beneath the row when it is expanded, usually the activity behind the total. */
  details?: ReactNode
}

/**
 * Largest-first horizontal bars. Rows past `limit` fold into one row that expands to all.
 * With `onSelect`, a row is a toggle that selects one item and shows its details; otherwise
 * each row expands independently.
 */
export function RankedBars({
  items,
  label,
  favorable = 'decrease',
  limit = 7,
  emptyMessage,
  selected,
  onSelect,
  comparison = 'the previous period',
  markLabel = 'usually by this point',
}: {
  items: RankedBar[]
  label: string
  favorable?: 'increase' | 'decrease'
  limit?: number
  emptyMessage: string
  selected?: string | null
  onSelect?: (name: string | null) => void
  /** What `change` is measured against, for screen readers. */
  comparison?: string
  /** What the track's mark is, for screen readers. */
  markLabel?: string
}) {
  const id = useId()
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set())
  const [showAll, setShowAll] = useState(false)
  const sorted = items
    .filter(({ value }) => value !== 0)
    .toSorted((a, b) => Math.abs(b.value) - Math.abs(a.value))
  // A selected row past the fold stays visible.
  const folded =
    !showAll &&
    sorted.length > limit &&
    !sorted.slice(limit - 1).some(({ name }) => name === selected)
  const visible = folded ? sorted.slice(0, limit - 1) : sorted
  const rest = folded ? sorted.slice(limit - 1) : []
  const maximum = Math.max(
    ...sorted.map(({ value, mark }) => Math.max(Math.abs(value), Math.abs(mark ?? 0))),
    1,
  )
  const comparable = items.some(({ change }) => change !== undefined)
  if (!sorted.length) return <p className="ranked-bars-empty">{emptyMessage}</p>

  const toggle = (name: string) =>
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      return next
    })
  const cells = (
    { name, value, color, change, mark }: Omit<RankedBar, 'value'> & { value: number | null },
    expandable: boolean,
  ) => (
    <>
      <span className="ranked-bars-name">
        {expandable ? <ChevronRight className="ranked-bars-chevron" size={14} /> : null}
        <i style={{ background: color }} aria-hidden="true" />
        <span>{name}</span>
      </span>
      <span className="ranked-bars-track" aria-hidden="true">
        {value == null ? null : (
          <span style={{ width: `${(Math.abs(value) / maximum) * 100}%`, background: color }} />
        )}
        {mark != null ? (
          <i
            className="ranked-bars-mark"
            style={{ left: `${(Math.abs(mark) / maximum) * 100}%` }}
          />
        ) : null}
      </span>
      <strong className="sensitive-value">
        {value == null ? null : formatCurrency(value)}
        {mark != null ? (
          <span className="sr-only">{`, ${markLabel} ${formatCurrency(mark)}`}</span>
        ) : null}
      </strong>
      {comparable ? (
        <small
          className={`sensitive-value ${
            change ? valueTone(favorable === 'decrease' ? -change : change) : 'muted'
          }`}
        >
          {change == null
            ? ''
            : change === 0
              ? 'No change'
              : `${change > 0 ? '+' : ''}${formatCurrency(change)}`}
          {change == null ? null : <span className="sr-only"> versus {comparison}</span>}
        </small>
      ) : null}
    </>
  )

  return (
    <ul className={`ranked-bars${comparable ? '' : ' ranked-bars-totals'}`} aria-label={label}>
      {visible.map((item, index) => {
        const open = onSelect ? selected === item.name : expanded.has(item.name)
        const panel = `${id}-${index}`
        return (
          <li
            key={item.name}
            data-open={open || undefined}
            data-dimmed={(onSelect && selected != null && !open) || undefined}
          >
            {item.details || onSelect ? (
              <>
                <button
                  type="button"
                  className="ranked-bars-row"
                  aria-expanded={item.details ? open : undefined}
                  aria-pressed={onSelect ? open : undefined}
                  aria-controls={item.details ? panel : undefined}
                  onClick={() => (onSelect ? onSelect(open ? null : item.name) : toggle(item.name))}
                >
                  {/* Selecting filters elsewhere, so only a row with details shows the chevron. */}
                  {cells(item, Boolean(item.details))}
                </button>
                {open && item.details ? (
                  <div className="ranked-bars-details" id={panel}>
                    {item.details}
                  </div>
                ) : null}
              </>
            ) : (
              <div className="ranked-bars-row">{cells(item, false)}</div>
            )}
          </li>
        )
      })}
      {rest.length ? (
        <li>
          <button
            type="button"
            className="ranked-bars-row"
            aria-expanded={false}
            onClick={() => setShowAll(true)}
          >
            {/* The folded rows read as themselves once shown, never as a sum made here. */}
            {cells(
              {
                name: `${rest.length} more`,
                value: null,
                color: 'var(--text-tertiary)',
                change: comparable ? null : undefined,
              },
              true,
            )}
          </button>
        </li>
      ) : null}
    </ul>
  )
}
