import { preference } from './preference'

/** Named Activity filter sets. They change only what is shown, so they stay in renderer storage. */
export const activityViewsKey = 'brief.activitySavedViews'
const maxViews = 12

export type ActivityView = { name: string; search: Record<string, string> }

const isStringRecord = (value: unknown): value is Record<string, string> =>
  typeof value === 'object' &&
  value !== null &&
  Object.values(value).every((field) => typeof field === 'string')

const isActivityView = (value: unknown): value is ActivityView =>
  typeof value === 'object' &&
  value !== null &&
  'name' in value &&
  typeof value.name === 'string' &&
  'search' in value &&
  isStringRecord(value.search)

export function parseActivityViews(raw: string | null): ActivityView[] {
  try {
    const parsed: unknown = JSON.parse(raw ?? '[]')
    if (!Array.isArray(parsed)) return []
    return parsed.filter(isActivityView).map(({ name, search }) => ({ name, search }))
  } catch {
    return []
  }
}

/** Adds or replaces a view by name, newest first. */
export function withActivityView(views: ActivityView[], view: ActivityView) {
  return [view, ...views.filter(({ name }) => name !== view.name)].slice(0, maxViews)
}

/** The saved views, newest first. */
export const activityViews = preference(activityViewsKey, parseActivityViews, JSON.stringify)

/** Keeps only the filter fields worth saving, as strings. */
export function activityViewSearch(search: Record<string, unknown>) {
  return Object.fromEntries(
    [
      'q',
      'account',
      'category',
      'exclude',
      'method',
      'for',
      'from',
      'to',
      'min',
      'max',
      'analysis',
    ].flatMap((key) =>
      typeof search[key] === 'number'
        ? [[key, String(search[key])]]
        : typeof search[key] === 'string' && search[key]
          ? [[key, search[key]]]
          : [],
    ),
  )
}
