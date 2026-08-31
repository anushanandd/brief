import { Search, Sparkles } from 'lucide-react'
import { useMemo, useState } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { HoldingsTable } from '../components/holdings-table'
import { Card, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { formatCurrency } from '../lib/format'

export function HoldingsPage() {
  const query = useFinance()
  const [search, setSearch] = useState('')
  const [accountId, setAccountId] = useState('all')

  const filteredHoldings = useMemo(() => {
    if (!query.data) return []
    const normalized = search.trim().toLowerCase()
    return query.data.holdings.filter((holding) => {
      const matchesAccount = accountId === 'all' || holding.accountId === accountId
      const matchesSearch =
        !normalized || `${holding.ticker} ${holding.name}`.toLowerCase().includes(normalized)
      return matchesAccount && matchesSearch
    })
  }, [accountId, query.data, search])

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const selectedValue = filteredHoldings.reduce((total, holding) => total + holding.value, 0)
  const weightedDaily = selectedValue
    ? filteredHoldings.reduce(
        (total, holding) => total + holding.dailyChangePct * holding.value,
        0,
      ) / selectedValue
    : 0
  const leadHolding = filteredHoldings.toSorted((a, b) => b.dailyChangePct - a.dailyChangePct)[0]

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Holdings</h1>
        </div>
        <div className="holdings-summary">
          <span>Market value</span>
          <strong>{formatCurrency(selectedValue)}</strong>
          <small className={weightedDaily >= 0 ? 'positive' : 'negative'}>
            {weightedDaily >= 0 ? '+' : ''}
            {weightedDaily.toFixed(2)}% today
          </small>
        </div>
      </header>

      <Card className="table-card">
        <div className="table-toolbar">
          <div className="search-field">
            <Search size={15} aria-hidden="true" />
            <input
              data-search
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search holdings"
              aria-label="Search holdings"
            />
          </div>
          <select
            value={accountId}
            onChange={(event) => setAccountId(event.target.value)}
            aria-label="Filter by account"
          >
            <option value="all">All investment accounts</option>
            {data.accounts
              .filter((account) => ['brokerage', 'retirement'].includes(account.type))
              .map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
          </select>
        </div>
        <HoldingsTable data={filteredHoldings} />
        <div className="table-footer">{filteredHoldings.length} positions</div>
      </Card>

      {leadHolding ? (
        <Card className="market-brief">
          <span className="brief-icon">
            <Sparkles size={17} />
          </span>
          <div>
            <SectionHeading title={`${leadHolding.ticker} leads today`} />
            <p>{leadHolding.summary}</p>
          </div>
          <div className="after-hours-price">
            <span>After hours</span>
            <strong>{formatCurrency(leadHolding.afterHoursPrice)}</strong>
          </div>
        </Card>
      ) : null}
    </div>
  )
}
