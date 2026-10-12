import { useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'

import { ActivityFacets } from '../components/activity-facets'
import { ActivityFilterCard } from '../components/activity-filter-card'
import { GroupedActivityList } from '../components/activity-list'
import { ActivityQueryBar } from '../components/activity-query-bar'
import { PageError, PageLoading } from '../components/data-state'
import { DateRangePicker } from '../components/date-range-picker'
import { Download, Filter } from '../components/icons'
import { TransactionDialog } from '../components/transaction-dialog'
import { Button, ScrollCueCard } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useCsvExport } from '../hooks/use-csv-export'
import { useFinance } from '../hooks/use-finance'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useLatest } from '../hooks/use-latest'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ActivityItem, ActivitySort } from '../lib/activity'
import {
  activityDatePreset,
  activityDateRangeOptions,
  activityFiltersOpen,
  filterValue,
  type ActivityDateRange,
} from '../lib/activity-filters'
import {
  activityLedger,
  activityRows,
  type ActivityLedger,
  type LedgerPatch,
  type SectionPatches,
} from '../lib/activity-ledger'
import { activityViewSearch, withActivityView, activityViews } from '../lib/activity-views'
import { localDateKey } from '../lib/dates'
import { errorMessage } from '../lib/errors'
import type { FinanceSnapshot } from '../lib/schema'
import { nextSort } from '../lib/sort'

const toggled = (values: string[], value: string) =>
  values.includes(value) ? values.filter((item) => item !== value) : [...values, value]

const sortColumns = [
  ['description', 'Description'],
  ['account', 'Account'],
  ['category', 'Category'],
  ['owner', 'For'],
  ['location', 'Location'],
  ['date', 'Date'],
  ['amount', 'Amount'],
] as const

const filterSections = [
  ['categories', 'category'],
  ['accounts', 'account'],
  ['methods', 'method'],
] as const

export function ActivitiesPage() {
  useLayoutEffect(() => {
    document.documentElement.classList.add('activity-viewport')
    return () => document.documentElement.classList.remove('activity-viewport')
  }, [])
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <ActivityLedgerPage data={query.data} />
}

/** The ledger row holding keyboard focus, which F switches between Me and Other. */
function useFocusedActivity() {
  const [focused, setFocused] = useState<string>()
  useEffect(() => {
    const update = () =>
      setFocused(
        document.activeElement?.closest<HTMLElement>('.activity-page-list [data-activity-id]')
          ?.dataset.activityId,
      )
    document.addEventListener('focusin', update)
    document.addEventListener('focusout', update)
    return () => {
      document.removeEventListener('focusin', update)
      document.removeEventListener('focusout', update)
    }
  }, [])
  return focused
}

