import { useState } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { FilterSelect } from '../components/filter-select'
import { LedgerToolbar } from '../components/ledger-toolbar'
import { ActivityDetailHeader } from '../components/spending-detail-header'
import { TransactionList } from '../components/transaction-list'
import { Card } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
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
  const searchInputRef = useSearchShortcuts(search, setSearch)
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
        <LedgerToolbar
          label="Search transactions"
          placeholder="Search merchant, category or amount"
          value={search}
          onValueChange={setSearch}
          inputRef={searchInputRef}
        >
          <FilterSelect
            label="Transaction month"
            value={selectedMonth}
            options={[
              { value: '', label: 'All months' },
              ...months.map((value) => ({ value, label: monthLabel(value) })),
            ]}
            onValueChange={setMonth}
          />
        </LedgerToolbar>
        <p className="spending-detail-note" role="status">
          {filtered.length} of {transactions.length} transactions · All imported history
        </p>
        <TransactionList transactions={filtered} referenceIso={data.updatedAt} detailed />
      </Card>
    </div>
  )
}
