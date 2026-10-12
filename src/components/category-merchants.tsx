import { Link, type LinkProps } from '@tanstack/react-router'
import { useId, useState } from 'react'

import type { ActivityItem } from '../lib/activity'
import { formatActivityName, formatCurrency, pluralize } from '../lib/format'
import { ActivityList, ActivityMark } from './activity-list'
import { ArrowUpRight, ChevronRight, Repeat2 } from './icons'

const merchantLimit = 6

export type MerchantGroup = {
  name: string
  total: number
  activities: ActivityItem[]
  recurring: boolean
}

/** Spending activity grouped by merchant, largest total first, summed in cents. */
export function groupByMerchant(
  activities: ActivityItem[],
  recurringIds: ReadonlySet<string> = new Set(),
): MerchantGroup[] {
  const groups = new Map<string, { name: string; cents: number; activities: ActivityItem[] }>()
  for (const activity of activities) {
    const key = activity.title.trim().toLocaleLowerCase()
    const group = groups.get(key) ?? { name: activity.title, cents: 0, activities: [] }
    group.cents += Math.round(Math.abs(activity.amount) * 100)
    group.activities.push(activity)
    groups.set(key, group)
  }
  return [...groups.values()]
    .map(({ name, cents, activities: items }) => ({
      name,
      total: cents / 100,
      activities: items,
      recurring: items.some(({ id }) => recurringIds.has(id)),
    }))
    .toSorted((left, right) => right.total - left.total || left.name.localeCompare(right.name))
}

/** The merchants behind one category total; each opens to its own transactions. */
export function CategoryMerchants({
  merchants,
  referenceIso,
  link,
}: {
  merchants: MerchantGroup[]
  referenceIso: string
  link: Pick<LinkProps, 'to' | 'search'>
}) {
  const id = useId()
  const [open, setOpen] = useState<string | null>(null)
  const [showAll, setShowAll] = useState(false)
  const visible = showAll ? merchants : merchants.slice(0, merchantLimit)
  const largest = merchants[0]?.total || 1
  const count = merchants.reduce((sum, { activities }) => sum + activities.length, 0)

  return (
    <div className="category-merchants">
      <ul aria-label="Merchants">
        {visible.map((merchant, index) => {
          const expanded = open === merchant.name
          const panel = `${id}-${index}`
          return (
            <li key={merchant.name} data-open={expanded || undefined}>
              <button
                type="button"
                className="category-merchant-row"
                aria-expanded={expanded}
                aria-controls={panel}
                onClick={() => setOpen(expanded ? null : merchant.name)}
              >
                <ChevronRight className="ranked-bars-chevron" size={14} aria-hidden="true" />
                <ActivityMark activity={merchant.activities[0]} />
                <span className="category-merchant-name">
                  <span>{formatActivityName(merchant.name)}</span>
                  <small>
                    {merchant.activities.length} {pluralize(merchant.activities.length, 'charge')}
                    {merchant.recurring ? (
                      <>
                        {' · '}
                        <Repeat2 size={12} aria-hidden="true" /> Recurring
                      </>
                    ) : null}
                  </small>
                </span>
                <span className="category-merchant-share" aria-hidden="true">
                  <span style={{ width: `${(merchant.total / largest) * 100}%` }} />
                </span>
                <strong className="sensitive-value">{formatCurrency(merchant.total)}</strong>
              </button>
              {expanded ? (
                <div id={panel} className="category-merchant-activity">
                  <ActivityList
                    activities={merchant.activities}
                    referenceIso={referenceIso}
                    emptyMessage="No activity for this merchant."
                  />
                </div>
              ) : null}
            </li>
          )
        })}
      </ul>
      <div className="category-merchants-footer">
        {merchants.length > merchantLimit && !showAll ? (
          <button type="button" onClick={() => setShowAll(true)}>
            {merchants.length - merchantLimit} more merchants
          </button>
        ) : null}
        <Link className="ranked-bars-details-more" {...link}>
          {`All ${count} in Activity`}
          <ArrowUpRight size={12} aria-hidden="true" />
        </Link>
      </div>
    </div>
  )
}