function ActivityLedgerPage({ data }: { data: FinanceSnapshot }) {
  const { annotate } = useFinanceEdits()
  const search = useSearch({ from: '/activities' })
  const navigate = useNavigate({ from: '/activities' })
  const update = useCallback(
    (patch: LedgerPatch) =>
      void navigate({ search: (current) => ({ ...current, ...patch }), replace: true }),
    [navigate],
  )
  const filtersOpen = activityFiltersOpen.use()
  const focusedRow = useFocusedActivity()
  const [draft, setDraft] = useState('')
  const inputRef = useSearchShortcuts(draft, setDraft)
  const csv = useCsvExport()
  const [sort, setSort] = useState<ActivitySort>({ column: 'date', direction: 'desc' })
  const savedViews = activityViews.use()
  // Me/Other choices shown at once, by activity ID, while their save is on its way.
  const [flipped, setFlipped] = useState<Record<string, boolean>>({})
  const [openId, setOpenId] = useState<string | undefined>(() =>
    search.open ? `spending:${search.open}` : undefined,
  )
  const openerRef = useRef<HTMLElement | null>(null)

  const rows = useMemo(
    () => activityRows(data, { analysis: search.analysis, review: search.review }),
    [data, search.analysis, search.review],
  )
  const today = localDateKey(Date.now() / 1000)
  const ledger = activityLedger({ data, rows, search, draft, sort, today })
  const shown = Object.keys(flipped).length
    ? ledger.sorted.map((activity) =>
        activity.id in flipped ? { ...activity, other: flipped[activity.id] } : activity,
      )
    : ledger.sorted

  // Opening moves focus to the first filter, so the arrows and Enter continue there.
  const toggleFilters = useCallback(() => {
    const open = !activityFiltersOpen.get()
    activityFiltersOpen.save(open)
    if (open)
      requestAnimationFrame(() =>
        document.querySelector<HTMLElement>('.activity-filter-card [data-keyboard-row]')?.focus(),
      )
  }, [])
  const toggleSort = useCallback(
    (column: ActivitySort['column']) =>
      setSort((current) =>
        nextSort(current, column, column === 'date' || column === 'amount' ? 'desc' : 'asc'),
      ),
    [],
  )
  const clearAll = useCallback(() => {
    setDraft('')
    void navigate({ search: {}, replace: true })
  }, [navigate])
  const toggleReview = useCallback(
    () =>
      void navigate({
        search: (current) => ({ ...current, review: current.review ? undefined : true }),
        replace: true,
      }),
    [navigate],
  )
  const openDetails = (activity: ActivityItem) => {
    if (document.activeElement instanceof HTMLElement) openerRef.current = document.activeElement
    setOpenId(activity.id)
  }
  const closeDetails = useCallback(() => {
    const opener = openerRef.current
    setOpenId(undefined)
    void navigate({ search: ({ open: _open, ...rest }) => rest, replace: true })
    // The row that opened the dialog takes focus back, so keyboard review continues in place.
    requestAnimationFrame(() => opener?.focus({ preventScroll: true }))
  }, [navigate])
  const saveView = () => {
    if (!ledger.viewName) return
    activityViews.save(
      withActivityView(savedViews, { name: ledger.viewName, search: activityViewSearch(search) }),
    )
  }
  const exportCsv = async () => {
    if (ledger.filtered.length) await csv.run(ledger.sorted, 'all', 'Brief')
  }

  // The row switches on click; the saved snapshot replaces it, or the row switches back.
  const toggleOther = (activity: ActivityItem) => {
    const other = !(flipped[activity.id] ?? activity.other)
    setFlipped((current) => ({ ...current, [activity.id]: other }))
    void annotate({ [activity.id.slice('spending:'.length)]: { other } })
      .catch((error: unknown) =>
        toast.error('Could not change who it’s for', {
          description: errorMessage(error),
        }),
      )
      .finally(() =>
        setFlipped(({ [activity.id]: pending, ...rest }) =>
          pending === other ? rest : { ...rest, [activity.id]: pending },
        ),
      )
  }
  const commitDraft = () => {
    update(ledger.committed())
    setDraft('')
  }
  const setDates = (range: Exclude<ActivityDateRange, 'custom'>) => {
    const next = activityDatePreset(range, today)
    update({ from: next.from, to: next.to })
  }

  useActivityCommands({
    ledger,
    shown,
    filtersOpen,
    focusedRow,
    focusSearch: () => inputRef.current?.focus(),
    update,
    toggleFilters,
    toggleSort,
    toggleReview,
    clearAll,
    exportCsv: () => void exportCsv(),
    saveView,
    toggleOther,
  })

  const sectionControls = (name: 'categories' | 'accounts' | 'methods' | 'amount') => {
    const section: SectionPatches = ledger.sections[name]
    return {
      command: `activity.filters.${name}`,
      all: section.all && section.canAll ? () => update(section.all!) : undefined,
      clear: section.canClear ? () => update(section.clear) : undefined,
    }
  }
  const openActivity = openId ? rows.activities.find(({ id }) => id === openId) : undefined
  const neighbor = (id: string | undefined, offset: number) =>
    ledger.sorted[ledger.sorted.findIndex((activity) => activity.id === id) + offset]

  return (
    <div className="page spending-detail-page ledger-page activity-ledger-page">
      <WorkspaceHeader
        title="Activity"
        breadcrumbs={
          ledger.breadcrumbs.length
            ? [
                { label: 'Activity', to: '/activities', search: {} },
                ...ledger.breadcrumbs.map((crumb) => ({
                  label: crumb.label,
                  to: '/activities' as const,
                  search: crumb.search,
                })),
              ]
            : undefined
        }
      />
      <div className="activity-page-layout">
        <ScrollCueCard
          className="spending-detail-card ledger-card activity-page-card"
          scrollSelector=".activity-page-list"
        >
          <div className="activity-toolbar">
            <ActivityQueryBar
              inputRef={inputRef}
              draft={draft}
              onDraftChange={setDraft}
              chips={ledger.chips.map((chip) => ({ ...chip, remove: () => update(chip.patch) }))}
              label={`Filters, ${ledger.activeFilterCount} active`}
              suggestions={ledger.suggestions}
              preview={ledger.preview}
              onCommit={commitDraft}
              canSave={Boolean(ledger.viewName)}
              onSave={saveView}
            />

            <Button
              variant="ghost"
              icon={Filter}
              className="activity-filters-button"
              aria-pressed={filtersOpen}
              aria-keyshortcuts="F"
              data-command="activity.filters.toggle"
              onClick={toggleFilters}
            >
              Filters
              {ledger.cardFilterCount ? (
                <span className="activity-filters-count">{ledger.cardFilterCount}</span>
              ) : null}
            </Button>

            <DateRangePicker<Exclude<ActivityDateRange, 'custom'>>
              label="Activity dates"
              command="activity.range."
              presets={ledger.datePresets}
              value={ledger.selectedDateRange === 'custom' ? undefined : ledger.selectedDateRange}
              summary={
                ledger.selectedDateRange === 'custom'
                  ? (ledger.dateFilterLabel ?? 'Custom')
                  : (ledger.datePresets.find(({ value }) => value === ledger.selectedDateRange)
                      ?.label ?? 'All time')
              }
              onPreset={setDates}
            />

            <Button
              variant="ghost"
              size="icon"
              className="icon-only-subtle"
              aria-label="Download filtered activity as CSV"
              title="Download filtered activity as CSV"
              data-command="activity.export"
              disabled={csv.exporting || !ledger.filtered.length}
              aria-busy={csv.exporting}
              onClick={() => void exportCsv()}
            >
              <Download size={16} aria-hidden="true" />
            </Button>
          </div>
          <ActivityFacets
            review={Boolean(search.review)}
            reviewCount={rows.reviewCount}
            onReview={toggleReview}
            facets={filtersOpen ? [] : ledger.facets}
            onFacet={(category) =>
              update({ category: filterValue([...ledger.selectedCategories, category]) })
            }
            savedViews={savedViews}
            onOpenView={(view) => {
              setDraft('')
              void navigate({ search: view.search, replace: true })
            }}
            onDeleteView={(view) =>
              activityViews.save(savedViews.filter(({ name }) => name !== view.name))
            }
          />
          {csv.failed ? (
            <p role="alert">CSV could not be saved. Try again with a new filename.</p>
          ) : null}
          <div
            className="activity-page-list"
            role="region"
            aria-label="Activity results"
            tabIndex={0}
            data-range-focus-region
            data-keyboard-region
          >
            <GroupedActivityList
              activities={shown}
              sizingActivities={rows.allActivities}
              accountTypes={ledger.accountTypes}
              accountInstitutions={ledger.accountInstitutions}
              referenceIso={data.updatedAt}
              emptyMessage={
                search.review && ledger.activeFilterCount === 1
                  ? 'Nothing to review. Brief flags possible duplicates and unusual amounts here.'
                  : undefined
              }
              showColumns
              sort={sort}
              onSort={toggleSort}
              onOpen={openDetails}
              selectedId={openId}
              onFilter={(column, activity) => {
                const patch = ledger.cellFilter(column, activity)
                if (patch) update(patch)
              }}
              onToggleOther={toggleOther}
            />
          </div>
        </ScrollCueCard>
        {filtersOpen ? (
          <ActivityFilterCard
            active={ledger.cardFilterCount}
            owner={search.for}
            ownerCounts={ledger.options.owners}
            onOwner={(owner) => update({ for: owner })}
            datePresets={ledger.datePresets}
            dateRange={ledger.selectedDateRange}
            from={search.from}
            to={search.to}
            dateBounds={ledger.dateBounds}
            onDatePreset={(range) => {
              const preset = ledger.datePresets.find(({ value }) => value === range)
              if (preset) setDates(preset.value)
            }}
            onDates={(from, to) => update({ from, to })}
            categories={ledger.options.categories}
            included={ledger.selectedCategories}
            excluded={ledger.excludedCategories}
            // A category hidden elsewhere (a chip, a saved view) comes back; any other is
            // chosen or unchosen.
            onInclude={(category) =>
              update(
                ledger.excludedCategories.includes(category)
                  ? {
                      exclude: filterValue(
                        ledger.excludedCategories.filter((value) => value !== category),
                      ),
                    }
                  : { category: filterValue(toggled(ledger.selectedCategories, category)) },
              )
            }
            accounts={ledger.options.accounts}
            selectedAccounts={ledger.selectedAccounts}
            onAccount={(id) =>
              update({ account: filterValue(toggled(ledger.selectedAccounts, id)) })
            }
            methods={ledger.options.methods}
            selectedMethods={ledger.selectedMethods}
            onMethod={(method) =>
              update({ method: filterValue(toggled(ledger.selectedMethods, method)) })
            }
            min={search.min}
            max={search.max}
            onAmount={(min, max) => update({ min, max })}
            sections={{
              categories: sectionControls('categories'),
              accounts: sectionControls('accounts'),
              methods: sectionControls('methods'),
              amount: sectionControls('amount'),
            }}
            onClear={clearAll}
            onClose={toggleFilters}
          />
        ) : null}
        <TransactionDialog
          activity={openActivity}
          data={data}
          onClose={closeDetails}
          onStep={(direction) => {
            const next = neighbor(openId, direction)
            if (next) setOpenId(next.id)
          }}
          onReviewed={
            search.review
              ? (id) => {
                  // A reviewed item leaves the queue, so the inspector moves on to the next one.
                  const next = neighbor(id, 1) ?? neighbor(id, -1)
                  if (next && next.id !== id) setOpenId(next.id)
                  else closeDetails()
                }
              : undefined
          }
        />
      </div>
    </div>
  )
}

