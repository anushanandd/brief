import { accountDisplayName } from './account-names'
import {
  activityInCategory,
  activityLocationLabel,
  activityMethod,
  buildActivities,
  linkedAccountIds,
  sortActivities,
  type ActivityEvidence,
  type ActivityFilterColumn,
  type ActivityItem,
  type ActivitySort,
} from './activity'
import {
  activityDateRangeOptions,
  activityDateRangeValue,
  activityFilterCandidates,
  filterValue,
  selectedFilterValues,
} from './activity-filters'
import {
  activityQuerySuggestions,
  amountMatches,
  completeActivityQuery,
  hasStructuredQuery,
  parseActivityQuery,
  settledActivityQuery,
  type ParsedActivityQuery,
} from './activity-query'
import { analyticsCharts, type AnalyticsChart } from './analytics'
import { financeSettings } from './finance-settings'
import { formatCurrency, formatDay } from './format'
import { getExternalLogosEnabled } from './logos'
import { placeLabel } from './places'
import type { FinanceSnapshot, ReportEntry } from './schema'
import { transactionDateKey } from './spending'

/** Activity's address: every filter, the open transaction, and the view it came from. */
export type LedgerSearch = {
  account?: string
  category?: string
  /** Categories hidden from the list, comma-separated. */
  exclude?: string
  method?: string
  q?: string
  from?: string
  to?: string
  min?: number
  max?: number
  analysis?: AnalyticsChart
  /** A transaction to open in the inspector, by its ID. */
  open?: string
  /** Only the activity Rust flagged for a second look that hasn't been reviewed. */
  review?: true
  /** Only your money, or only someone else's. */
  for?: 'me' | 'other'
  /** Only transactions with this standing against their evidence. */
  evidence?: ActivityEvidence
}

export const evidenceKinds: readonly ActivityEvidence[] = [
  'statement',
  'unlisted',
  'conflict',
  'unsure',
]

export const evidenceLabels: Record<ActivityEvidence, string> = {
  statement: 'On statement',
  unlisted: 'Not on statement',
  conflict: 'Sources disagree',
  unsure: 'Plaid unsure',
}

/** A change to the address; `undefined` clears a filter. */
export type LedgerPatch = Partial<LedgerSearch>

/** A filter section's All and Clear: where each moves, and whether it changes anything. */
export type SectionPatches = {
  all?: LedgerPatch
  clear: LedgerPatch
  canAll?: boolean
  canClear: boolean
}

/**
 * Every activity, the ones a report or the review queue scopes the page to (newest first),
 * and how many await review. Rebuilding activity is the costly part of the page, so it is
 * derived once per snapshot and scope, not on each keystroke.
 */
export function activityRows(
  data: FinanceSnapshot,
  search: LedgerSearch,
  /** The report's entries on any day, as Rust chose them, when a report scopes the page. */
  reportEntries: readonly ReportEntry[] = [],
) {
  const settings = financeSettings(data)
  const analyticEntries = search.analysis
    ? new Map(reportEntries.map((entry) => [entry.id, entry]))
    : undefined
  const allActivities = buildActivities(data, settings.accountNames, getExternalLogosEnabled())
  const activities = allActivities
    .filter(({ id }) => !analyticEntries || analyticEntries.has(id))
    .filter(({ flag }) => !search.review || flag)
    .map((activity) => {
      const scoped = analyticEntries
        ? { ...activity, date: analyticEntries.get(activity.id)!.date }
        : activity
      return { activity: scoped, date: transactionDateKey(scoped.date) }
    })
    .toSorted((a, b) => b.date.localeCompare(a.date))
    .map(({ activity }) => activity)
  return {
    allActivities,
    activities,
    reviewCount: allActivities.filter(({ flag }) => flag).length,
  }
}

export type ActivityRows = ReturnType<typeof activityRows>

const amountLabel = (min: number | undefined, max: number | undefined) =>
  min != null && max != null
    ? `${formatCurrency(min)}–${formatCurrency(max)}`
    : min != null
      ? `Over ${formatCurrency(min)}`
      : max != null
        ? `Under ${formatCurrency(max)}`
        : undefined

