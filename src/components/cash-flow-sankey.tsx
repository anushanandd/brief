import { validDate } from '../lib/dates'
import {
  countOf,
  formatActivityName,
  formatCurrency,
  formatShortDay,
  pluralize,
} from '../lib/format'
import type { CashFlow, CashFlowItem } from '../lib/schema'
import { EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

type FlowTone = 'positive' | 'negative' | 'neutral' | 'warning'

export type CashFlowSankeyValues = CashFlow

type Flow = CashFlowItem & { tone: FlowTone }
type PositionedFlow = Flow & { y: number; height: number; center: number }

/**
 * The chart's two sides as Rust grouped them: each lists its largest groups and one "Other",
 * so the chart stays a few rows tall, then fees and taxes, then the gap or what was left.
 */
export function cashFlowSankeyModel(values: CashFlowSankeyValues) {
  const sources: Flow[] = values.sources
    .filter(({ value }) => value > 0)
    .map((flow) => ({ ...flow, tone: 'positive' as const }))
  const uses: Flow[] = [...values.purchases, ...(values.charges ? [values.charges] : [])]
    .filter(({ value }) => value > 0)
    .map((flow) => ({ ...flow, tone: 'negative' as const }))
  // Rust found what came in, what went out, and the gap or remainder between them.
  const total = Math.max(values.inflow, values.outflow)
  if (values.gap > 0) {
    sources.push({
      id: 'gap',
      label: 'Funding gap',
      value: values.gap,
      children: [],
      tone: 'warning',
    })
  } else if (values.retained > 0) {
    uses.push({
      id: 'retained',
      label: 'Cash retained',
      value: values.retained,
      children: [],
      tone: 'neutral',
    })
  }
  return { sources, uses, total }
}

const chartTop = 50
const chartBottomPadding = 26
const gap = 28
const sourceX = 220
const poolX = 491
const useX = 762
const nodeWidth = 18

function positionFlows(
  flows: Flow[],
  total: number,
  flowHeight: number,
  chartHeight: number,
): PositionedFlow[] {
  const groupHeight = flowHeight + Math.max(0, flows.length - 1) * gap
  let y = (chartHeight - groupHeight) / 2
  return flows.map((flow) => {
    const height = (flow.value / total) * flowHeight
    const positioned = { ...flow, y, height, center: y + height / 2 }
    y += height + gap
    return positioned
  })
}

function bandPath(
  startX: number,
  startTop: number,
  startBottom: number,
  endX: number,
  endTop: number,
  endBottom: number,
) {
  const middle = (startX + endX) / 2
  return `M ${startX} ${startTop} C ${middle} ${startTop}, ${middle} ${endTop}, ${endX} ${endTop} L ${endX} ${endBottom} C ${middle} ${endBottom}, ${middle} ${startBottom}, ${startX} ${startBottom} Z`
}

const flowDate = (date: string) => (validDate(date) ? formatShortDay(date) : '')

export const boundedTransactionTitle = formatActivityName

/** What a band holds, read aloud: an "Other" its groups, any other its transactions. */
function transactionLabel(flow: Flow) {
  if (flow.parts?.length)
    return flow.parts.map((part) => `${part.label} ${formatCurrency(part.value)}`).join(', ')
  return flow.children
    .map((transaction) => {
      const date = flowDate(transaction.date)
      return `${transaction.label} ${formatCurrency(transaction.value)}${date ? ` on ${date}` : ''}`
    })
    .join(', ')
}

const shownRows = 8

/** A band's detail: an "Other" lists the groups it holds; any other, its transactions. */
function FlowTooltip({ flow }: { flow: Flow }) {
  const parts = flow.parts ?? []
  const rows = parts.length
    ? parts.map((part) => ({ id: part.label, label: part.label, value: part.value }))
    : flow.children.map((transaction) => ({
        id: transaction.id,
        label: `${boundedTransactionTitle(transaction.label)} · ${flowDate(transaction.date)}`,
        value: transaction.value,
      }))
  const details = rows.slice(0, shownRows)
  const remaining = rows.length - details.length
  return (
    <TooltipContent className="treemap-tooltip sankey-tooltip" sideOffset={8}>
      <div className="treemap-tooltip-heading">
        <strong>{boundedTransactionTitle(flow.label)}</strong>
        <span>{formatCurrency(flow.value)}</span>
      </div>
      <div className="treemap-tooltip-composition">
        <span>
          {parts.length
            ? `${countOf(parts.length, 'group')} · ${countOf(flow.children.length, 'transaction')}`
            : pluralize(flow.children.length, 'Transaction')}
        </span>
        <ul>
          {details.map((row) => (
            <li key={row.id}>
              <span>{row.label}</span>
              <strong>{formatCurrency(row.value)}</strong>
            </li>
          ))}
          {remaining > 0 ? <li className="muted">+{remaining} more</li> : null}
        </ul>
      </div>
    </TooltipContent>
  )
}

function FlowBand({ flow, path }: { flow: Flow; path: string }) {
  const className = `sankey-link ${flow.tone}`
  if (!flow.children.length) return <path className={className} d={path} />
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <path
            className={`${className} sankey-link-interactive`}
            d={path}
            tabIndex={0}
            aria-label={`${flow.label}, ${formatCurrency(flow.value)}: ${transactionLabel(flow)}`}
          />
        }
      />
      <FlowTooltip flow={flow} />
    </Tooltip>
  )
}

