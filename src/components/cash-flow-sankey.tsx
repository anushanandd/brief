import { formatCurrency, formatActivityName } from '../lib/format'
import type { CashFlowBreakdownItem } from '../lib/money'
import { EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

type FlowTone = 'positive' | 'negative' | 'neutral' | 'warning'

export type CashFlowSankeyValues = {
  sources: CashFlowBreakdownItem[]
  purchases: CashFlowBreakdownItem[]
  fees: number
  taxes: number
}

type Flow = CashFlowBreakdownItem & { tone: FlowTone }
type PositionedFlow = Flow & { y: number; height: number; center: number }

export function cashFlowSankeyModel(values: CashFlowSankeyValues) {
  const sources: Flow[] = values.sources
    .filter(({ value }) => value > 0)
    .map((flow) => ({ ...flow, tone: 'positive' as const }))
  const uses: Flow[] = [
    ...values.purchases
      .filter(({ value }) => value > 0)
      .map((flow) => ({ ...flow, tone: 'negative' as const })),
    {
      id: 'fees',
      label: 'Fees',
      value: values.fees,
      children: [],
      tone: 'negative' as const,
    },
    {
      id: 'taxes',
      label: 'Taxes',
      value: values.taxes,
      children: [],
      tone: 'negative' as const,
    },
  ].filter(({ value }) => value > 0)
  const inflow = sources.reduce((total, flow) => total + flow.value, 0)
  const outflow = uses.reduce((total, flow) => total + flow.value, 0)
  const total = Math.max(inflow, outflow)
  if (outflow > inflow) {
    sources.push({
      id: 'gap',
      label: 'Funding gap',
      value: outflow - inflow,
      children: [],
      tone: 'warning',
    })
  } else if (inflow > outflow) {
    uses.push({
      id: 'retained',
      label: 'Cash retained',
      value: inflow - outflow,
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

const transactionDate = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})

export const boundedTransactionTitle = formatActivityName

function transactionLabel(flow: Flow) {
  return flow.children
    .map(
      (transaction) =>
        `${transaction.label} ${formatCurrency(transaction.value)} on ${transactionDate.format(new Date(`${transaction.date}T00:00:00Z`))}`,
    )
    .join(', ')
}

function FlowTooltip({ flow }: { flow: Flow }) {
  const details = flow.children.slice(0, 8)
  const remaining = flow.children.length - details.length
  return (
    <TooltipContent className="treemap-tooltip sankey-tooltip" sideOffset={8}>
      <div className="treemap-tooltip-heading">
        <strong>{boundedTransactionTitle(flow.label)}</strong>
        <span>{formatCurrency(flow.value)}</span>
      </div>
      <div className="treemap-tooltip-composition">
        <span>{flow.children.length === 1 ? 'Transaction' : 'Transactions'}</span>
        <ul>
          {details.map((transaction) => (
            <li key={transaction.id}>
              <span>
                {boundedTransactionTitle(transaction.label)} ·{' '}
                {transactionDate.format(new Date(`${transaction.date}T00:00:00Z`))}
              </span>
              <strong>{formatCurrency(transaction.value)}</strong>
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

function FlowLabel({ flow, side }: { flow: PositionedFlow; side: 'source' | 'use' }) {
  const label = (
    <>
      <strong>{boundedTransactionTitle(flow.label)}</strong>
      <small>{formatCurrency(flow.value)}</small>
    </>
  )
  if (!flow.children.length) {
    return (
      <span className={`sankey-label sankey-label-${side}`} style={{ top: flow.center }}>
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
            style={{ top: flow.center }}
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
  return `${flow.label} ${formatCurrency(flow.value)}${count ? ` across ${count} ${count === 1 ? 'transaction' : 'transactions'}` : ''}`
}

export function CashFlowSankey({ values }: { values: CashFlowSankeyValues }) {
  const model = cashFlowSankeyModel(values)
  if (!model.total) {
    return <EmptyState>No classified cash flow is available for this period.</EmptyState>
  }

  const maxCount = Math.max(model.sources.length, model.uses.length)
  const chartHeight = Math.max(320, maxCount * 44 + 72)
  const flowHeight = chartHeight - chartTop - chartBottomPadding - Math.max(0, maxCount - 1) * gap
  const sources = positionFlows(model.sources, model.total, flowHeight, chartHeight)
  const uses = positionFlows(model.uses, model.total, flowHeight, chartHeight)
  const poolY = (chartHeight - flowHeight) / 2
  let incomingY = poolY
  let outgoingY = poolY
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
          <strong>Cash available</strong>
          <small>{formatCurrency(model.total)}</small>
        </span>
        {sources.map((flow) => (
          <FlowLabel flow={flow} side="source" key={flow.id} />
        ))}
        {uses.map((flow) => (
          <FlowLabel flow={flow} side="use" key={flow.id} />
        ))}
      </div>
    </div>
  )
}
