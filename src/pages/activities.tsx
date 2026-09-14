import { useNavigate, useSearch } from '@tanstack/react-router'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { PageError, PageLoading } from '../components/data-state'
import { FilterSelect } from '../components/filter-select'
import { LedgerToolbar } from '../components/ledger-toolbar'
import { Button, Card } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities, type ActivityItem } from '../lib/activity'
import { getExternalLogosEnabled } from '../lib/logos'

export const activityPageSize = (height: number) => Math.max(1, Math.floor(height / 62))

function PaginatedActivityList({
  activities,
  referenceIso,
}: {
  activities: ActivityItem[]
  referenceIso: string
}) {
  const listRef = useRef<HTMLDivElement>(null)
  const [page, setPage] = useState(0)
  const [pageSize, setPageSize] = useState(() => Math.max(1, activities.length))
  const pageCount = Math.max(1, Math.ceil(activities.length / pageSize))
  const currentPage = Math.min(page, pageCount - 1)

  useEffect(() => {
    const list = listRef.current
    if (!list) return undefined
    const update = () => setPageSize(activityPageSize(list.clientHeight))
    update()
    const observer = new ResizeObserver(update)
    observer.observe(list)
    return () => observer.disconnect()
  }, [])

  useEffect(() => setPage((value) => Math.min(value, pageCount - 1)), [pageCount])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      if (
        pageCount <= 1 ||
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }
      event.preventDefault()
      setPage((value) =>
        event.key === 'ArrowLeft' ? Math.max(0, value - 1) : Math.min(pageCount - 1, value + 1),
      )
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [pageCount])

  const visible = activities.slice(currentPage * pageSize, (currentPage + 1) * pageSize)

  return (
    <>
      <div className="activity-page-list" ref={listRef}>
        <ActivityList
          activities={visible}
          referenceIso={referenceIso}
          emptyMessage="No activity matches these filters."
        />
      </div>
      <nav className="activity-pagination" aria-label="Activity pages">
        <Button
          size="icon"
          variant="ghost"
          onClick={() => setPage((value) => Math.max(0, value - 1))}
          disabled={currentPage === 0}
          aria-label="Previous activity page"
        >
          <ChevronLeft size={16} aria-hidden="true" />
        </Button>
        <span aria-live="polite">
          Page {currentPage + 1} of {pageCount}
        </span>
        <Button
          size="icon"
          variant="ghost"
          onClick={() => setPage((value) => Math.min(pageCount - 1, value + 1))}
          disabled={currentPage === pageCount - 1}
          aria-label="Next activity page"
        >
          <ChevronRight size={16} aria-hidden="true" />
        </Button>
      </nav>
    </>
  )
}

export function ActivitiesPage() {
  const query = useFinance()
  const routeSearch = useSearch({ from: '/activities' })
  const navigate = useNavigate({ from: '/activities' })
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
  const activities = buildActivities(data, displayNames, getExternalLogosEnabled())
  const categories = [...new Set(activities.map(({ category }) => category || 'Other'))].toSorted(
    (left, right) => left.localeCompare(right),
  )
  const categoryOptions = [
    { value: '', label: 'All categories' },
    ...categories.map((category) => ({ value: category, label: category })),
  ]
  const requestedAccount = routeSearch.account ?? ''
  const requestedCategory = routeSearch.category ?? ''
  const selectedAccount = accounts.some(({ id }) => id === requestedAccount) ? requestedAccount : ''
  const selectedCategory = categories.includes(requestedCategory) ? requestedCategory : ''
  const accountIds = new Set([
    selectedAccount,
    ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
      snaptradeId === selectedAccount ? [plaidId] : [],
    ),
  ])
  const queryText = search.trim().toLocaleLowerCase()
  const filtered = activities.filter(
    (activity) =>
      (!selectedAccount || (activity.accountId != null && accountIds.has(activity.accountId))) &&
      (!selectedCategory || activity.category === selectedCategory) &&
      `${activity.title} ${activity.detail} ${activity.category} ${activity.amount}`
        .toLocaleLowerCase()
        .includes(queryText),
  )

  const updateFilters = (account: string, category: string) =>
    void navigate({
      search: { account: account || undefined, category: category || undefined },
      replace: true,
    })

  return (
    <div className="page spending-detail-page ledger-page activity-ledger-page">
      <WorkspaceHeader title="Activity" />
      <Card className="spending-detail-card ledger-card activity-page-card">
        <LedgerToolbar
          label="Search activity"
          placeholder="Search merchant, security or amount"
          value={search}
          onValueChange={setSearch}
          inputRef={searchInputRef}
        >
          <FilterSelect
            label="Activity account"
            value={selectedAccount}
            options={accountOptions}
            onValueChange={(value) => updateFilters(value, selectedCategory)}
          />
          <FilterSelect
            label="Activity category"
            value={selectedCategory}
            options={categoryOptions}
            onValueChange={(value) => updateFilters(selectedAccount, value)}
          />
        </LedgerToolbar>
        <PaginatedActivityList
          key={`${queryText}:${selectedAccount}:${selectedCategory}`}
          activities={filtered}
          referenceIso={data.updatedAt}
        />
      </Card>
    </div>
  )
}
