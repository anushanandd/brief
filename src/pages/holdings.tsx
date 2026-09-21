import { Link, useNavigate, useSearch } from '@tanstack/react-router'

import { BrandMark } from '../components/brand-mark'
import { PageError, PageLoading } from '../components/data-state'
import { HoldingChart } from '../components/holding-chart'
import { HoldingAccounts, HoldingActivity, HoldingPortfolio } from '../components/holding-details'
import { HoldingNews } from '../components/holding-news'
import { PositionTable } from '../components/position-table'
import { Card } from '../components/ui'
import { MarketStatus, WorkspaceHeader } from '../components/workspace-header'
import { useLiveFinance } from '../hooks/use-live-finance'
import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import { holdingDetail } from '../lib/holding-detail'
import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'

export function HoldingsPage() {
  const navigate = useNavigate({ from: '/holdings' })
  const routeSearch = useSearch({ from: '/holdings' })
  const query = useLiveFinance()

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const externalLogos = getExternalLogosEnabled()
  const securities = [...new Set(data.holdings.map(({ ticker }) => ticker))]
    .map((ticker) => ({ ticker, ...holdingDetail(data.holdings, ticker) }))
    .toSorted((left, right) => {
      if (left.value == null) return right.value == null ? 0 : 1
      if (right.value == null) return -1
      return right.value - left.value
    })
  const holdings = securities.flatMap(({ positions }) => positions)
  const requestedTicker = routeSearch.ticker
  const ticker =
    requestedTicker && holdings.some((holding) => holding.ticker === requestedTicker)
      ? requestedTicker
      : (holdings[0]?.ticker ?? '')

  return (
    <div className="page holdings-page">
      <WorkspaceHeader
        title="Holdings"
        status={<MarketStatus />}
        breadcrumbs={
          ticker
            ? [
                { label: 'Holdings', to: '/holdings', search: {} },
                { label: ticker, to: '/holdings', search: { ticker } },
              ]
            : undefined
        }
      />
      {securities.length ? (
        <nav className="account-switcher" aria-label="Held securities">
          <div className="account-switcher-grid">
            {securities.map((security) => (
              <Link
                className="account-switcher-button"
                activeOptions={{ exact: true }}
                data-keyboard-row
                data-keyboard-open
                aria-current={security.ticker === ticker ? 'page' : undefined}
                key={security.ticker}
                to="/holdings"
                search={{ ticker: security.ticker }}
              >
                <BrandMark
                  className="asset-mark"
                  fallback={stockMarkLabel(security.ticker)}
                  label={`${security.positions[0].name} logo`}
                  src={stockLogoUrl(security.ticker, externalLogos)}
                  style={{ backgroundColor: stockMarkColor(security.ticker) }}
                />
                <span className="account-switcher-copy">
                  <span>{security.ticker}</span>
                  <strong className="account-switcher-value">
                    <span>{formatCurrency(security.value)}</span>
                    <span
                      className={valueTone(security.dailyChange)}
                      aria-label={
                        security.dailyChange == null
                          ? 'Daily change unavailable'
                          : `${formatPercent(security.dailyChange)} daily change`
                      }
                    >
                      {formatPercent(security.dailyChange)}
                    </span>
                  </strong>
                </span>
              </Link>
            ))}
          </div>
        </nav>
      ) : null}
      <div className="holdings-overview-grid">
        <HoldingChart
          holdings={holdings}
          ticker={ticker}
          onTickerChange={(nextTicker) =>
            void navigate({ search: { ticker: nextTicker }, replace: true })
          }
        />
        <HoldingPortfolio data={query.data} ticker={ticker} />
      </div>
      <div className="holdings-detail-grid">
        <div className="holdings-account-activity-column">
          <HoldingAccounts data={query.data} ticker={ticker} />
          <HoldingActivity data={query.data} ticker={ticker} />
        </div>
        <Card className="brokerage-holdings-card holdings-total-card">
          <PositionTable
            title="Holdings"
            positions={data.holdings.toSorted(
              (left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity),
            )}
            externalLogosEnabled={externalLogos}
            view="market"
            emptyMessage="No holdings yet."
          />
        </Card>
        <HoldingNews ticker={ticker} connected={query.integrationStatus.alphaVantage} />
      </div>
    </div>
  )
}