const dateLabel = (from: string | undefined, to: string | undefined, since = 'From') =>
  from && to
    ? from === to
      ? formatDay(from)
      : `${formatDay(from)} – ${formatDay(to)}`
    : from
      ? `${since} ${formatDay(from)}`
      : to
        ? `Through ${formatDay(to)}`
        : undefined

const union = (left: string[], right: string[]) => [...new Set([...left, ...right])]
const without = (values: string[], value: string) => values.filter((item) => item !== value)
const tally = (items: ActivityItem[], key: (activity: ActivityItem) => string) => {
  const counts = new Map<string, number>()
  for (const item of items) counts.set(key(item), (counts.get(key(item)) ?? 0) + 1)
  return counts
}
const categoryOf = ({ category }: ActivityItem) => category || 'Other'
// Trades are never anyone else's, so a For filter leaves them out.
const ownedBy = (activity: ActivityItem, owner: LedgerSearch['for']) =>
  !owner || (activity.other != null && activity.other === (owner === 'other'))
const backedBy = (activity: ActivityItem, evidence: LedgerSearch['evidence']) =>
  !evidence || Boolean(activity.evidence?.includes(evidence))

/**
 * Everything Activity shows for an address, a draft query, and a sort: its accounts and
 * filters, the results and their counts, the chips, breadcrumbs, and the address each control
 * moves to. Pure, so typing re-derives it without rebuilding activity.
 */
