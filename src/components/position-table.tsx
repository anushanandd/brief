import type { ReactNode } from 'react'

import {
  formatCurrency,
  formatPercent,
  formatSecurityName,
  formatShares,
  valueTone,
} from '../lib/format'
import type { FinanceSnapshot } from '../lib/schema'
import { SecurityLink } from './security-link'
import { Change, EmptyState, SectionHeading } from './ui'

export function PositionTable({
  title,
  positions,
  externalLogosEnabled,
  accountNames,
  marketChangeNote,
  marketLoading = false,
  view = 'positions',
  emptyMessage = 'No positions in this account.',
}: {
  title?: ReactNode
  positions: FinanceSnapshot['holdings']
  externalLogosEnabled: boolean
  accountNames?: Record<string, string>
  marketLoading?: boolean
  marketChangeNote?: string
  view?: 'positions' | 'market' | 'summary'
  emptyMessage?: string
}) {
  if (!positions.length)
    return (
      <>
        {title ? <SectionHeading title={title} /> : null}
        <EmptyState>{emptyMessage}</EmptyState>
      </>
    )
  const columns =
    view === 'market'
      ? ['Price', 'Today %', 'Week %', 'Total %', 'Value']
      : view === 'summary'
        ? ['Value', 'Unrealized P/L %']
        : ['Shares', 'Price', 'Cost basis', 'Value', 'Unrealized P/L']
  return (
    <div
      className={`financial-table-scroll${title ? ' financial-table-scroll-titled' : ''}`}
      role="region"
      aria-label="Holdings"
      tabIndex={0}
      data-keyboard-region
    >
      <table className={`financial-table position-table position-table-${view}`}>
        <thead>
          <tr>
            <th scope="col" aria-label="Security">
              {title ? <SectionHeading title={title} /> : 'Security'}
            </th>
            {accountNames ? <th scope="col">Account</th> : null}
            {columns.map((column) => (
              <th
                scope="col"
                key={column}
                aria-description={
                  column === 'Today %' || column === 'Week %' ? marketChangeNote : undefined
                }
              >
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {positions.map((holding) => (
            <tr key={`${holding.accountId}:${holding.ticker}`} data-keyboard-row tabIndex={-1}>
              <th scope="row">
                <SecurityLink
                  ticker={holding.ticker}
                  name={holding.name}
                  note={holding.valuationNote ?? formatSecurityName(holding.name)}
                  externalLogosEnabled={externalLogosEnabled}
                />
              </th>
              {accountNames ? (
                <td className="holding-account-name">{accountNames[holding.accountId] ?? '—'}</td>
              ) : null}
              {view === 'positions' ? (
                <>
                  <td>{formatShares(holding.shares)}</td>
                  <td className="public-market-value">{formatCurrency(holding.price)}</td>
                  <td>{formatCurrency(holding.costBasis)}</td>
                  <td>{formatCurrency(holding.value)}</td>
                  <td className={valueTone(holding.unrealizedGain)}>
                    <strong>{formatCurrency(holding.unrealizedGain)}</strong>
                    <small>{formatPercent(holding.totalChangePct)}</small>
                  </td>
                </>
              ) : view === 'market' ? (
                <>
                  <td className="public-market-value">{formatCurrency(holding.price)}</td>
                  <td className="public-market-value">
                    {marketLoading && holding.dailyChangePct == null ? (
                      <span className="muted" aria-label="Market change loading">
                        …
                      </span>
                    ) : (
                      <Change value={holding.dailyChangePct} />
                    )}
                  </td>
                  <td className="public-market-value">
                    {marketLoading && holding.weeklyChangePct == null ? (
                      <span className="muted" aria-label="Market change loading">
                        …
                      </span>
                    ) : (
                      <Change value={holding.weeklyChangePct} />
                    )}
                  </td>
                  <td>
                    <Change value={holding.totalChangePct} />
                  </td>
                  <td>{formatCurrency(holding.value)}</td>
                </>
              ) : (
                <>
                  <td>{formatCurrency(holding.value)}</td>
                  <td>
                    <Change value={holding.totalChangePct} />
                  </td>
                </>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
