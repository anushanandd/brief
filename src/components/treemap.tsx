import { useId, type MouseEvent } from 'react'

import { formatCurrency, formatPercent } from '../lib/format'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

export type TreemapSegment = {
  name: string
  value: number
  color: string
  changePct?: number | null
  href: string
  onSelect: () => void
  details?: Array<{ name: string; value: number }>
}

type Tile = TreemapSegment & { x: number; y: number; width: number; height: number }

function partition(
  items: TreemapSegment[],
  x: number,
  y: number,
  width: number,
  height: number,
): Tile[] {
  if (!items.length) return []
  if (items.length === 1) return [{ ...items[0], x, y, width, height }]
  const total = items.reduce((sum, { value }) => sum + value, 0)
  let first = items[0].value
  let split = 1
  while (
    split < items.length - 1 &&
    Math.abs(first + items[split].value - total / 2) < Math.abs(first - total / 2)
  )
    first += items[split++].value
  const ratio = first / total
  return width * 1.5 >= height
    ? [
        ...partition(items.slice(0, split), x, y, width * ratio, height),
        ...partition(items.slice(split), x + width * ratio, y, width * (1 - ratio), height),
      ]
    : [
        ...partition(items.slice(0, split), x, y, width, height * ratio),
        ...partition(items.slice(split), x, y + height * ratio, width, height * (1 - ratio)),
      ]
}

// Balanced binary subdivision preserves area without adding a chart dependency.
export function treemapTiles(segments: TreemapSegment[]): Tile[] {
  const items = segments
    .filter(({ value }) => Number.isFinite(value) && value > 0)
    .toSorted((a, b) => b.value - a.value || a.name.localeCompare(b.name))
  return partition(items, 0, 0, 100, 100)
}

export function Treemap({ segments, label }: { segments: TreemapSegment[]; label: string }) {
  const tooltipGroupId = useId()
  const tiles = treemapTiles(segments)
  const total = tiles.reduce((sum, tile) => sum + tile.value, 0)
  return (
    <div className="treemap">
      <div className="treemap-plot" role="group" aria-label={`${label}, sized by value`}>
        {tiles.map((tile, index) => {
          const allocation = ((tile.value / total) * 100).toFixed(1)
          const details = tile.details?.slice(0, 5) ?? []
          const remainingDetails = (tile.details?.length ?? 0) - details.length
          const detailLabel = tile.details?.length
            ? `, composition: ${tile.details.map((detail) => `${detail.name} ${formatCurrency(detail.value)}`).join(', ')}`
            : ''
          const tileId = `${tooltipGroupId}-tile-${index}`
          const titleId = `${tooltipGroupId}-title-${index}`
          return (
            <Tooltip key={`${tile.name}:${index}`}>
              <TooltipTrigger
                render={
                  <a
                    id={tileId}
                    className="treemap-tile"
                    aria-label={`${tile.name}: ${formatCurrency(tile.value)}, ${allocation}%${tile.changePct == null ? '' : `, one week ${formatPercent(tile.changePct)}`}${detailLabel}`}
                    href={tile.href}
                    onClick={(event: MouseEvent<HTMLAnchorElement>) => {
                      if (
                        event.button ||
                        event.metaKey ||
                        event.ctrlKey ||
                        event.shiftKey ||
                        event.altKey
                      )
                        return
                      event.preventDefault()
                      tile.onSelect()
                    }}
                    style={{
                      left: `${tile.x}%`,
                      top: `${tile.y}%`,
                      width: `${tile.width}%`,
                      height: `${tile.height}%`,
                      backgroundColor: tile.color,
                    }}
                  />
                }
              >
                {tile.width >= 10 && tile.height >= 10 ? (
                  <span className="treemap-tile-content" aria-hidden="true">
                    <span className="treemap-label">
                      <strong id={titleId}>{tile.name}</strong>
                      {tile.width >= 18 && tile.height >= 16 ? (
                        <small>{formatCurrency(tile.value)}</small>
                      ) : null}
                    </span>
                  </span>
                ) : null}
              </TooltipTrigger>
              <TooltipContent
                className="treemap-tooltip"
                anchor={() => document.getElementById(titleId) ?? document.getElementById(tileId)}
                side="top"
                sideOffset={8}
              >
                <div className="treemap-tooltip-heading">
                  <strong>{tile.name}</strong>
                  <span>{formatCurrency(tile.value)}</span>
                </div>
                <dl className="treemap-tooltip-metrics">
                  <div>
                    <dt>Allocation</dt>
                    <dd>{allocation}%</dd>
                  </div>
                  <div>
                    <dt>One week</dt>
                    <dd
                      className={
                        tile.changePct == null
                          ? 'muted'
                          : tile.changePct > 0
                            ? 'positive'
                            : tile.changePct < 0
                              ? 'negative'
                              : undefined
                      }
                    >
                      {formatPercent(tile.changePct)}
                    </dd>
                  </div>
                </dl>
                {details.length ? (
                  <div className="treemap-tooltip-composition">
                    <span>Composition</span>
                    <ul>
                      {details.map((detail) => (
                        <li key={detail.name}>
                          <span>{detail.name}</span>
                          <strong>{formatCurrency(detail.value)}</strong>
                        </li>
                      ))}
                      {remainingDetails > 0 ? (
                        <li className="muted">+{remainingDetails} more</li>
                      ) : null}
                    </ul>
                  </div>
                ) : null}
              </TooltipContent>
            </Tooltip>
          )
        })}
      </div>
    </div>
  )
}
