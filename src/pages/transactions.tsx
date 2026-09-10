import { Search, X } from 'lucide-react'
import { useState } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { ActivityDetailHeader } from '../components/spending-detail-header'
import { TransactionList } from '../components/transaction-list'
import { Button, Card } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import {
  resolveSpendingAccount,
  sortTransactionsByRecency,
  transactionDateKey,
} from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

const monthLabel = (month: string) =>
  new Intl.DateTimeFormat('en-US', {
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  }).format(new Date(`${month}-01T00:00:00Z`))

export function TransactionsPage() {
  const query = useFinance()
  const [search, setSearch] = useState('')
  const [month, setMonth] = useState('')
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const account = resolveSpendingAccount(data.accounts, getSpendingAccountId())
  const accountName = account
    ? accountDisplayName(account.id, account.name, getAccountDisplayNames())
    : undefined
  const transactions = sortTransactionsByRecency(
    account ? data.transactions.filter(({ accountId }) => accountId === account.id) : [],
    data.updatedAt,
  )
  const months = [
    ...new Set(
      transactions
        .map(({ date }) => transactionDateKey(date, data.updatedAt).slice(0, 7))
        .filter(Boolean),
    ),
  ].toSorted((left, right) => right.localeCompare(left))
  const selectedMonth = months.includes(month) ? month : ''
  const queryText = search.trim().toLocaleLowerCase()
  const filtered = transactions.filter(
    (transaction) =>
      (!selectedMonth ||
        transactionDateKey(transaction.date, data.updatedAt).startsWith(selectedMonth)) &&
      `${transaction.merchant} ${transaction.description ?? ''} ${transaction.category} ${transaction.amount}`
        .toLocaleLowerCase()
        .includes(queryText),
  )

  return (
    <div className="page spending-detail-page">
      <ActivityDetailHeader title="Transactions" accountName={accountName} />
      <Card className="spending-detail-card">
        <div className="spending-history-toolbar">
          <label className="spending-history-search">
            <Search size={16} aria-hidden="true" />
            <span className="sr-only">Search transactions</span>
            <input
              type="search"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search merchant, category or amount"
            />
            {search ? (
              <Button
                size="icon"
                variant="ghost"
                onClick={() => setSearch('')}
                aria-label="Clear search"
              >
                <X size={14} aria-hidden="true" />
              </Button>
            ) : null}
          </label>
          <label className="spending-history-filter">
            <span className="sr-only">Transaction month</span>
            <select value={selectedMonth} onChange={(event) => setMonth(event.target.value)}>
              <option value="">All months</option>
              {months.map((value) => (
                <option key={value} value={value}>
                  {monthLabel(value)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <p className="spending-detail-note" role="status">
          {filtered.length} of {transactions.length} transactions · All imported history
        </p>
        <TransactionList transactions={filtered} referenceIso={data.updatedAt} detailed />
      </Card>
    </div>
  )
}
