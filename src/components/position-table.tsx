import { isTauri, openExternalUrl } from '../lib/api'
import { formatCurrency, formatPercent, formatSecurityName } from '../lib/format'
import { stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { BrandMark } from './brand-mark'

export function PositionTable({
  positions,
  externalLogosEnabled,
  accountNames,
  emptyMessage = 'No positions in this account.',
}: {
  positions: FinanceSnapshot['holdings']
  externalLogosEnabled: boolean
  accountNames?: Record<string, string>
  emptyMessage?: string
}) {
  const showsAccount = accountNames != null
  return (
    <div className="account-positions-scroll">
      <table
        className={`account-positions-table${showsAccount ? ' account-holdings-ledger-table' : ''}`}
      >
        <thead>
          <tr>
            <th scope="col">Security</th>
            {showsAccount ? <th scope="col">Account</th> : null}
            <th scope="col">Shares</th>
            <th scope="col">Price</th>
            <th scope="col">Cost basis</th>
            <th scope="col">Value</th>
            <th scope="col">Unrealized P/L</th>
          </tr>
        </thead>
        <tbody>
          {positions.map((holding) => {
            const yahooUrl = `https://finance.yahoo.com/quote/${encodeURIComponent(holding.ticker)}/`
            const tone =
              holding.unrealizedGain == null
                ? 'muted'
                : holding.unrealizedGain > 0
                  ? 'positive'
                  : holding.unrealizedGain < 0
                    ? 'negative'
                    : 'muted'
            return (
              <tr key={`${holding.accountId}:${holding.ticker}`} data-keyboard-row tabIndex={-1}>
                <th scope="row">
                  <a
                    className="position-security"
                    data-keyboard-open
                    href={yahooUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(event) => {
                      if (!isTauri()) return
                      event.preventDefault()
                      void openExternalUrl(yahooUrl)
                    }}
                  >
                    <BrandMark
                      className="asset-mark"
                      fallback={stockMarkLabel(holding.ticker)}
                      label={`${holding.name} logo`}
                      src={stockLogoUrl(holding.ticker, externalLogosEnabled)}
                      style={{ backgroundColor: stockMarkColor(holding.ticker) }}
                    />
                    <span>
                      <strong>{holding.ticker}</strong>
                      <small>{holding.valuationNote ?? formatSecurityName(holding.name)}</small>
                    </span>
                  </a>
                </th>
                {showsAccount ? (
                  <td className="holding-account-name">{accountNames[holding.accountId] ?? '—'}</td>
                ) : null}
                <td>
                  {holding.shares?.toLocaleString('en-US', { maximumFractionDigits: 4 }) ?? '—'}
                </td>
                <td>{formatCurrency(holding.price)}</td>
                <td>{formatCurrency(holding.costBasis)}</td>
                <td>{formatCurrency(holding.value)}</td>
                <td className={tone}>
                  <strong>{formatCurrency(holding.unrealizedGain)}</strong>
                  <small>{formatPercent(holding.totalChangePct)}</small>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {!positions.length ? <p className="overview-recent-empty">{emptyMessage}</p> : null}
    </div>
  )
}
