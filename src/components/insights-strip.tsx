import { useMemo } from 'react'

import { insightsViewSearch, insightsViews, type InsightsView } from '../lib/insights-views'
import type { FinanceSnapshot } from '../lib/schema'
import { tripYearTotals } from '../lib/trips'
import { SelectorStrip, StripIconMark, type SelectorStripItem } from './selector-strip'

/**
 * Insights' strip: each view as a card with its headline figure. Trips reports the year's
 * spending away; People, what was sent to people. Rust found every figure; this only adds a
 * list where a card needs one number.
 */
export function InsightsStrip({
  data,
  current,
  today,
}: {
  data: FinanceSnapshot
  current: InsightsView
  today: string
}) {
  const items = useMemo<SelectorStripItem[]>(() => {
    const insights = data.insights
    const year = today.slice(0, 4)
    const figures: Record<
      InsightsView,
      Pick<SelectorStripItem, 'value' | 'unavailable' | 'change' | 'favorable'>
    > = {
      trips: {
        value: tripYearTotals(
          (insights?.trips.trips ?? []).filter(({ start }) => start.startsWith(year)),
        ).spent,
        unavailable: !insights,
      },
      // What went to people through payment apps, all time, summed in cents.
      people: {
        value:
          (insights?.people.people ?? []).reduce(
            (sum, { sent }) => sum + Math.round(Math.abs(sent) * 100),
            0,
          ) / 100,
        unavailable: !insights,
      },
    }
    return insightsViews.map(({ id, label, icon: Icon, color }) => ({
      id,
      label,
      mark: (
        <StripIconMark
          icon={<Icon size={15} />}
          color={figures[id].unavailable ? 'var(--text-tertiary)' : color}
        />
      ),
      link: { to: '/insights', search: insightsViewSearch(id) },
      current: current === id,
      ...figures[id],
    }))
  }, [current, data, today])
  return <SelectorStrip label="Insights" items={items} />
}