/**
 * Activity's commands. They stay registered while the page renders, and run the page's
 * current handlers, so typing never re-registers them.
 */
function useActivityCommands(handlers: {
  ledger: ActivityLedger
  shown: ActivityItem[]
  filtersOpen: boolean
  focusedRow: string | undefined
  focusSearch: () => void
  update: (patch: LedgerPatch) => void
  toggleFilters: () => void
  toggleSort: (column: ActivitySort['column']) => void
  toggleReview: () => void
  clearAll: () => void
  exportCsv: () => void
  saveView: () => void
  toggleOther: (activity: ActivityItem) => void
}) {
  const latest = useLatest(handlers)
  const { filtersOpen, focusedRow, update, toggleFilters, toggleSort, toggleReview, clearAll } =
    handlers
  const actions = useMemo<AppAction[]>(
    () => [
      {
        id: 'activity.filters.focus',
        title: 'Search Activity',
        search: 'find text query',
        group: 'Here',
        scope: 'page',
        run: () => latest.current.focusSearch(),
      },
      {
        id: 'activity.filters.toggle',
        title: 'Show or hide filters',
        search: 'filter panel categories accounts dates amount hide exclude for other',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['f'], label: 'F' }],
        run: toggleFilters,
      },
      ...(filtersOpen
        ? [
            {
              id: 'activity.filters.close',
              title: 'Close filters',
              search: 'hide filter panel dismiss',
              group: 'View',
              scope: 'page' as const,
              bindings: [{ sequence: ['Escape'], label: 'Esc' }],
              run: toggleFilters,
            },
          ]
        : []),
      ...filterSections.flatMap(([section, noun]) =>
        (['all', 'clear'] as const).map((kind) => ({
          id: `activity.filters.${section}.${kind}`,
          title: kind === 'all' ? `Select every ${noun}` : `Clear ${noun} filters`,
          search: 'filter section reset select all',
          group: 'Here',
          scope: 'page' as const,
          run: () => update(latest.current.ledger.sections[section][kind]),
        })),
      ),
      {
        id: 'activity.filters.amount.clear',
        title: 'Clear the amount filter',
        search: 'filter section reset select all',
        group: 'Here',
        scope: 'page',
        run: () => update(latest.current.ledger.sections.amount.clear),
      },
      ...activityDateRangeOptions
        .filter(
          (option): option is typeof option & { value: Exclude<ActivityDateRange, 'custom'> } =>
            option.value !== 'custom',
        )
        .map((option) => ({
          id: `activity.range.${option.value}`,
          title: `Show ${option.accessibleLabel}`,
          search: `activity date range ${option.accessibleLabel}`,
          group: 'View',
          scope: 'page' as const,
          bindings: [{ sequence: [option.value], label: option.value.toUpperCase() }],
          repeatable: true,
          run: () => update(activityDatePreset(option.value, localDateKey(Date.now() / 1000))),
        })),
      {
        id: 'activity.review',
        title: 'Show activity to review',
        search: 'review flagged duplicate unusual second look queue',
        group: 'View',
        scope: 'page',
        run: toggleReview,
      },
      {
        id: 'activity.export',
        title: 'Download filtered activity as CSV',
        search: 'export csv download spreadsheet',
        group: 'Actions',
        scope: 'page',
        run: () => latest.current.exportCsv(),
      },
      {
        id: 'activity.views.save',
        title: 'Save this view',
        search: 'bookmark pin saved view filters',
        group: 'Actions',
        scope: 'page',
        run: () => latest.current.saveView(),
      },
      ...sortColumns.map(([column, label]) => ({
        id: `activity.sort.${column}`,
        title: `Sort by ${label}`,
        search: `order ledger column ${label}`,
        group: 'View',
        scope: 'page' as const,
        run: () => toggleSort(column),
      })),
      {
        id: 'activity.filters.clear',
        title: 'Clear all Activity filters',
        search: 'reset search filters',
        group: 'Here',
        scope: 'page',
        bindings: [{ sequence: ['c'], label: 'C' }],
        run: clearAll,
      },
      // Once a row has focus, the list's own motions take over the arrows.
      {
        id: 'activity.rows.first',
        title: 'Select the first activity',
        search: 'row highlight down arrow ledger',
        group: 'Here',
        scope: 'page',
        bindings: [{ sequence: ['ArrowDown'], label: '↓' }],
        run: () => {
          const row = document.querySelector<HTMLElement>('.activity-page-list [data-keyboard-row]')
          row?.focus()
          row?.scrollIntoView({ block: 'nearest' })
        },
      },
      // Only while a row has focus, so F otherwise still toggles filters.
      ...(focusedRow
        ? [
            {
              id: 'activity.row.for',
              title: 'Switch between Me and Other',
              search: 'for someone else family other me toggle',
              group: 'Here',
              scope: 'selection' as const,
              bindings: [{ sequence: ['f'], label: 'F' }],
              run: () => {
                const activity = latest.current.shown.find(({ id }) => id === focusedRow)
                if (activity?.other != null) latest.current.toggleOther(activity)
              },
            },
          ]
        : []),
    ],
    [clearAll, filtersOpen, focusedRow, latest, toggleFilters, toggleReview, toggleSort, update],
  )
  useRegisterActions(actions)
}
