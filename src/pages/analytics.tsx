import NumberFlow from '@number-flow/react'
import { ArrowDownRight, ArrowUpRight, CircleDollarSign, Landmark, PiggyBank } from 'lucide-react'

import { AllocationChart, DividendChart, NetWorthChart } from '../components/charts'
import { PageError, PageLoading } from '../components/data-state'
import { Card, Change, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { formatCurrency } from '../lib/format'

export function AnalyticsPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const monthlyIncome = data.transactions
    .filter((transaction) => transaction.amount > 0)
    .reduce((total, transaction) => total + transaction.amount, 0)
  const monthlyOutflow = Math.abs(
    data.transactions
      .filter((transaction) => transaction.amount < 0)
      .reduce((total, transaction) => total + transaction.amount, 0),
  )
  const savingsRate = ((monthlyIncome - monthlyOutflow) / monthlyIncome) * 100

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Analytics</h1>
        </div>
        <div className="period-control" aria-label="Selected time range">
          <button type="button">1M</button>
          <button type="button" className="active">
            6M
          </button>
          <button type="button">1Y</button>
          <button type="button">All</button>
        </div>
      </header>

      <div className="metric-grid analytics-metrics">
        <Card className="metric-card icon-metric">
          <span className="metric-icon">
            <ArrowUpRight size={16} />
          </span>
          <p>Total return</p>
          <NumberFlow value={data.totalReturn} format={{ style: 'currency', currency: 'USD' }} />
          <Change value={data.totalReturnPct} label="all time" />
        </Card>
        <Card className="metric-card icon-metric">
          <span className="metric-icon">
            <CircleDollarSign size={16} />
          </span>
          <p>August income</p>
          <strong>{formatCurrency(monthlyIncome)}</strong>
        </Card>
        <Card className="metric-card icon-metric">
          <span className="metric-icon">
            <PiggyBank size={16} />
          </span>
          <p>Savings rate</p>
          <strong>{savingsRate.toFixed(1)}%</strong>
        </Card>
      </div>

      <Card className="analytics-chart-card">
        <SectionHeading title="Net worth progression" />
        <div className="chart-callout">
          <strong>{formatCurrency(data.netWorthChange)}</strong>
          <span>gained today</span>
        </div>
        <NetWorthChart data={data.netWorthHistory} />
      </Card>

      <div className="analytics-grid">
        <Card>
          <SectionHeading title="Asset mix" />
          <AllocationChart data={data.allocation} centerValue={data.investedAssets} />
        </Card>
        <Card>
          <SectionHeading title="Dividend income" />
          <div className="dividend-total">
            <strong>
              {formatCurrency(data.dividends.reduce((total, item) => total + item.value, 0))}
            </strong>
            <span>last six months</span>
          </div>
          <DividendChart data={data.dividends} />
        </Card>
      </div>

      <Card>
        <SectionHeading title="Accounts" />
        <div className="account-breakdown">
          {data.accounts.slice(1).map((account) => {
            const percent = Math.abs(account.value / data.netWorth) * 100
            return (
              <div className="account-row" key={account.id}>
                <span className="account-icon">
                  <Landmark size={16} />
                </span>
                <span className="account-name">
                  <strong>{account.name}</strong>
                  <small>{account.institution}</small>
                </span>
                <span className="account-bar">
                  <i style={{ width: `${Math.max(percent, 2)}%` }} />
                </span>
                <strong>{formatCurrency(account.value)}</strong>
                {account.value >= 0 ? (
                  <ArrowUpRight className="positive-icon" size={16} />
                ) : (
                  <ArrowDownRight className="negative-icon" size={16} />
                )}
              </div>
            )
          })}
        </div>
      </Card>
    </div>
  )
}
