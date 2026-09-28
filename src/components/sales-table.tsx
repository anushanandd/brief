import type { Ref } from 'react'

import { formatCurrency, formatPercent, formatShares, valueTone } from '../lib/format'
import type { Trade } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { SecurityLink } from './security-link'
import { EmptyState, SectionHeading } from './ui'

export function SalesTable({
  sales,
  externalLogosEnabled,
  referenceIso,
  scrollRef,
}: {
  sales: Trade[]
  externalLogosEnabled: boolean
  referenceIso: string
  scrollRef?: Ref<HTMLDivElement>
}) {
  if (!sales.length)
    return (
      <>
        <SectionHeading title="Realized sales" />
        <EmptyState>No imported sales.</EmptyState>
      </>
    )
  return (
    <div
      ref={scrollRef}
      className="financial-table-scroll financial-table-scroll-titled"
      role="region"
      aria-label="Realized sales"
      tabIndex={0}
      data-keyboard-region
    >
      <table className="financial-table sales-table">
        <thead>
          <tr>
            <th scope="col" aria-label="Security">
              <SectionHeading title="Realized sales" />
            </th>
            {['Date', 'Shares', 'Sale price', 'Proceeds', 'FIFO basis', 'Estimated P/L'].map(
              (label) => (
                <th key={label} scope="col">
                  {label}
                </th>
              ),
            )}
          </tr>
        </thead>
        <tbody>
          {sales.map((trade) => (
            <tr key={trade.id} data-keyboard-row tabIndex={-1}>
              <th scope="row">
                <SecurityLink
                  ticker={trade.ticker}
                  note={trade.description ?? 'Sell execution'}
                  externalLogosEnabled={externalLogosEnabled}
                />
              </th>
              <td>
                <time dateTime={trade.date}>{formatActivityDate(trade.date, referenceIso)}</time>
              </td>
              <td>{formatShares(trade.units)}</td>
              <td>{formatCurrency(trade.price)}</td>
              <td>{formatCurrency(Math.abs(trade.amount))}</td>
              <td>{formatCurrency(trade.realizedCostBasis)}</td>
              <td className={valueTone(trade.estimatedRealizedGain)}>
                <strong>{formatCurrency(trade.estimatedRealizedGain)}</strong>
                <small>
                  {trade.estimatedRealizedGain == null
                    ? 'Lot match unavailable'
                    : formatPercent(trade.estimatedRealizedGainPct)}
                </small>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
