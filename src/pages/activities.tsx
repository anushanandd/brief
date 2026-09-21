import { Slider } from '@base-ui/react/slider'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { Download, Search, X } from 'lucide-react'
import { useState } from 'react'

import { GroupedActivityList } from '../components/activity-list'
import { PageError, PageLoading } from '../components/data-state'
import {
  FilterCheckboxGroup,
  LedgerFilters,
  selectedFilterValues,
  toggleFilterValue,
} from '../components/ledger-filters'
import { Button, Card, SectionHeading } from '../components/ui'
import { WorkspaceHeader, type WorkspaceBreadcrumb } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities, type ActivityItem } from '../lib/activity'
import { downloadActivityCsv } from '../lib/activity-csv'
import { analyticsCharts, analyticsEntries } from '../lib/analytics'
import { getExternalLogosEnabled } from '../lib/logos'
import { transactionDateKey } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

const dayMs = 86_400_000
const sliderDateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  timeZone: 'UTC',
})

export function activityMatchesDateRange(
  activity: ActivityItem,
  referenceIso: string,
  from?: string,
  to?: string,
) {
  if (!from && !to) return true
  const date = transactionDateKey(activity.date, referenceIso)
  return Boolean(date) && (!from || date >= from) && (!to || date <= to)
}

export function activityDateBounds(activities: ActivityItem[], referenceIso: string) {
  let earliest: string | undefined
  let latest: string | undefined
  for (const activity of activities) {
    const date = transactionDateKey(activity.date, referenceIso)
    if (!date) continue
    if (!earliest || date < earliest) earliest = date
    if (!latest || date > latest) latest = date
  }
  return earliest && latest ? ([earliest, latest] as const) : undefined
}

const dateToSliderValue = (date: string) => Date.parse(`${date}T00:00:00Z`) / dayMs
const sliderValueToDate = (value: number) => new Date(value * dayMs).toISOString().slice(0, 10)
const formatSliderDate = (value: number) => sliderDateFormatter.format(new Date(value * dayMs))

function DateRangeSlider({
  minDate,
  maxDate,
  from,
  to,
  onValueChange,
}: {
  minDate: string
  maxDate: string
  from?: string
  to?: string
  onValueChange: (from?: string, to?: string) => void
}) {
  const min = dateToSliderValue(minDate)
  const max = dateToSliderValue(maxDate)
  const requestedStart = from ? dateToSliderValue(from) : min
  const requestedEnd = to ? dateToSliderValue(to) : max
  const start = Number.isFinite(requestedStart) ? Math.max(min, Math.min(max, requestedStart)) : min
  const end = Number.isFinite(requestedEnd) ? Math.max(start, Math.min(max, requestedEnd)) : max

  if (min === max) {
    return <span className="ledger-date-single">{formatSliderDate(min)}</span>
  }

  return (
    <Slider.Root
      className="ledger-date-slider"
      defaultValue={[start, end]}
      key={`${min}-${max}-${start}-${end}`}
      min={min}
      max={max}
      step={1}
      thumbCollisionBehavior="none"
      onValueCommitted={(values) =>
        onValueChange(
          values[0] === min ? undefined : sliderValueToDate(values[0]),
          values[1] === max ? undefined : sliderValueToDate(values[1]),
        )
      }
    >
      <Slider.Value className="ledger-date-slider-values">
        {(_, values) => (
          <>
            <span>{formatSliderDate(values[0])}</span>
            <span>{formatSliderDate(values[1])}</span>
          </>
        )}
      </Slider.Value>
      <Slider.Control className="ledger-date-slider-control">
        <Slider.Track className="ledger-date-slider-track">
          <Slider.Indicator className="ledger-date-slider-indicator" />
        </Slider.Track>
        <Slider.Thumb
          className="ledger-date-slider-thumb"
          index={0}
          getAriaLabel={() => 'Start date'}
          getAriaValueText={(_, value) => formatSliderDate(value)}
        />
        <Slider.Thumb
          className="ledger-date-slider-thumb"
          index={1}
          getAriaLabel={() => 'End date'}
          getAriaValueText={(_, value) => formatSliderDate(value)}
        />
      </Slider.Control>
    </Slider.Root>
  )
}

