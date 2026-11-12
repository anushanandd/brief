import { useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'

import type { SelectionEdit } from '../components/activity-selection-bar'
import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useReports } from '../hooks/use-reports'
import { useSearchShortcuts } from '../hooks/use-search-shortcuts'
import type { ActivityItem, ActivitySort } from '../lib/activity'
import {
  activityDatePreset,
  activityFiltersOpen,
  filterValue,
  type ActivityDateRange,
} from '../lib/activity-filters'
import {
  activityLedger,
  activityRows,
  type LedgerPatch,
  type SectionPatches,
} from '../lib/activity-ledger'
import { activityViewSearch, activityViews, withActivityView } from '../lib/activity-views'
import { localDateKey } from '../lib/dates'
import { errorMessage } from '../lib/errors'
import type { FinanceSnapshot } from '../lib/schema'
import { nextSort } from '../lib/sort'

const toggled = (values: string[], value: string) =>
  values.includes(value) ? values.filter((item) => item !== value) : [...values, value]

/**
 * Activity's filters: the URL's search, the typed draft, the sort, and the saved views, with
 * the ledger Rust's rows make under them and every change a control can ask for. Filters live
 * in the URL, so Back undoes one and a saved view is a search.
 */
export function useActivityFilters(data: FinanceSnapshot) {
  const search = useSearch({ from: '/activities' })
  const navigate = useNavigate({ from: '/activities' })
  const update = useCallback(
    (patch: LedgerPatch) =>
      void navigate({ search: (current) => ({ ...current, ...patch }), replace: true }),
    [navigate],
  )
  // The filters card opens with a slide and closes at once, so the ledger reclaims its width
  // in one step instead of waiting for a fade.
  const filtersOpen = activityFiltersOpen.use()
  const [draft, setDraft] = useState('')
  const inputRef = useSearchShortcuts(draft, setDraft)
  const [sort, setSort] = useState<ActivitySort>({ column: 'date', direction: 'desc' })
  const savedViews = activityViews.use()

  // A report scopes the page to its entries on any day, as Rust chose them.
  const reportEntries = useReports(
    data,
    { chart: search.analysis, range: 'all' },
    Boolean(search.analysis),
  ).data?.report?.entries
  const valuesHidden = useAmountsHidden()
  const rows = useMemo(
    () => activityRows(data, { analysis: search.analysis, review: search.review }, reportEntries),
    // Rows carry formatted amounts, which screen privacy masks.
    [data, reportEntries, search.analysis, search.review, valuesHidden],
  )
  const today = localDateKey(Date.now() / 1000)
  const ledger = activityLedger({ data, rows, search, draft, sort, today })

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
  const saveView = () => {
    if (!ledger.viewName) return
    activityViews.save(
      withActivityView(savedViews, { name: ledger.viewName, search: activityViewSearch(search) }),
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
  const sectionControls = (name: 'categories' | 'accounts' | 'methods' | 'amount') => {
    const section: SectionPatches = ledger.sections[name]
    const all = section.canAll ? section.all : undefined
    return {
      command: `activity.filters.${name}`,
      all: all ? () => update(all) : undefined,
      clear: section.canClear ? () => update(section.clear) : undefined,
    }
  }

  return {
    search,
    navigate,
    update,
    filtersOpen,
    toggleFilters,
    draft,
    setDraft,
    inputRef,
    sort,
    toggleSort,
    savedViews,
    rows,
    ledger,
    clearAll,
    toggleReview,
    saveView,
    commitDraft,
    setDates,
    sectionControls,
    /** Adds a category from the facets above the list. */
    addCategory: (category: string) =>
      update({ category: filterValue([...ledger.selectedCategories, category]) }),
    // A category hidden elsewhere (a chip, a saved view) comes back; any other is chosen or
    // unchosen.
    includeCategory: (category: string) =>
      update(
        ledger.excludedCategories.includes(category)
          ? {
              exclude: filterValue(ledger.excludedCategories.filter((value) => value !== category)),
            }
          : { category: filterValue(toggled(ledger.selectedCategories, category)) },
      ),
    toggleAccount: (id: string) =>
      update({ account: filterValue(toggled(ledger.selectedAccounts, id)) }),
    toggleMethod: (method: string) =>
      update({ method: filterValue(toggled(ledger.selectedMethods, method)) }),
  }
}

/**
 * The rows chosen for a bulk edit, by activity ID, and the edit itself. Only ledger
 * transactions can be edited, and a row filtered away leaves the selection, so a bulk edit
 * only touches what is shown.
 */
export function useActivitySelection(data: FinanceSnapshot, shown: ActivityItem[]) {
  const { annotate } = useFinanceEdits()
  const [checked, setChecked] = useState<ReadonlySet<string>>(() => new Set())
  const [pending, setPending] = useState(false)
  const selected = useMemo(
    () => (checked.size ? shown.filter(({ id }) => checked.has(id)) : []),
    [checked, shown],
  )
  useEffect(() => {
    if (selected.length !== checked.size) setChecked(new Set(selected.map(({ id }) => id)))
  }, [checked.size, selected])
  const toggleChecked = useCallback((id: string) => {
    if (!id.startsWith('spending:')) return
    setChecked((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next
    })
  }, [])
  const toggleAllChecked = useCallback(() => {
    const editable = shown.filter(({ id }) => id.startsWith('spending:')).map(({ id }) => id)
    setChecked((current) =>
      editable.length && editable.every((id) => current.has(id)) ? new Set() : new Set(editable),
    )
  }, [shown])
  const clearChecked = useCallback(() => setChecked(new Set()), [])
  const editSelection = (edit: SelectionEdit) => {
    if (!selected.length || pending) return
    const changes = Object.fromEntries(
      selected.map((activity) => {
        const id = activity.id.slice('spending:'.length)
        if ('tag' in edit) {
          const tags = data.transactions.find((candidate) => candidate.id === id)?.tags ?? []
          return [id, { tags: tags.includes(edit.tag) ? tags : [...tags, edit.tag] }]
        }
        if ('person' in edit) {
          const people = data.transactions.find((candidate) => candidate.id === id)?.people ?? []
          return [id, { people: people.includes(edit.person) ? people : [...people, edit.person] }]
        }
        return [id, edit]
      }),
    )
    setPending(true)
    void annotate(changes)
      .then(() => toast.success(`Updated ${selected.length} transactions`))
      .catch((error: unknown) =>
        toast.error('Could not edit the selection', { description: errorMessage(error) }),
      )
      .finally(() => setPending(false))
  }
  return {
    checked,
    selected,
    pending,
    toggleChecked,
    toggleAllChecked,
    clearChecked,
    editSelection,
  }
}
