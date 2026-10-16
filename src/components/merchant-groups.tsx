import { useId, useState } from 'react'

import type { ActivityItem } from '../lib/activity'
import { countOf, displayCase, formatCurrency, valueTone } from '../lib/format'
import { ActivityList, ActivityMark } from './activity-list'
import { ChevronDown } from './icons'
import { EmptyState } from './ui'
import { DisclosedPanel } from './ui/presence'

export type MerchantGroup = { key: string; name: string; total: number; activities: ActivityItem[] }

/**
 * Activity grouped by merchant, summed in cents with refunds netting out, largest spending
 * first. Each group keeps its activities newest first.
 */
export function groupByMerchant(activities: ActivityItem[]): MerchantGroup[] {
  const groups = new Map<string, { name: string; cents: number; activities: ActivityItem[] }>()
  for (const activity of activities) {
    const key = activity.title.trim().toLocaleLowerCase()
    const group = groups.get(key) ?? { name: activity.title, cents: 0, activities: [] }
    group.cents += Math.round(activity.amount * 100)
    group.activities.push(activity)
    groups.set(key, group)
  }
  return [...groups]
    .map(([key, { name, cents, activities: items }]) => ({
      key,
      name,
      total: cents / 100,
      activities: items,
    }))
    .toSorted((left, right) => left.total - right.total || left.name.localeCompare(right.name))
}

/** What a merchant's activities are: purchases when every one is spending. */
const countLabel = (activities: ActivityItem[]) =>
  countOf(
    activities.length,
    activities.every(({ amount }) => amount < 0) ? 'purchase' : 'transaction',
  )

/** Where the money went: one row per merchant, largest first, opening to its activities. */
export function MerchantGroups({
  activities,
  referenceIso,
  emptyMessage,
  onOpen,
}: {
  activities: ActivityItem[]
  referenceIso: string
  emptyMessage: string
  /** Opens an activity's details. */
  onOpen?: (activity: ActivityItem) => void
}) {
  const id = useId()
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set())
  const toggle = (key: string) =>
    setExpanded((current) => {
      const next = new Set(current)
      if (!next.delete(key)) next.add(key)
      return next
    })
  const groups = groupByMerchant(activities)
  if (!groups.length) return <EmptyState>{emptyMessage}</EmptyState>

  return (
    <div className="merchant-groups" role="group" aria-label="Spending by merchant">
      {groups.map((group, index) => {
        // One activity has nothing to open into, so it reads as itself.
        if (group.activities.length === 1)
          return (
            <div key={group.key} className="merchant-group merchant-group-single">
              <ActivityList
                activities={group.activities}
                referenceIso={referenceIso}
                onOpen={onOpen}
              />
            </div>
          )
        const open = expanded.has(group.key)
        const panel = `${id}-${index}`
        return (
          <div key={group.key} className="merchant-group" data-open={open || undefined}>
            <button
              type="button"
              className="financial-activity-row merchant-group-row tree-parent"
              data-keyboard-row
              data-keyboard-open
              aria-expanded={open}
              aria-controls={panel}
              onClick={() => toggle(group.key)}
            >
              <ActivityMark activity={group.activities[0]} />
              <span className="financial-activity-copy">
                <span className="financial-activity-title" title={group.name}>
                  <strong>{displayCase(group.name)}</strong>
                </span>
                <small className="merchant-group-count">
                  {countLabel(group.activities)}
                  <ChevronDown size={12} aria-hidden="true" />
                </small>
              </span>
              <span className="financial-activity-meta">
                <strong className={valueTone(group.total)} data-copy-amount>
                  {formatCurrency(group.total)}
                </strong>
              </span>
            </button>
            <DisclosedPanel
              open={open}
              className="merchant-group-activities tree-children"
              id={panel}
            >
              <ActivityList
                activities={group.activities}
                referenceIso={referenceIso}
                onOpen={onOpen}
              />
            </DisclosedPanel>
          </div>
        )
      })}
    </div>
  )
}