export function ActivitiesPage() {
  const query = useFinance()
  const routeSearch = useSearch({ from: '/activities' })
  const navigate = useNavigate({ from: '/activities' })
  const [search, setSearch] = useState('')
  const inputRef = useSearchShortcuts(search, setSearch)
  const [exporting, setExporting] = useState(false)
  const [exportError, setExportError] = useState(false)

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
  const activities = buildActivities(data, displayNames, getExternalLogosEnabled())
    .filter(({ id }) => !analyticEntries || analyticEntries.has(id))
    .map((activity) =>
      analyticEntries ? { ...activity, date: analyticEntries.get(activity.id)!.date } : activity,
    )
    .toSorted((a, b) =>
      transactionDateKey(b.date, data.updatedAt).localeCompare(
        transactionDateKey(a.date, data.updatedAt),
      ),
    )
  const activityAccountIds = new Set(
    activities.flatMap(({ accountId }) => (accountId ? [accountId] : [])),
  )
  Object.entries(data.accountLinks ?? {}).forEach(([plaidId, snaptradeId]) => {
    if (activityAccountIds.has(plaidId) || activityAccountIds.has(snaptradeId)) {
      activityAccountIds.add(plaidId)
      activityAccountIds.add(snaptradeId)
    }
  })
  const accounts = routeSearch.analysis
    ? allAccounts
    : allAccounts.filter(({ id }) => activityAccountIds.has(id))
  const accountNames = Object.fromEntries(
    allAccounts.map((account) => [
      account.id,
      accountDisplayName(account.id, account.name, displayNames),
    ]),
  )
  const selectedAccounts = selectedFilterValues(
    routeSearch.account,
    accounts.map(({ id }) => id),
  )
  const accountIds = new Set(selectedAccounts)
  Object.entries(data.accountLinks ?? {}).forEach(([plaidId, snaptradeId]) => {
    if (routeSearch.analysis) return
    if (accountIds.has(plaidId) || accountIds.has(snaptradeId)) {
      accountIds.add(plaidId)
      accountIds.add(snaptradeId)
    }
  })
  const accountActivities = activities.filter(({ accountId }) =>
    routeSearch.analysis && routeSearch.account && !selectedAccounts.length
      ? false
      : !selectedAccounts.length || (accountId != null && accountIds.has(accountId)),
  )
  const selectedCategories = selectedFilterValues(routeSearch.category, [
    ...new Set(activities.map(({ category }) => category || 'Other')),
  ])
  const categories = [
    ...new Set([
      ...accountActivities.map(({ category }) => category || 'Other'),
      ...selectedCategories,
    ]),
  ].toSorted((left, right) => left.localeCompare(right))
  const categoryOptions = categories.map((category) => ({ value: category, label: category }))
  const categoryAccountIds = new Set(
    activities.flatMap(({ accountId, category }) =>
      accountId && (!selectedCategories.length || selectedCategories.includes(category || 'Other'))
        ? [accountId]
        : [],
    ),
  )
  Object.entries(data.accountLinks ?? {}).forEach(([plaidId, snaptradeId]) => {
    if (categoryAccountIds.has(plaidId) || categoryAccountIds.has(snaptradeId)) {
      categoryAccountIds.add(plaidId)
      categoryAccountIds.add(snaptradeId)
    }
  })
  const accountOptions = accounts
    .filter(({ id }) => selectedAccounts.includes(id) || categoryAccountIds.has(id))
    .map(({ id }) => ({ value: id, label: accountNames[id] }))
  const from = routeSearch.from
  const to = routeSearch.to
  const dateBounds = activityDateBounds(activities, data.updatedAt)
  const queryText = search.trim().toLocaleLowerCase()
  const filtered = accountActivities.filter(
    (activity) =>
      (!selectedCategories.length || selectedCategories.includes(activity.category || 'Other')) &&
      activityMatchesDateRange(activity, data.updatedAt, from, to) &&
      `${activity.title} ${activity.detail} ${activity.category} ${activity.amount} ${activity.accountId ? (accountNames[activity.accountId] ?? '') : ''}`
        .toLocaleLowerCase()
        .includes(queryText),
  )

  const updateFilters = (
    nextAccounts: string[],
    nextCategories: string[],
    nextFrom: string | undefined,
    nextTo: string | undefined,
  ) =>
    void navigate({
      search: {
        analysis: routeSearch.analysis,
        account: nextAccounts.join(',') || undefined,
        category: nextCategories.join(',') || undefined,
        from: nextFrom || undefined,
        to: nextTo || undefined,
      },
      replace: true,
    })

  const clearFilters = () => {
    void navigate({ search: {}, replace: true })
  }
  const activeFilterCount =
    selectedAccounts.length +
    selectedCategories.length +
    Number(Boolean(from || to)) +
    Number(Boolean(routeSearch.analysis))

  const breadcrumbs: WorkspaceBreadcrumb[] = [{ label: 'Activity', to: '/activities', search: {} }]
  if (routeSearch.analysis)
    breadcrumbs.push({
      label: analyticsCharts.find(({ id }) => id === routeSearch.analysis)!.label,
      to: '/activities',
      search: { analysis: routeSearch.analysis },
    })
  const accountSearch = selectedAccounts.join(',') || undefined
  const categorySearch = selectedCategories.join(',') || undefined
  if (selectedAccounts.length)
    breadcrumbs.push({
      label: selectedAccounts.map((id) => accountNames[id]).join(', '),
      to: '/activities',
      search: { analysis: routeSearch.analysis, account: accountSearch },
    })
  if (selectedCategories.length)
    breadcrumbs.push({
      label: selectedCategories.join(', '),
      to: '/activities',
      search: { analysis: routeSearch.analysis, account: accountSearch, category: categorySearch },
    })
  if (from || to)
    breadcrumbs.push({
      label:
        from && to
          ? `${formatSliderDate(dateToSliderValue(from))} – ${formatSliderDate(dateToSliderValue(to))}`
          : from
            ? `From ${formatSliderDate(dateToSliderValue(from))}`
            : `Through ${formatSliderDate(dateToSliderValue(to!))}`,
      to: '/activities',
      search: {
        analysis: routeSearch.analysis,
        account: accountSearch,
        category: categorySearch,
        from,
        to,
      },
    })

  return (
    <div className="page spending-detail-page ledger-page activity-ledger-page">
      <WorkspaceHeader
        title="Activity"
        breadcrumbs={breadcrumbs.length > 1 ? breadcrumbs : undefined}
      />
      <div className="ledger-workspace-grid activity-ledger-grid">
        <Card className="spending-detail-card ledger-card activity-page-card">
          <SectionHeading
            title="Activity"
            action={
              <div className="activity-toolbar">
                <label className="ledger-search">
                  <Search size={16} aria-hidden="true" />
                  <span className="sr-only">Search activity</span>
                  <input
                    ref={inputRef}
                    type="search"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    placeholder="Search description, category, or account"
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
                </label>

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
                      await downloadActivityCsv(filtered, 'all', 'Brief')
                    } catch {
                      setExportError(true)
                    } finally {
                      setExporting(false)
                    }
                  }}
                >
                  <Download size={16} aria-hidden="true" />
                </Button>
              </div>
            }
          />
          {exportError ? (
            <p role="alert">CSV could not be saved. Try again with a new filename.</p>
          ) : null}
          <div
            className="activity-page-list"
            role="region"
            aria-label="Activity results"
            tabIndex={0}
          >
            <GroupedActivityList activities={filtered} referenceIso={data.updatedAt} />
          </div>
        </Card>
        <LedgerFilters activeCount={activeFilterCount} onClearAll={clearFilters}>
          {routeSearch.analysis ? (
            <div className="analytics-note">
              <span>
                {analyticsCharts.find(({ id }) => id === routeSearch.analysis)!.label} · Posted
                dates
              </span>
              <Button
                icon={X}
                variant="ghost"
                size="compact"
                aria-label="Clear analytics filter"
                onClick={() => void navigate({ search: { ...routeSearch, analysis: undefined } })}
              >
                Clear
              </Button>
            </div>
          ) : null}
          {dateBounds ? (
            <fieldset className="ledger-date-group">
              <legend className="sr-only">Dates</legend>
              {from || to ? (
                <div className="ledger-checkbox-group-header">
                  <button
                    type="button"
                    className="ledger-filter-clear"
                    aria-label="Clear dates"
                    onClick={() =>
                      updateFilters(selectedAccounts, selectedCategories, undefined, undefined)
                    }
                  >
                    <X size={14} aria-hidden="true" />
                    <span className="sr-only">Clear</span>
                  </button>
                </div>
              ) : null}
              <DateRangeSlider
                minDate={dateBounds[0]}
                maxDate={dateBounds[1]}
                from={from}
                to={to}
                onValueChange={(nextFrom, nextTo) =>
                  updateFilters(selectedAccounts, selectedCategories, nextFrom, nextTo)
                }
              />
            </fieldset>
          ) : null}
          {accountOptions.length ? (
            <FilterCheckboxGroup
              label="Accounts"
              options={accountOptions}
              values={new Set(selectedAccounts)}
              onToggle={(value) =>
                updateFilters(
                  toggleFilterValue(selectedAccounts, value),
                  selectedCategories,
                  from,
                  to,
                )
              }
              onClear={() => updateFilters([], selectedCategories, from, to)}
            />
          ) : null}
          {categoryOptions.length ? (
            <FilterCheckboxGroup
              label="Categories"
              options={categoryOptions}
              values={new Set(selectedCategories)}
              onToggle={(value) =>
                updateFilters(
                  selectedAccounts,
                  toggleFilterValue(selectedCategories, value),
                  from,
                  to,
                )
              }
              onClear={() => updateFilters(selectedAccounts, [], from, to)}
            />
          ) : null}
        </LedgerFilters>
      </div>
    </div>
  )
}
