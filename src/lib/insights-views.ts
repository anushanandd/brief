import { BriefcaseBusiness, Plane, ScrollText, Users } from '../components/icons'

/** Insights' views, in strip order. Trips is the first and the default. */
export const insightsViews = [
  { id: 'trips', label: 'Trips', icon: Plane, color: 'var(--data-4)' },
  { id: 'people', label: 'People', icon: Users, color: 'var(--data-2)' },
  { id: 'chapters', label: 'Chapters', icon: BriefcaseBusiness, color: 'var(--data-1)' },
  { id: 'rules', label: 'Rules', icon: ScrollText, color: 'var(--data-3)' },
] as const
export type InsightsView = (typeof insightsViews)[number]['id']

export type InsightsSearch = {
  view?: InsightsView
  /** Trips: the calendar year, when not this one. */
  year?: number
  /** Trips: one trip, by its id. */
  trip?: string
  /** People: one person, by their id. */
  person?: string
  /** Chapters: one chapter, by its id. */
  chapter?: string
}

export const insightsView = (value: unknown): InsightsView | undefined =>
  insightsViews.find(({ id }) => id === value)?.id

/** The search a view opens on; Trips, the default, needs none. */
export const insightsViewSearch = (view: InsightsView): InsightsSearch =>
  view === 'trips' ? {} : { view }

export function insightsSearch(search: Record<string, unknown>): InsightsSearch {
  const view = insightsView(search.view)
  const year = Number(search.year)
  return {
    ...(view && view !== 'trips' ? { view } : {}),
    ...(Number.isInteger(year) && year >= 2000 && year <= 2100 ? { year } : {}),
    ...(typeof search.trip === 'string' && search.trip ? { trip: search.trip } : {}),
    ...(typeof search.person === 'string' && search.person.startsWith('person:')
      ? { person: search.person }
      : {}),
    ...(typeof search.chapter === 'string' && search.chapter ? { chapter: search.chapter } : {}),
  }
}
