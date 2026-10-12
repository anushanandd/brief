import {
  activityInCategory,
  activityMatchesSearch,
  activityMethod,
  type ActivityItem,
} from './activity'
import { addDays } from './dates'
import { graphWindows } from './graph-preferences'
import { preference } from './preference'
import { transactionDateKey } from './spending'

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

export function activityFilterCandidates(
  activities: ActivityItem[],
  referenceIso: string,
  search: string,
  from: string | undefined,
  to: string | undefined,
  accountIds: ReadonlySet<string> | undefined,
  categories: readonly string[],
  methods: readonly string[],
) {
  const searched = activities.filter(
    (activity) =>
      activityMatchesSearch(activity, search) &&
      activityMatchesDateRange(activity, referenceIso, from, to),
  )
  const inAccount = (activity: ActivityItem) =>
    !accountIds || (activity.accountId != null && accountIds.has(activity.accountId))
  const inCategory = (activity: ActivityItem) =>
    !categories.length || categories.some((category) => activityInCategory(activity, category))
  const inMethod = (activity: ActivityItem) =>
    !methods.length || methods.includes(activityMethod(activity))
  const accountActivities = searched.filter(inAccount)
  return {
    accounts: searched.filter((activity) => inCategory(activity) && inMethod(activity)),
    categories: accountActivities.filter(inMethod),
    methods: accountActivities.filter(inCategory),
    results: accountActivities.filter((activity) => inCategory(activity) && inMethod(activity)),
  }
}

/** Whether Activity shows its filters card; a remembered layout, not a filter. */
export const activityFiltersOpen = preference(
  'brief:activity-filters-open',
  (raw) => raw === 'true',
)

export const filterValue = (values: string[]) => values.join(',') || undefined
/** The values a comma-separated filter names that are still options. */
export const selectedFilterValues = (query: string | undefined, options: string[]) =>
  (query ?? '').split(',').filter((value) => options.includes(value))

export type ActivityDateRange = 'w' | 'm' | 'q' | 'y' | 'a' | 'custom'
const activityDateRangeKeys = ['w', 'm', 'q', 'y', 'a'] as const
export const activityDateRangeOptions = [
  ...graphWindows.map(({ label, settingsLabel }, index) => ({
    value: activityDateRangeKeys[index],
    label,
    accessibleLabel: settingsLabel,
  })),
  { value: 'custom' as const, label: 'Custom', accessibleLabel: 'Custom' },
]

export function activityDatePreset(preset: Exclude<ActivityDateRange, 'custom'>, today: string) {
  const days = graphWindows[activityDateRangeKeys.indexOf(preset)].secs / 86_400
  if (!days) return { from: undefined, to: undefined }
  return {
    from: addDays(today, 1 - days),
    to: today,
  }
}

export function activityDateRangeValue(
  from: string | undefined,
  to: string | undefined,
  today: string,
): ActivityDateRange {
  if (!from && !to) return 'a'
  return (
    activityDateRangeKeys.slice(0, -1).find((range) => {
      const preset = activityDatePreset(range, today)
      return from === preset.from && to === preset.to
    }) ?? 'custom'
  )
}
