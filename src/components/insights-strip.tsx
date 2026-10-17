import { useMemo } from 'react'

import { insightsViewSearch, insightsViews, type InsightsView } from '../lib/insights-views'
import type { FinanceSnapshot } from '../lib/schema'
import { tripYearTotals } from '../lib/trips'
import { SelectorStrip, StripIconMark, type SelectorStripItem } from './selector-strip'

/** Average monthly spending since the open chapter that began most recently. */
function nowSpending(insights: FinanceSnapshot['insights']): number | null {
  const open = (insights?.evidence?.chapters ?? [])
    .filter((chapter) => !chapter.end && chapter.kind !== 'planned')
    .toSorted((left, right) => right.start.localeCompare(left.start))
  const effect = open
    .flatMap((chapter) => chapter.effects)
    .find((candidate) => candidate.measure === 'spending')
  return effect ? -effect.after : null
}

/**
 * Insights' strip: each view as a card with its headline figure. Trips reports the year's
 * spending away; People, what was sent to people; Chapters, monthly spending since the
 * current chapter began; Rules, what rules recovered. Rust found and summed every figure.
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
    const year = Number(today.slice(0, 4))
    const figures: Record<
      InsightsView,
      Pick<SelectorStripItem, 'value' | 'unavailable' | 'change' | 'favorable'>
    > = {
      trips: {
        value: tripYearTotals(insights?.trips, year).spent,
        unavailable: !insights,
      },
      // What went to people through payment apps, all time.
      people: {
        value: insights?.people.sent ?? 0,
        unavailable: !insights,
      },
      // Monthly spending since the chapter you're in began.
      chapters: {
        value: nowSpending(insights),
        unavailable: nowSpending(insights) == null,
      },
      // What rules recovered: money from elsewhere filed as repaying card fees.
      rules: {
        value: [insights?.benefits, ...(insights?.otherCardBenefits ?? [])].reduce(
          (sum, card) => sum + (card?.fee?.offsets ?? 0),
          0,
        ),
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