export function activityLedger({
  data,
  rows,
  search,
  draft,
  sort,
  today,
}: {
  data: FinanceSnapshot
  rows: ActivityRows
  search: LedgerSearch
  draft: string
  sort: ActivitySort
  today: string
}) {
  const settings = financeSettings(data)
  const allAccounts = data.accounts.filter(({ id }) => id !== 'all')
  const { activities } = rows
  const activityAccountIds = linkedAccountIds(
    activities.map(({ accountId }) => accountId),
    data.accountLinks,
  )
  const accounts = search.analysis
    ? allAccounts
    : allAccounts.filter(({ id }) => activityAccountIds.has(id))
  const accountNames = Object.fromEntries(
    allAccounts.map((account) => [
      account.id,
      accountDisplayName(account.id, account.name, settings.accountNames),
    ]),
  )
  const allCategories = [...new Set(activities.map(categoryOf))]
  const allMethods = [...new Set(activities.map(activityMethod).filter(Boolean))]
  const selectedAccounts = selectedFilterValues(
    search.account,
    accounts.map(({ id }) => id),
  )
  const selectedCategories = selectedFilterValues(search.category, allCategories)
  const selectedMethods = selectedFilterValues(search.method, allMethods)
  const excludedCategories = selectedFilterValues(search.exclude, allCategories)
  const { from, to, min, max } = search
  const text = search.q ?? ''

  // The draft previews on top of the committed filters; Enter commits it.
  const vocabulary = {
    accounts: accounts.map(({ id }) => ({ id, label: accountNames[id] })),
    categories: allCategories,
    methods: allMethods,
  }
  const suggestions = activityQuerySuggestions(draft, vocabulary)
  const parsed = parseActivityQuery(settledActivityQuery(draft, suggestions), vocabulary, today)
  const withCommitted = (query: ParsedActivityQuery) => ({
    accounts: union(selectedAccounts, query.accounts),
    categories: union(selectedCategories, query.categories),
    methods: union(selectedMethods, query.methods),
    from: query.from ?? from,
    to: query.to ?? to,
    min: query.min ?? min,
    max: query.max ?? max,
    owner: query.owner ?? search.for,
    q: [text, query.text].filter(Boolean).join(' '),
  })
  const effective = withCommitted(parsed)
  /** The address Enter commits: the draft, its unfinished word completed, as chips. */
  const committed = (
    query = suggestions.length && !/\s$/.test(draft)
      ? completeActivityQuery(draft, suggestions[0])
      : draft,
  ): LedgerPatch => {
    const final = withCommitted(parseActivityQuery(query, vocabulary, today))
    return {
      account: filterValue(final.accounts),
      category: filterValue(final.categories),
      method: filterValue(final.methods),
      from: final.from,
      to: final.to,
      min: final.min,
      max: final.max,
      for: final.owner,
      q: final.q || undefined,
    }
  }

  const effectiveAccountIds = search.analysis
    ? new Set(effective.accounts)
    : linkedAccountIds(effective.accounts, data.accountLinks)
  const candidates = activityFilterCandidates(
    activities,
    effective.q,
    effective.from,
    effective.to, // An account filter that matches nothing stays empty rather than showing everything.
    search.account || effective.accounts.length ? effectiveAccountIds : undefined,
    effective.categories,
    effective.methods,
  )
  const inAmount = ({ amount }: ActivityItem) => amountMatches(amount, effective.min, effective.max)
  // A hidden category hides a split that has a part in it.
  const shownCategory = (activity: ActivityItem) =>
    !excludedCategories.some((category) => activityInCategory(activity, category))
  const backed = (activity: ActivityItem) => backedBy(activity, search.evidence)
  const filtered = candidates.results.filter(
    (activity) =>
      inAmount(activity) &&
      ownedBy(activity, effective.owner) &&
      backed(activity) &&
      shownCategory(activity),
  )

  // Each filter's counts leave out its own choice, so a count is what choosing it would show.
  const ownerBase = candidates.results.filter(
    (activity) => inAmount(activity) && backed(activity) && shownCategory(activity),
  )
  const evidenceBase = candidates.results.filter(
    (activity) =>
      inAmount(activity) && ownedBy(activity, effective.owner) && shownCategory(activity),
  )
  const categoryCounts = tally(
    candidates.categories.filter(
      (activity) => inAmount(activity) && ownedBy(activity, effective.owner) && backed(activity),
    ),
    categoryOf,
  )
  const accountCounts = tally(
    candidates.accounts.filter(
      (activity) =>
        inAmount(activity) &&
        ownedBy(activity, effective.owner) &&
        backed(activity) &&
        shownCategory(activity),
    ),
    ({ accountId }) => accountId ?? '',
  )
  const methodCounts = tally(
    candidates.methods.filter(
      (activity) =>
        inAmount(activity) &&
        ownedBy(activity, effective.owner) &&
        backed(activity) &&
        shownCategory(activity),
    ),
    activityMethod,
  )
  // Options keep the order of all activity, so rows don't jump as counts change.
  const byVolume = (values: string[], key: (activity: ActivityItem) => string) => {
    const totals = tally(activities, key)
    return values.toSorted(
      (a, b) => (totals.get(b) ?? 0) - (totals.get(a) ?? 0) || a.localeCompare(b),
    )
  }
  const options = {
    categories: byVolume(allCategories, categoryOf).map((category) => ({
      value: category,
      label: category,
      count: categoryCounts.get(category) ?? 0,
    })),
    accounts: accounts.map(({ id }) => ({
      value: id,
      label: accountNames[id] ?? id,
      count: accountCounts.get(id) ?? 0,
    })),
    methods: byVolume(allMethods, activityMethod).map((method) => ({
      value: method,
      label: method,
      count: methodCounts.get(method) ?? 0,
    })),
    owners: {
      all: ownerBase.length,
      me: ownerBase.filter((activity) => ownedBy(activity, 'me')).length,
      other: ownerBase.filter((activity) => ownedBy(activity, 'other')).length,
    },
    evidence: {
      all: evidenceBase.length,
      statement: evidenceBase.filter((activity) => backedBy(activity, 'statement')).length,
      unlisted: evidenceBase.filter((activity) => backedBy(activity, 'unlisted')).length,
      conflict: evidenceBase.filter((activity) => backedBy(activity, 'conflict')).length,
      unsure: evidenceBase.filter((activity) => backedBy(activity, 'unsure')).length,
    },
  }

  const preview = hasStructuredQuery(parsed)
    ? [
        ...parsed.categories,
        ...parsed.accounts.map((id) => accountNames[id] ?? id),
        ...parsed.methods,
        ...(parsed.owner ? [parsed.owner === 'other' ? 'For other' : 'For me'] : []),
        ...[amountLabel(parsed.min, parsed.max)].filter((label) => label != null),
        ...[dateLabel(parsed.from, parsed.to, 'Since')].filter((label) => label != null),
        ...(parsed.text ? [`“${parsed.text}”`] : []),
      ]
    : []
  // Result-aware shortcuts: the categories that make up what is on screen now.
  const facets = [...tally(filtered, categoryOf).entries()]
    .filter(([name]) => !effective.categories.includes(name))
    .toSorted((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, 6)

  const amountFilterLabel = amountLabel(min, max)
  const analysisLabel = search.analysis
    ? analyticsCharts.find(({ id }) => id === search.analysis)!.label
    : undefined
  const dateFilterLabel = dateLabel(from, to)
  const activeFilterCount =
    selectedAccounts.length +
    selectedCategories.length +
    excludedCategories.length +
    selectedMethods.length +
    Number(Boolean(from || to)) +
    Number(min != null || max != null) +
    Number(Boolean(text)) +
    Number(Boolean(search.analysis)) +
    Number(Boolean(search.review)) +
    Number(Boolean(search.for)) +
    Number(Boolean(search.evidence))
  // The filters the card sets; search, a report, and the review view have their own controls.
  const cardFilterCount =
    activeFilterCount -
    Number(Boolean(text)) -
    Number(Boolean(search.analysis)) -
    Number(Boolean(search.review))

  type Chip = { id: string; label: string; value: string; patch: LedgerPatch }
  const chips: Chip[] = [
    ...(search.review
      ? [{ id: 'review', label: 'View', value: 'To review', patch: { review: undefined } }]
      : []),
    ...(analysisLabel
      ? [
          {
            id: 'analysis',
            label: 'Report',
            value: `${analysisLabel} · Posted dates`,
            patch: { analysis: undefined },
          },
        ]
      : []),
    ...selectedAccounts.map((id) => ({
      id: `account:${id}`,
      label: 'Account',
      value: accountNames[id] ?? id,
      patch: { account: filterValue(without(selectedAccounts, id)) },
    })),
    ...selectedCategories.map((category) => ({
      id: `category:${category}`,
      label: 'Category',
      value: category,
      patch: { category: filterValue(without(selectedCategories, category)) },
    })),
    ...excludedCategories.map((category) => ({
      id: `exclude:${category}`,
      label: 'Hide',
      value: category,
      patch: { exclude: filterValue(without(excludedCategories, category)) },
    })),
    ...selectedMethods.map((method) => ({
      id: `method:${method}`,
      label: 'Method',
      value: method,
      patch: { method: filterValue(without(selectedMethods, method)) },
    })),
    ...(search.for
      ? [
          {
            id: 'for',
            label: 'For',
            value: search.for === 'other' ? 'Other' : 'Me',
            patch: { for: undefined },
          },
        ]
      : []),
    ...(search.evidence
      ? [
          {
            id: 'evidence',
            label: 'Evidence',
            value: evidenceLabels[search.evidence],
            patch: { evidence: undefined },
          },
        ]
      : []),
    ...(dateFilterLabel
      ? [
          {
            id: 'date',
            label: 'Date',
            value: dateFilterLabel,
            patch: { from: undefined, to: undefined },
          },
        ]
      : []),
    ...(amountFilterLabel
      ? [
          {
            id: 'amount',
            label: 'Amount',
            value: amountFilterLabel,
            patch: { min: undefined, max: undefined },
          },
        ]
      : []),
    ...(text ? [{ id: 'text', label: 'Text', value: `“${text}”`, patch: { q: undefined } }] : []),
  ]

  // Each crumb keeps the filters before it, so going back up drops only the later ones.
  const breadcrumbs: Array<{ label: string; search: LedgerPatch }> = []
  const crumb = (label: string, patch: LedgerPatch) =>
    breadcrumbs.push({ label, search: { ...breadcrumbs.at(-1)?.search, ...patch } })
  if (search.review) crumb('To review', { review: true })
  if (analysisLabel) crumb(analysisLabel, { analysis: search.analysis })
  if (selectedAccounts.length)
    crumb(selectedAccounts.map((id) => accountNames[id]).join(', '), {
      account: filterValue(selectedAccounts),
    })
  if (selectedCategories.length)
    crumb(selectedCategories.join(', '), { category: filterValue(selectedCategories) })
  if (selectedMethods.length)
    crumb(selectedMethods.join(', '), { method: filterValue(selectedMethods) })
  if (dateFilterLabel) crumb(dateFilterLabel, { from, to })
  const viewName = [
    ...breadcrumbs.map(({ label }) => label),
    amountFilterLabel,
    text && `“${text}”`,
  ]
    .filter(Boolean)
    .join(' · ')

  // Each filter section's All and Clear; the card offers them only where they change something.
  const sections = {
    categories: {
      all: { category: filterValue(allCategories), exclude: undefined },
      clear: { category: undefined, exclude: undefined },
      canAll: selectedCategories.length < allCategories.length,
      canClear: Boolean(selectedCategories.length || excludedCategories.length),
    },
    accounts: {
      all: { account: filterValue(accounts.map(({ id }) => id)) },
      clear: { account: undefined },
      canAll: selectedAccounts.length < accounts.length,
      canClear: Boolean(selectedAccounts.length),
    },
    methods: {
      all: { method: filterValue(allMethods) },
      clear: { method: undefined },
      canAll: selectedMethods.length < allMethods.length,
      canClear: Boolean(selectedMethods.length),
    },
    amount: {
      clear: { min: undefined, max: undefined },
      canClear: min != null || max != null,
    },
  } satisfies Record<string, SectionPatches>

  /** An account filter, through the Plaid account a SnapTrade account is linked to. */
  const accountFilter = (accountId: string | undefined): LedgerPatch | undefined => {
    if (!accountId) return undefined
    const account =
      accounts.find(({ id }) => id === accountId)?.id ??
      Object.entries(data.accountLinks ?? {}).find(
        ([plaidId, snaptradeId]) => plaidId === accountId || snaptradeId === accountId,
      )?.[0]
    return account ? { account } : undefined
  }

  /**
   * The address a cell narrows to: its text, account, category, method, or day. A transfer's
   * location is the account on its other side.
   */
  const cellFilter = (
    column: ActivityFilterColumn,
    activity: ActivityItem,
  ): LedgerPatch | undefined => {
    if (column === 'description') return { q: activity.title || undefined }
    if (column === 'location') {
      if (activity.transfer) return accountFilter(activity.transfer.accountId)
      if (activity.place) return { q: placeLabel(activity.place) || undefined }
      const label = activityLocationLabel(activity.location)
      return { q: (label === 'Address' ? activity.location?.address : label) || undefined }
    }
    if (column === 'account') return accountFilter(activity.accountId)
    if (column === 'category') return { category: categoryOf(activity) }
    if (column === 'method') return { method: activityMethod(activity) }
    const day = transactionDateKey(activity.date)
    return day ? { from: day, to: day } : undefined
  }

  // The date slider spans the saved activity, first day to last.
  const days = activities
    .map(({ date }) => transactionDateKey(date))
    .filter(Boolean)
    .toSorted()
  const dateBounds = days.length ? { min: days[0], max: days[days.length - 1] } : undefined
  const datePresets = activityDateRangeOptions.flatMap((option) =>
    option.value === 'custom'
      ? []
      : [
          {
            value: option.value,
            label: option.accessibleLabel,
            short: option.value === 'a' ? 'All' : option.label,
            shortcut: option.value.toUpperCase(),
          },
        ],
  )

  return {
    accounts,
    accountNames,
    accountTypes: Object.fromEntries(allAccounts.map(({ id, type }) => [id, type])),
    accountInstitutions: Object.fromEntries(
      allAccounts.map(({ id, institution, domain }) => [id, { institution, domain }]),
    ),
    selectedAccounts,
    selectedCategories,
    selectedMethods,
    excludedCategories,
    selectedDateRange: activityDateRangeValue(from, to, today),
    datePresets,
    dateBounds,
    suggestions,
    preview,
    committed,
    filtered,
    sorted: sortActivities(filtered, sort),
    options,
    facets,
    chips,
    breadcrumbs,
    viewName,
    sections,
    cellFilter,
    activeFilterCount,
    cardFilterCount,
    dateFilterLabel,
  }
}

export type ActivityLedger = ReturnType<typeof activityLedger>
