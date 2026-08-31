import NumberFlow from '@number-flow/react'
import { CalendarClock, Check, CircleDollarSign, CreditCard, Search } from 'lucide-react'
import { useState } from 'react'
import { Virtuoso } from 'react-virtuoso'

import { PageError, PageLoading } from '../components/data-state'
import { Card, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { formatCurrency } from '../lib/format'
import type { Transaction } from '../lib/schema'

function TransactionRow({ transaction }: { transaction: Transaction }) {
  return (
    <div className="transaction-row">
      <span className={`transaction-mark category-${transaction.category.toLowerCase()}`}>
        {transaction.merchant.slice(0, 1)}
      </span>
      <span className="transaction-name">
        <strong>{transaction.merchant}</strong>
        <small>
          {transaction.category} · {transaction.account}
        </small>
      </span>
      <span className="transaction-meta">
        <strong className={transaction.amount > 0 ? 'positive' : ''}>
          {formatCurrency(transaction.amount)}
        </strong>
        <small>
          {transaction.pending ? 'Pending · ' : ''}
          {transaction.date}
        </small>
      </span>
    </div>
  )
}

export function SpendingPage() {
  const query = useFinance()
  const [search, setSearch] = useState('')
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const normalizedSearch = search.trim().toLowerCase()
  const transactions = data.transactions.filter((transaction) =>
    `${transaction.merchant} ${transaction.category} ${transaction.account}`
      .toLowerCase()
      .includes(normalizedSearch),
  )

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Spending</h1>
        </div>
      </header>

      <div className="spending-hero-grid">
        <Card className="spending-total-card">
          <span className="metric-icon">
            <CircleDollarSign size={17} />
          </span>
          <p>Spent this month</p>
          <NumberFlow
            value={data.spending.monthTotal}
            format={{ style: 'currency', currency: 'USD' }}
          />
          <span className="positive">
            {Math.abs(data.spending.monthChangePct).toFixed(1)}% less than July
          </span>
        </Card>
        <Card className="statement-card">
          <span className="metric-icon">
            <CreditCard size={17} />
          </span>
          <div>
            <p>Amex statement</p>
            <strong>{formatCurrency(data.spending.statementBalance)}</strong>
          </div>
          <div className="statement-due">
            <CalendarClock size={15} />
            <span>Due {data.spending.statementDueDate}</span>
          </div>
          <button
            type="button"
            onClick={() => window.alert('Payment workflow is intentionally read-only in this MVP.')}
          >
            Review payment
          </button>
        </Card>
      </div>

      <div className="spending-grid">
        <Card>
          <SectionHeading title="By category" />
          <div className="category-stacked-bar" aria-label="Spending category distribution">
            {data.spending.categories.map((category) => (
              <span
                key={category.name}
                style={{ width: `${category.percent}%`, backgroundColor: category.color }}
              />
            ))}
          </div>
          <div className="category-list">
            {data.spending.categories.map((category) => (
              <div key={category.name}>
                <span className="legend-swatch" style={{ backgroundColor: category.color }} />
                <span>
                  <strong>{category.name}</strong>
                  <small>{category.percent.toFixed(1)}%</small>
                </span>
                <strong>{formatCurrency(category.value)}</strong>
              </div>
            ))}
          </div>
        </Card>

        <Card>
          <SectionHeading title="Card credits" />
          <div className="credit-list">
            {data.credits.map((credit) => {
              const percent = (credit.used / credit.total) * 100
              return (
                <div className="credit-row" key={credit.id}>
                  <div className="credit-title">
                    <strong>{credit.name}</strong>
                    <span>
                      {credit.status === 'used' ? (
                        <>
                          <Check size={12} /> Used
                        </>
                      ) : (
                        credit.deadline
                      )}
                    </span>
                  </div>
                  <div className="credit-progress">
                    <i style={{ width: `${percent}%` }} />
                  </div>
                  <small>
                    {formatCurrency(credit.used)} of {formatCurrency(credit.total)}
                  </small>
                </div>
              )
            })}
          </div>
        </Card>
      </div>

      <Card className="transactions-card">
        <SectionHeading
          title="Recent activity"
          action={
            <div className="search-field transaction-search">
              <Search size={15} aria-hidden="true" />
              <input
                data-search
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search activity"
                aria-label="Search activity"
              />
            </div>
          }
        />
        <Virtuoso
          className="transaction-virtuoso"
          style={{ height: 494 }}
          data={transactions}
          itemContent={(_, transaction) => <TransactionRow transaction={transaction} />}
        />
      </Card>
    </div>
  )
}
