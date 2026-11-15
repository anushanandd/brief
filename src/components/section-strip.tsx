import { useLocation } from '@tanstack/react-router'

import { useFinance } from '../hooks/use-finance'
import {
  pageSections,
  primaryNavigation,
  sectionHome,
  sectionTabActive,
  type SectionTab,
} from '../lib/navigation'
import { taxYear } from '../lib/taxes'
import { tripYearTotals } from '../lib/trips'
import { SelectorStrip, StripIconMark, type SelectorStripItem } from './selector-strip'

type Figure = Pick<SelectorStripItem, 'value' | 'unavailable'>

/**
 * The strip of cards that heads each of Insights' pages, as Spending's reports have one: each
 * card is a page with its headline figure, which Rust computed. ⌘↑ and ⌘↓ step through it,
 * except where the page takes them: Documents for its years, and one open person for people.
 */
export function SectionStrip() {
  const pathname = useLocation({ select: (location) => location.pathname })
  const view = useLocation({ select: (location) => (location.search as { view?: unknown }).view })
  const person = useLocation({
    select: (location) => (location.search as { person?: unknown }).person,
  })
  const insights = useFinance().data?.insights
  const home = sectionHome(pathname)
  if (!home) return null
  const year = Number(insights?.today.slice(0, 4))
  const missing = !insights
  const estimate = taxYear(insights?.taxes, year)?.estimate
  const wages = insights?.documents.years.find((entry) => entry.year === year)?.income?.wages
  // Each figure is this year's: spent away, sent to people, the estimated tax, wages read.
  const figures: Record<SectionTab['id'], Figure> = {
    trips: { value: tripYearTotals(insights?.trips, year).spent, unavailable: missing },
    people: { value: insights?.people.sent ?? 0, unavailable: missing },
    taxes: { value: estimate?.high ?? null, unavailable: !estimate },
    documents: { value: wages ?? null, unavailable: wages == null },
  }
  const items = pageSections[home].map((tab): SelectorStripItem => {
    const figure = figures[tab.id]
    return {
      id: tab.id,
      label: tab.label,
      mark: (
        <StripIconMark
          icon={<tab.icon size={15} />}
          color={figure.unavailable ? 'var(--text-tertiary)' : tab.color}
        />
      ),
      link: { to: tab.to, search: tab.search ?? {} },
      current: sectionTabActive(tab, pathname, view),
      ...figure,
    }
  })
  const pageSteps =
    pathname === '/documents' || (pathname === '/insights' && typeof person === 'string')
  const label = primaryNavigation.find(({ to }) => to === home)?.label ?? 'Pages'
  return <SelectorStrip label={label} items={items} stepping={!pageSteps} />
}
