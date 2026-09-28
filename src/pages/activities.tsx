import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { Command } from 'cmdk'
import { useEffect, useLayoutEffect, useRef, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { ActivityDateSlider } from '../components/activity-date-slider'
import { GroupedActivityList, type ActivityFilterColumn } from '../components/activity-list'
import { ActivityMethodMark } from '../components/activity-method-mark'
import { CategoryMark } from '../components/category-mark'
import { PageError, PageLoading } from '../components/data-state'
import { Download, Search, X } from '../components/icons'
import { selectedFilterValues } from '../components/ledger-filters'
import { Button, ChartRangeSelect, ScrollCueCard, SectionHeading } from '../components/ui'
import { WorkspaceHeader, type WorkspaceBreadcrumb } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import {
  buildActivities,
  activityLocationLabel,
  activityMethod,
  linkedAccountIds,
  sortActivities,
  type ActivityItem,
  type ActivitySort,
} from '../lib/activity'
import { downloadActivityCsv } from '../lib/activity-csv'
import {
  activityDateBounds,
  activityDatePreset,
  activityDateRangeOptions,
  activityDateRangeValue,
  activityFilterCandidates,
  filterValue,
  formatActivityDate,
  type ActivityDateRange,
} from '../lib/activity-filters'
import { analyticsCharts, analyticsEntries, localDateKey } from '../lib/analytics'
import { pageShortcutBlocked } from '../lib/keyboard'
import { getExternalLogosEnabled } from '../lib/logos'
import { transactionDateKey } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

function activityPlainShortcut(
  event: Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'repeat'>,
  blocked: boolean,
  key: string,
) {
  return (
    !blocked &&
    !event.repeat &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.shiftKey &&
    event.key.toLowerCase() === key
  )
}

export const activityFilterShortcut = (
  event: Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'repeat'>,
  blocked: boolean,
) => activityPlainShortcut(event, blocked, 'f')

export const activityClearShortcut = (
  event: Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'repeat'>,
  blocked: boolean,
) => activityPlainShortcut(event, blocked, 'c')

export function ActivitiesPage() {
  return <ActivitiesContent />
}

export function MiddayActivityPrototypePage() {
  return <ActivitiesContent prototype />
}

function ActivitiesContent({ prototype = false }: { prototype?: boolean }) {
  useLayoutEffect(() => {
    document.documentElement.classList.add('activity-viewport')
    return () => document.documentElement.classList.remove('activity-viewport')
  }, [])

  const query = useFinance()
  const routePath = prototype ? '/settings/design/midday/activity' : '/activities'
  const routeSearch = useSearch({ from: routePath })
  const navigate = useNavigate({ from: routePath })
  const search = routeSearch.q ?? ''
  const updateFilters = (patch: Partial<typeof routeSearch>) =>
    void navigate({ search: { ...routeSearch, ...patch }, replace: true })
  const setSearch = (value: string) => updateFilters({ q: value || undefined })
  const inputRef = useSearchShortcuts(search, setSearch)
  const [exporting, setExporting] = useState(false)
  const [exportError, setExportError] = useState(false)
  const [sort, setSort] = useState<ActivitySort>({ column: 'date', direction: 'desc' })
  const [filterQuery, setFilterQuery] = useState('')
  const [filterSelection, setFilterSelection] = useState('')
  const [customDateMode, setCustomDateMode] = useState(false)
  const filterInputRef = useRef<HTMLInputElement>(null)
  const filterBodyRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!routeSearch.from && !routeSearch.to) setCustomDateMode(false)
  }, [routeSearch.from, routeSearch.to])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const blocked = pageShortcutBlocked(event)
      if (activityFilterShortcut(event, blocked)) {
        event.preventDefault()
        filterInputRef.current?.focus()
      } else if (activityClearShortcut(event, blocked)) {
        event.preventDefault()
        setFilterQuery('')
        setFilterSelection('')
        setCustomDateMode(false)
        filterBodyRef.current?.blur()
        void navigate({ search: {}, replace: true })
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [navigate])

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const displayNames = getAccountDisplayNames()
  const allAccounts = data.accounts.filter(({ id }) => id !== 'all')
  const analyticEntries = routeSearch.analysis
    ? new Map(
        analyticsEntries(data, routeSearch.analysis, getSpendingAccountId()).map((entry) => [
          entry.id,
          entry,
        ]),
      )
    : undefined
  const allActivities = buildActivities(data, displayNames, getExternalLogosEnabled())
  const activities = allActivities
    .filter(({ id }) => !analyticEntries || analyticEntries.has(id))
    .map((activity) =>
      analyticEntries ? { ...activity, date: analyticEntries.get(activity.id)!.date } : activity,
    )
    .toSorted((a, b) =>
      transactionDateKey(b.date, data.updatedAt).localeCompare(
        transactionDateKey(a.date, data.updatedAt),
      ),
    )
  const activityAccountIds = linkedAccountIds(
    activities.map(({ accountId }) => accountId),
    data.accountLinks,
  )
  const accounts = routeSearch.analysis
    ? allAccounts
    : allAccounts.filter(({ id }) => activityAccountIds.has(id))
  const accountNames = Object.fromEntries(
    allAccounts.map((account) => [
      account.id,
      accountDisplayName(account.id, account.name, displayNames),
    ]),
  )
  const accountTypes = Object.fromEntries(allAccounts.map(({ id, type }) => [id, type]))
  const selectedAccounts = selectedFilterValues(
    routeSearch.account,
    accounts.map(({ id }) => id),
  )
  const accountIds = routeSearch.analysis
    ? new Set(selectedAccounts)
    : linkedAccountIds(selectedAccounts, data.accountLinks)
  const selectedCategories = selectedFilterValues(routeSearch.category, [
    ...new Set(activities.map(({ category }) => category || 'Other')),
  ])
  const selectedMethods = selectedFilterValues(routeSearch.method, [
    ...new Set(activities.map(activityMethod).filter(Boolean)),
  ])
  const from = routeSearch.from
  const to = routeSearch.to
  const dateBounds = activityDateBounds(activities, data.updatedAt)
  const selectedDateRange = customDateMode
    ? 'custom'
    : activityDateRangeValue(from, to, localDateKey(Date.now() / 1000))
  const candidates = activityFilterCandidates(
    activities,
    data.updatedAt,
    search,
    from,
    to,
    routeSearch.account ? accountIds : undefined,
    selectedCategories,
    selectedMethods,
  )
  const categories = [
    ...new Set([
      ...candidates.categories.map(({ category }) => category || 'Other'),
      ...selectedCategories,
    ]),
  ].toSorted((left, right) => left.localeCompare(right))
  const categoryOptions = categories.map((category) => ({
    value: category,
    label: category,
    mark: <CategoryMark category={category} />,
  }))
  const methods = [
    ...new Set([...candidates.methods.map(activityMethod).filter(Boolean), ...selectedMethods]),
  ].toSorted((left, right) => left.localeCompare(right))
  const methodOptions = methods.map((method) => ({
    value: method,
    label: method,
    mark: <ActivityMethodMark method={method} />,
  }))
  const matchingAccountIds = linkedAccountIds(
    candidates.accounts.map(({ accountId }) => accountId),
    data.accountLinks,
  )
  const accountOptions = accounts
    .filter(({ id }) => selectedAccounts.includes(id) || matchingAccountIds.has(id))
    .map(({ id, type }) => ({
      value: id,
      label: accountNames[id],
      mark: <AccountMark type={type} />,
    }))
  const filterGroups = [
    {
      heading: 'Accounts',
      key: 'account',
      keywords: ['account'],
      options: accountOptions,
      selected: selectedAccounts,
      add: (value: string) => updateFilters({ account: filterValue([...selectedAccounts, value]) }),
    },
    {
      heading: 'Categories',
      key: 'category',
      keywords: ['category'],
      options: categoryOptions,
      selected: selectedCategories,
      add: (value: string) =>
        updateFilters({ category: filterValue([...selectedCategories, value]) }),
    },
    {
      heading: 'Methods',
      key: 'method',
      keywords: ['method', 'channel'],
      options: methodOptions,
      selected: selectedMethods,
      add: (value: string) => updateFilters({ method: filterValue([...selectedMethods, value]) }),
    },
  ]
  const filtered = candidates.results
  const sorted = sortActivities(filtered, data.updatedAt, sort)
  const toggleSort = (column: ActivitySort['column']) =>
    setSort((current) => ({
      column,
      direction:
        current.column === column
          ? current.direction === 'asc'
            ? 'desc'
            : 'asc'
          : column === 'date' || column === 'amount'
            ? 'desc'
            : 'asc',
    }))

  const clearFilters = () => {
    void navigate({ search: {}, replace: true })
    setFilterQuery('')
    setFilterSelection('')
    setCustomDateMode(false)
  }
  const finishFilterSelection = () => {
    setFilterQuery('')
    setFilterSelection('')
    filterBodyRef.current?.focus()
  }
  const activeFilterCount =
    selectedAccounts.length +
    selectedCategories.length +
    selectedMethods.length +
    Number(Boolean(from || to)) +
    Number(Boolean(routeSearch.analysis))
  const analysisLabel = routeSearch.analysis
    ? analyticsCharts.find(({ id }) => id === routeSearch.analysis)!.label
    : undefined
  const dateFilterLabel =
    from && to
      ? from === to
        ? formatActivityDate(from)
        : `${formatActivityDate(from)} – ${formatActivityDate(to)}`
      : from
        ? `From ${formatActivityDate(from)}`
        : to
          ? `Through ${formatActivityDate(to)}`
          : undefined
  const activeTags = [
    ...(analysisLabel
      ? [
          {
            id: 'analysis',
            label: 'Analytics',
            value: `${analysisLabel} · Posted dates`,
            remove: () => updateFilters({ analysis: undefined }),
          },
        ]
      : []),
    ...selectedAccounts.map((id) => ({
      id: `account:${id}`,
      label: 'Account',
      value: accountNames[id] ?? id,
      remove: () =>
        updateFilters({ account: filterValue(selectedAccounts.filter((value) => value !== id)) }),
    })),
    ...selectedCategories.map((category) => ({
      id: `category:${category}`,
      label: 'Category',
      value: category,
      remove: () =>
        updateFilters({
          category: filterValue(selectedCategories.filter((value) => value !== category)),
        }),
    })),
    ...selectedMethods.map((method) => ({
      id: `method:${method}`,
      label: 'Method',
      value: method,
      remove: () =>
        updateFilters({ method: filterValue(selectedMethods.filter((value) => value !== method)) }),
    })),
    ...(dateFilterLabel
      ? [
          {
            id: 'date',
            label: 'Date',
            value: dateFilterLabel,
            remove: () => {
              setCustomDateMode(false)
              updateFilters({ from: undefined, to: undefined })
            },
          },
        ]
      : []),
  ]
  const applyCellFilter = (column: ActivityFilterColumn, activity: ActivityItem) => {
    if (column === 'description') {
      setSearch(activity.title)
      return
    }
    if (column === 'location') {
      setSearch(
        activityLocationLabel(activity.location) === 'Address'
          ? (activity.location?.address ?? '')
          : activityLocationLabel(activity.location),
      )
      return
    }
    if (column === 'account') {
      if (!activity.accountId) return
      const matchingAccount = accounts.find(({ id }) => id === activity.accountId)
      const accountId =
        matchingAccount?.id ??
        Object.entries(data.accountLinks ?? {}).find(
          ([plaidId, snaptradeId]) =>
            plaidId === activity.accountId || snaptradeId === activity.accountId,
        )?.[0]
      if (accountId) updateFilters({ account: accountId })
      return
    }
    if (column === 'category') {
      updateFilters({ category: activity.category || 'Other' })
      return
    }
    if (column === 'method') {
      updateFilters({ method: activityMethod(activity) })
      return
    }
    const day = transactionDateKey(activity.date, data.updatedAt)
    if (day) updateFilters({ from: day, to: day })
  }

  const breadcrumbs: WorkspaceBreadcrumb[] = [{ label: 'Activity', to: routePath, search: {} }]
  const breadcrumbSearch: Partial<typeof routeSearch> = {}
  const addBreadcrumb = (label: string, patch: Partial<typeof routeSearch>) => {
    Object.assign(breadcrumbSearch, patch)
    breadcrumbs.push({ label, to: routePath, search: { ...breadcrumbSearch } })
  }
  if (analysisLabel) addBreadcrumb(analysisLabel, { analysis: routeSearch.analysis })
  if (selectedAccounts.length)
    addBreadcrumb(selectedAccounts.map((id) => accountNames[id]).join(', '), {
      account: filterValue(selectedAccounts),
    })
  if (selectedCategories.length)
    addBreadcrumb(selectedCategories.join(', '), {
      category: filterValue(selectedCategories),
    })
  if (selectedMethods.length)
    addBreadcrumb(selectedMethods.join(', '), {
      method: filterValue(selectedMethods),
    })
  if (dateFilterLabel) addBreadcrumb(dateFilterLabel, { from, to })

  return (
    <div className="page spending-detail-page ledger-page activity-ledger-page">
      <WorkspaceHeader
        title="Activity"
        breadcrumbs={breadcrumbs.length > 1 ? breadcrumbs : undefined}
        actions={
          prototype ? (
            <Link to="/settings/design" className="midday-prototype-back">
              Design
            </Link>
          ) : undefined
        }
      />
      <div className="activity-page-layout">
        <ScrollCueCard
          className="activity-filter-card"
          scrollSelector=".activity-filter-card-body"
          role="region"
          aria-label={`Filters, ${activeFilterCount} active`}
        >
          <SectionHeading
            title="Filters"
            action={
              <button
                type="button"
                className="activity-filter-clear"
                aria-label="Clear all filters"
                aria-keyshortcuts="C"
                disabled={!activeFilterCount && !search && !filterQuery}
                onClick={clearFilters}
              >
                Clear all
                <kbd aria-hidden="true">C</kbd>
              </button>
            }
          />
          <Command
            label="Activity filters"
            className="activity-filter-card-command"
            value={filterSelection}
            onValueChange={setFilterSelection}
            onKeyDownCapture={(event) => {
              if (event.key !== 'Escape') return
              event.preventDefault()
              event.stopPropagation()
              setFilterQuery('')
              setFilterSelection('')
              if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
            }}
          >
            <label className="command-input-wrap activity-filter-card-search">
              <span className="sr-only">Find a filter</span>
              <Command.Input
                ref={filterInputRef}
                value={filterQuery}
                onValueChange={setFilterQuery}
                placeholder="Find a filter"
                aria-keyshortcuts="F Escape"
              />
              <kbd>F</kbd>
            </label>
            <div
              ref={filterBodyRef}
              className="activity-filter-card-body"
              role="region"
              aria-label="Filter options"
              tabIndex={0}
            >
              <Command.List
                className="command-list activity-filter-card-list"
                label="Available filters"
              >
                <Command.Empty className="command-empty">No matching filters.</Command.Empty>
                {filterGroups.map((group) => {
                  const available = group.options.filter(
                    (option) => !group.selected.includes(option.value),
                  )
                  return available.length ? (
                    <Command.Group
                      className="command-group"
                      heading={group.heading}
                      key={group.key}
                    >
                      {available.map((option) => (
                        <Command.Item
                          key={option.value}
                          value={`${group.key}:${option.label}`}
                          keywords={[option.label, ...group.keywords]}
                          onSelect={() => {
                            finishFilterSelection()
                            group.add(option.value)
                          }}
                        >
                          {option.mark}
                          <span>{option.label}</span>
                        </Command.Item>
                      ))}
                    </Command.Group>
                  ) : null
                })}
              </Command.List>
            </div>
          </Command>
        </ScrollCueCard>
        <ScrollCueCard
          className="spending-detail-card ledger-card activity-page-card"
          scrollSelector=".activity-page-list"
        >
          <div className="activity-toolbar">
            <div className="ledger-search activity-search">
              <Search size={16} aria-hidden="true" />
              <input
                ref={inputRef}
                type="search"
                aria-label="Search activity"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search activity"
              />
              {search ? (
                <Button
                  size="icon"
                  variant="ghost"
                  type="button"
                  onClick={() => setSearch('')}
                  aria-label="Clear search"
                >
                  <X size={14} aria-hidden="true" />
                </Button>
              ) : null}
            </div>
            {activeFilterCount ? (
              <div className="activity-active-filters" role="group" aria-label="Active filters">
                {activeTags.map((tag) => (
                  <button
                    type="button"
                    className="activity-filter-tag"
                    key={tag.id}
                    aria-label={`Remove ${tag.label.toLowerCase()} filter: ${tag.value}`}
                    onClick={tag.remove}
                  >
                    <span className="activity-filter-label">{tag.label}:</span>
                    <span className="activity-filter-value">{tag.value}</span>
                    <X size={12} aria-hidden="true" />
                  </button>
                ))}
              </div>
            ) : null}

            <ChartRangeSelect<ActivityDateRange>
              label="Activity date range"
              options={activityDateRangeOptions}
              value={selectedDateRange}
              onValueChange={(range) => {
                if (range === 'custom') {
                  setCustomDateMode(true)
                  return
                }
                setCustomDateMode(false)
                const next = activityDatePreset(range, localDateKey(Date.now() / 1000))
                updateFilters({ from: next.from, to: next.to })
              }}
            />

            <Button
              variant="ghost"
              size="icon"
              className="icon-only-subtle"
              aria-label="Download filtered activity as CSV"
              title="Download filtered activity as CSV"
              disabled={exporting || !filtered.length}
              aria-busy={exporting}
              onClick={async () => {
                setExporting(true)
                setExportError(false)
                try {
                  await downloadActivityCsv(sorted, 'all', 'Brief')
                } catch {
                  setExportError(true)
                } finally {
                  setExporting(false)
                }
              }}
            >
              <Download size={16} aria-hidden="true" />
            </Button>
            {selectedDateRange === 'custom' ? (
              <div
                className="activity-date-custom-panel"
                role="group"
                aria-label="Custom date range"
              >
                {dateBounds ? (
                  <ActivityDateSlider
                    minDate={from && from < dateBounds[0] ? from : dateBounds[0]}
                    maxDate={to && to > dateBounds[1] ? to : dateBounds[1]}
                    from={from}
                    to={to}
                    onChange={(nextFrom, nextTo) => updateFilters({ from: nextFrom, to: nextTo })}
                  />
                ) : (
                  <span className="ledger-date-single">No activity dates available.</span>
                )}
              </div>
            ) : null}
          </div>
          {exportError ? (
            <p role="alert">CSV could not be saved. Try again with a new filename.</p>
          ) : null}
          <div
            className="activity-page-list"
            role="region"
            aria-label="Activity results"
            tabIndex={0}
            data-keyboard-region
          >
            <GroupedActivityList
              activities={sorted}
              sizingActivities={allActivities}
              accountTypes={accountTypes}
              referenceIso={data.updatedAt}
              showColumns
              sort={sort}
              onSort={toggleSort}
              onFilter={applyCellFilter}
            />
          </div>
        </ScrollCueCard>
      </div>
    </div>
  )
}
