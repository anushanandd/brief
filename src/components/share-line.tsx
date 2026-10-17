import { formatCurrency, formatShare } from '../lib/format'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

export type ShareSegment = { key: string; label: string; value: number; percent: number }

const dataColors = Array.from({ length: 7 }, (_, index) => `var(--data-${index + 1})`)

/**
 * One line of a whole split into its largest parts, each with its own color; the parts past the
 * palette share one quiet color, and `rest` (such as cash) closes the line. Every value and
 * share comes from the caller, so a folded part reads as itself rather than as a sum.
 */
export function ShareLine({
  label,
  segments,
  rest,
}: {
  label: string
  /** Largest first; parts past the palette share a quiet color. */
  segments: ShareSegment[]
  rest?: ShareSegment
}) {
  const colored = segments.map((segment, index) => ({
    ...segment,
    color: dataColors[index] ?? 'var(--chart-secondary)',
  }))
  const tail = rest ? [{ ...rest, color: 'var(--border-strong)' }] : []
  const parts = [...colored, ...tail].filter(({ value }) => value > 0)
  if (!parts.length) return null
  return (
    <figure className="share-line" aria-label={label}>
      <div className="share-line-bar">
        {parts.map((part) => (
          <Tooltip key={part.key}>
            <TooltipTrigger
              render={
                <span
                  role="img"
                  className="share-line-part"
                  aria-label={`${part.label}: ${formatCurrency(part.value)}, ${formatShare(part.percent)}`}
                  style={{ flexGrow: part.value, color: part.color }}
                />
              }
            />
            <TooltipContent className="treemap-tooltip" sideOffset={8}>
              <div className="treemap-tooltip-heading">
                <strong>{part.label}</strong>
                <span>{formatShare(part.percent)}</span>
              </div>
              <dl className="treemap-tooltip-metrics">
                <div>
                  <dt>Value</dt>
                  <dd className="sensitive-value">{formatCurrency(part.value)}</dd>
                </div>
              </dl>
            </TooltipContent>
          </Tooltip>
        ))}
      </div>
    </figure>
  )
}
