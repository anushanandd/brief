import { CalendarDays, CreditCard, Landmark, Shapes, type IconComponent } from '../components/icons'
import { activityMethod, type ActivityItem } from './activity'
import { addDays } from './dates'
import { transactionDateKey } from './spending'

/** The `/` filter picker's fields; a choice becomes a chip in the palette's input. */
export type FilterField = 'account' | 'category' | 'method' | 'date'
export type FilterChip = { field: FilterField; value: string; label: string; to?: string }
/** The picker's step: choosing a field, then one of its values. */
export type FilterStep = 'field' | FilterField

export const filterFields: { field: FilterField; label: string; icon: IconComponent }[] = [
  { field: 'account', label: 'Account', icon: Landmark },
  { field: 'category', label: 'Category', icon: Shapes },
  { field: 'method', label: 'Method', icon: CreditCard },
  { field: 'date', label: 'Date', icon: CalendarDays },
]

/** Date choices for the `/` Date filter, ending today. */
export function datePresets(today: string) {
  return [
    { label: 'Today', from: today, to: today },
    { label: 'Past 7 days', from: addDays(today, -6), to: today },
    { label: 'Past 30 days', from: addDays(today, -29), to: today },
    { label: 'This month', from: `${today.slice(0, 8)}01`, to: today },
    { label: 'This year', from: `${today.slice(0, 5)}01-01`, to: today },
  ]
}

/** Whether an activity passes every filter chip. */
export function activityMatchesChips(
  activity: ActivityItem,
  chips: readonly FilterChip[],
  referenceIso: string,
) {
  return chips.every((chip) => {
    if (chip.field === 'account') return activity.accountId === chip.value
    if (chip.field === 'category') return (activity.category || 'Other') === chip.value
    if (chip.field === 'method') return activityMethod(activity) === chip.value
    const date = transactionDateKey(activity.date, referenceIso)
    return date >= chip.value && date <= (chip.to ?? chip.value)
  })
}

/** The Activity page's search for these chips. */
export function chipSearch(chips: readonly FilterChip[]): Record<string, string> {
  return Object.fromEntries(
    chips.flatMap((chip) =>
      chip.field === 'date'
        ? [
            ['from', chip.value],
            ['to', chip.to ?? chip.value],
          ]
        : [[chip.field, chip.value]],
    ),
  )
}

/** The query without a leading `field:` prefix, as Activity search reads it. */
export function activityTerm(query: string) {
  return query
    .replace(/^(description|account|category|method|location|amount|date):\s*/i, '')
    .trim()
}
