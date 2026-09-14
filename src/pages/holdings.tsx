import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { FilterSelect } from '../components/filter-select'
import { LedgerToolbar } from '../components/ledger-toolbar'
import { PositionTable } from '../components/position-table'
import { Card } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { formatSecurityName } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'

const categoryLabel = (value: string) =>
  value.replaceAll(/[_-]+/g, ' ').replace(/^./, (character) => character.toLocaleUpperCase())

export function HoldingsPage() {
  const query = useFinance()
  const routeSearch = useSearch({ from: '/holdings' })
  const navigate = useNavigate({ from: '/holdings' })
  const [search, setSearch] = useState('')
  const searchInputRef = useSearchShortcuts(search, setSearch)

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const displayNames = getAccountDisplayNames()
  const accounts = data.accounts.filter(({ id }) => id !== 'all')
  const accountNames = Object.fromEntries(
    accounts.map((account) => [
      account.id,
      accountDisplayName(account.id, account.name, displayNames),
    ]),
  )
  const accountOptions = [
    { value: '', label: 'All accounts' },
    ...accounts.map((account) => ({ value: account.id, label: accountNames[account.id] })),
  ]
  const categories = [
    ...new Set(data.holdings.map(({ instrumentKind }) => instrumentKind || 'other')),
  ].toSorted((left, right) => categoryLabel(left).localeCompare(categoryLabel(right)))
  const categoryOptions = [
    { value: '', label: 'All categories' },
    ...categories.map((category) => ({ value: category, label: categoryLabel(category) })),
  ]
  const requestedAccount = routeSearch.account ?? ''
  const requestedCategory = routeSearch.category ?? ''
  const selectedAccount = accounts.some(({ id }) => id === requestedAccount) ? requestedAccount : ''
  const selectedCategory = categories.includes(requestedCategory) ? requestedCategory : ''
  const queryText = search.trim().toLocaleLowerCase()
  const holdings = data.holdings
    .filter(
      (holding) =>
        (!selectedAccount || holding.accountId === selectedAccount) &&
        (!selectedCategory || (holding.instrumentKind || 'other') === selectedCategory) &&
        `${holding.ticker} ${formatSecurityName(holding.name)} ${accountNames[holding.accountId] ?? ''} ${holding.instrumentKind ?? ''} ${holding.value ?? ''}`
          .toLocaleLowerCase()
          .includes(queryText),
    )
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))

  const updateFilters = (account: string, category: string) =>
    void navigate({
      search: { account: account || undefined, category: category || undefined },
      replace: true,
    })

  return (
    <div className="page spending-detail-page ledger-page">
      <WorkspaceHeader title="Holdings" />
      <Card className="spending-detail-card ledger-card">
        <LedgerToolbar
          label="Search holdings"
          placeholder="Search security, account or value"
          value={search}
          onValueChange={setSearch}
          inputRef={searchInputRef}
        >
          <FilterSelect
            label="Holding account"
            value={selectedAccount}
            options={accountOptions}
            onValueChange={(value) => updateFilters(value, selectedCategory)}
          />
          <FilterSelect
            label="Holding category"
            value={selectedCategory}
            options={categoryOptions}
            onValueChange={(value) => updateFilters(selectedAccount, value)}
          />
        </LedgerToolbar>
        <PositionTable
          positions={holdings}
          externalLogosEnabled={getExternalLogosEnabled()}
          accountNames={accountNames}
          emptyMessage="No holdings match these filters."
        />
      </Card>
    </div>
  )
}
