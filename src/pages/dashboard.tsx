import NumberFlow from '@number-flow/react'
import { Link } from '@tanstack/react-router'
import { ArrowRight, ChevronDown } from 'lucide-react'

import { AllocationChart, NetWorthChart } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, Change, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { formatCurrency, formatUpdatedAt } from '../lib/format'
import { useUiStore } from '../store/ui'

export function DashboardPage() {
  const query = useFinance()
  const activeAccountId = useUiStore((state) => state.activeAccountId)
  const setActiveAccountId = useUiStore((state) => state.setActiveAccountId)

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const account = data.accounts.find((item) => item.id === activeAccountId) ?? data.accounts[0]
  const visibleHoldings =
    activeAccountId === 'all'
      ? data.holdings
      : data.holdings.filter((holding) => holding.accountId === activeAccountId)
  const displayedValue = activeAccountId === 'all' ? data.netWorth : account.value

  return (
    <div className="page dashboard-page">
      <header className="page-header">
        <div>
          <p className="eyebrow">Monday, August 31</p>
          <h1>Good afternoon.</h1>
        </div>
        <div className="header-actions">
          <span className="freshness">
            <StatusDot /> Updated {formatUpdatedAt(data.updatedAt)}
          </span>
          <RefreshButton />
        </div>
      </header>

      <div className="account-picker-wrap">
        <label htmlFor="account-picker">Viewing</label>
        <div className="select-wrap">
          <select
            id="account-picker"
            value={activeAccountId}
            onChange={(event) => setActiveAccountId(event.target.value)}
          >
            {data.accounts.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name} · {item.institution}
              </option>
            ))}
          </select>
          <ChevronDown size={14} aria-hidden="true" />
        </div>
      </div>

      <section className="hero-balance" aria-labelledby="net-worth-title">
        <p id="net-worth-title">{activeAccountId === 'all' ? 'Total net worth' : account.name}</p>
        <NumberFlow
          value={displayedValue}
          format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
          className="hero-number"
        />
        <div className="hero-change">
          <Change value={data.netWorthChangePct} />
          <span>{formatCurrency(data.netWorthChange)} today</span>
        </div>
      </section>

      <Card className="net-worth-card">
        <SectionHeading
          title="Net worth"
          action={
            <Link to="/analytics" className="text-link">
              Explore <ArrowRight size={14} />
            </Link>
          }
        />
        <NetWorthChart data={data.netWorthHistory} />
      </Card>

      <div className="metric-grid">
        <Card className="metric-card">
          <p>Invested assets</p>
          <strong>{formatCurrency(data.investedAssets)}</strong>
          <Change value={data.totalReturnPct} label="all time" />
        </Card>
        <Card className="metric-card">
          <p>Cash</p>
          <strong>{formatCurrency(data.cash)}</strong>
          <span className="muted-copy">17.2% of net worth</span>
        </Card>
        <Card className="metric-card">
          <p>Debt</p>
          <strong>{formatCurrency(data.debt)}</strong>
          <span className="muted-copy">Statement due Sep 18</span>
        </Card>
      </div>

      <div className="dashboard-grid">
        <Card>
          <SectionHeading title="Allocation" />
          <AllocationChart data={data.allocation} centerValue={data.investedAssets} />
        </Card>

        <Card className="movers-card">
          <SectionHeading
            title="Today’s movers"
            action={
              <Link to="/holdings" className="text-link">
                All holdings <ArrowRight size={14} />
              </Link>
            }
          />
          <div className="mover-list">
            {visibleHoldings
              .toSorted((a, b) => Math.abs(b.dailyChangePct) - Math.abs(a.dailyChangePct))
              .slice(0, 4)
              .map((holding) => (
                <div className="mover-row" key={holding.ticker}>
                  <span className="asset-mark" style={{ backgroundColor: holding.color }}>
                    {holding.ticker.slice(0, 1)}
                  </span>
                  <span className="mover-name">
                    <strong>{holding.ticker}</strong>
                    <small>{holding.name}</small>
                  </span>
                  <span className="mover-value">
                    <strong>{formatCurrency(holding.value, { maximumFractionDigits: 0 })}</strong>
                    <Change value={holding.dailyChangePct} />
                  </span>
                </div>
              ))}
          </div>
        </Card>
      </div>
    </div>
  )
}
