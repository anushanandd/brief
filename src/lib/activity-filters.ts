import { activityMatchesSearch, activityMethod, type ActivityItem } from './activity'
import { graphWindows } from './graph-preferences'
import { transactionDateKey } from './spending'

const dayMs = 86_400_000
const activityDateFormatter = new Intl.DateTimeFormat('en-US', {
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
    !categories.length || categories.includes(activity.category || 'Other')
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

export const formatActivityDate = (date: string) =>
  activityDateFormatter.format(new Date(`${date}T00:00:00Z`))
export const filterValue = (values: string[]) => values.join(',') || undefined

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
    from: new Date(Date.parse(`${today}T00:00:00Z`) - (days - 1) * dayMs)
      .toISOString()
      .slice(0, 10),
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