const labelSpacing = 38

/** Moves label centers apart by at least `spacing`, staying within `[min, max]`. */
export function spreadLabels(centers: number[], spacing: number, min: number, max: number) {
  const placed = centers
    .map((center, index) => ({ center, index }))
    .toSorted((a, b) => a.center - b.center)
  let floor = min
  for (const label of placed) {
    label.center = Math.max(label.center, floor)
    floor = label.center + spacing
  }
  let ceiling = max
  for (const label of placed.toReversed()) {
    label.center = Math.min(label.center, ceiling)
    ceiling = label.center - spacing
  }
  const result = Array<number>(centers.length)
  for (const { center, index } of placed) result[index] = center
  return result
}

function FlowLabel({
  flow,
  side,
  top,
}: {
  flow: PositionedFlow
  side: 'source' | 'use'
  top: number
}) {
  const label = (
    <>
      <strong>{boundedTransactionTitle(flow.label)}</strong>
      <small>{formatCurrency(flow.value)}</small>
    </>
  )
  if (!flow.children.length) {
    return (
      <span className={`sankey-label sankey-label-${side}`} style={{ top }}>
        {label}
      </span>
    )
  }
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            className={`sankey-label sankey-label-${side} sankey-label-interactive`}
            style={{ top }}
            aria-label={`${flow.label}, ${formatCurrency(flow.value)}: ${transactionLabel(flow)}`}
          />
        }
      >
        {label}
      </TooltipTrigger>
      <FlowTooltip flow={flow} />
    </Tooltip>
  )
}

function describeGroup(flow: Flow) {
  const count = flow.children.length
  return `${flow.label} ${formatCurrency(flow.value)}${count ? ` across ${countOf(count, 'transaction')}` : ''}`
}

export function CashFlowSankey({ values }: { values: CashFlowSankeyValues }) {
  const model = cashFlowSankeyModel(values)
  if (!model.total) {
    return <EmptyState>No cash flow in this period.</EmptyState>
  }

  const maxCount = Math.max(model.sources.length, model.uses.length)
  const chartHeight = Math.max(320, maxCount * 44 + 72)
  const flowHeight = chartHeight - chartTop - chartBottomPadding - Math.max(0, maxCount - 1) * gap
  const sources = positionFlows(model.sources, model.total, flowHeight, chartHeight)
  const uses = positionFlows(model.uses, model.total, flowHeight, chartHeight)
  const poolY = (chartHeight - flowHeight) / 2
  let incomingY = poolY
  let outgoingY = poolY
  const labelBounds = [labelSpacing / 2, chartHeight - labelSpacing / 2] as const
  const sourceLabels = spreadLabels(
    sources.map(({ center }) => center),
    labelSpacing,
    ...labelBounds,
  )
  const useLabels = spreadLabels(
    uses.map(({ center }) => center),
    labelSpacing,
    ...labelBounds,
  )
  const description = `Cash sources: ${model.sources.map(describeGroup).join(', ')}. Cash uses: ${model.uses.map(describeGroup).join(', ')}.`

  return (
    <div className="cash-flow-sankey-scroll">
      <div
        className="cash-flow-sankey"
        role="group"
        aria-label={description}
        style={{ height: chartHeight }}
      >
        <svg viewBox={`0 0 1000 ${chartHeight}`} preserveAspectRatio="none" aria-hidden="true">
          <g className="sankey-links">
            {sources.map((flow) => {
              const path = bandPath(
                sourceX + nodeWidth,
                flow.y,
                flow.y + flow.height,
                poolX,
                incomingY,
                incomingY + flow.height,
              )
              incomingY += flow.height
              return <FlowBand flow={flow} path={path} key={flow.id} />
            })}
            {uses.map((flow) => {
              const path = bandPath(
                poolX + nodeWidth,
                outgoingY,
                outgoingY + flow.height,
                useX,
                flow.y,
                flow.y + flow.height,
              )
              outgoingY += flow.height
              return <FlowBand flow={flow} path={path} key={flow.id} />
            })}
          </g>
          {sources.map((flow) => (
            <rect
              className={`sankey-node ${flow.tone}`}
              x={sourceX}
              y={flow.y}
              width={nodeWidth}
              height={flow.height}
              rx="3"
              key={flow.id}
            />
          ))}
          <rect
            className="sankey-node pool"
            x={poolX}
            y={poolY}
            width={nodeWidth}
            height={flowHeight}
            rx="3"
          />
          {uses.map((flow) => (
            <rect
              className={`sankey-node ${flow.tone}`}
              x={useX}
              y={flow.y}
              width={nodeWidth}
              height={flow.height}
              rx="3"
              key={flow.id}
            />
          ))}
        </svg>
        <span className="sankey-pool-label">
          <strong>{formatCurrency(model.total)}</strong>
        </span>
        {sources.map((flow, index) => (
          <FlowLabel
            flow={flow}
            side="source"
            top={sourceLabels[index] ?? flow.center}
            key={flow.id}
          />
        ))}
        {uses.map((flow, index) => (
          <FlowLabel flow={flow} side="use" top={useLabels[index] ?? flow.center} key={flow.id} />
        ))}
      </div>
    </div>
  )
}
